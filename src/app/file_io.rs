use crate::app::App;
use crate::canvas::Canvas;
use crate::export::export_canvas_to_image;
use crate::image_utils::{compress_and_scale, fit_display_size, process_file_to_images};
use crate::markdown::{looks_like_markdown, strip_markdown};
use crate::shapes::{ShapeData, Tool};
use crate::net::client::{NetworkClient, SyncStatus};
use crate::net::crypto::CredentialsStore;
use crate::net::kugelsh::KugelCloudPointer;
use crate::state::CanvasState;
use eframe::egui;
use std::path::Path;
use std::time::Instant;

impl App {
    pub fn open_kugel_file(&mut self, path: &Path, ctx: &egui::Context) -> bool {
        let is_cloud = KugelCloudPointer::is_kugelsh_path(path);

        if is_cloud {
            if let Ok(pointer) = KugelCloudPointer::read_from_file(path) {
                let state = pointer.offline_snapshot;
                self.canvas.shapes = state.shapes;
                self.canvas.next_id = state.next_id;
                self.background_color = egui::Color32::from_rgba_unmultiplied(
                    state.background_color[0],
                    state.background_color[1],
                    state.background_color[2],
                    state.background_color[3],
                );
                self.zoom = state.zoom;
                self.pan_offset = egui::vec2(state.pan_offset[0], state.pan_offset[1]);
                self.dark_mode = state.dark_mode;
                self.canvas.load_textures(ctx);
                self.clear_selection();
                self.editing_text_index = None;
                self.current_file_path = Some(path.to_path_buf());
                self.is_dirty = false;

                ensure_local_server_running();
                let creds = CredentialsStore::load();
                self.net_client = Some(NetworkClient::connect(
                    pointer.sync.server_url,
                    pointer.sync.room_id,
                    creds.auth_token,
                    ctx.clone(),
                ));
                self.sync_status = SyncStatus::Connecting;

                self.dashboard.recents.add_recent(path, pointer.metadata.title.clone(), true);

                self.notification = Some((
                    format!("Opened cloud board: {}", pointer.metadata.title),
                    Instant::now(),
                ));
                return true;
            }
        } else if let Ok(json) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str::<CanvasState>(&json) {
                self.leave_room();
                self.canvas.shapes = state.shapes;
                self.canvas.next_id = state.next_id;
                self.background_color = egui::Color32::from_rgba_unmultiplied(
                    state.background_color[0],
                    state.background_color[1],
                    state.background_color[2],
                    state.background_color[3],
                );
                self.zoom = state.zoom;
                self.pan_offset = egui::vec2(state.pan_offset[0], state.pan_offset[1]);
                self.dark_mode = state.dark_mode;
                self.canvas.load_textures(ctx);
                self.clear_selection();
                self.editing_text_index = None;
                self.generate_missing_link_previews(ctx);
                self.current_file_path = Some(path.to_path_buf());
                self.is_dirty = false;
                self.net_client = None;
                self.sync_status = SyncStatus::Disconnected;

                let title = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                self.dashboard.recents.add_recent(path, title, false);

                self.notification = Some((
                    format!(
                        "Opened board: {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    Instant::now(),
                ));
                return true;
            }
        }
        false
    }

    pub fn save(&mut self) -> bool {
        if let Some(path) = self.current_file_path.clone() {
            self.save_to_path(&path)
        } else {
            self.save_file_dialog()
        }
    }

    pub fn new_board(&mut self) {
        let empty_unsaved = self.canvas.shapes.is_empty() && self.current_file_path.is_none();
        if self.is_dirty && !empty_unsaved {
            let confirm = rfd::MessageDialog::new()
                .set_title("Unsaved Changes")
                .set_description("Do you want to save the current board before creating a new one?")
                .set_buttons(rfd::MessageButtons::YesNoCancel)
                .show();
            match confirm {
                rfd::MessageDialogResult::Yes => {
                    if !self.save() {
                        return;
                    }
                }
                rfd::MessageDialogResult::No => {}
                _ => return,
            }
        }

        self.leave_room();
        self.canvas = Canvas::default();
        self.current_file_path = None;
        self.is_dirty = false;
        self.clear_selection();
        self.editing_text_index = None;
        self.zoom = 1.0;
        self.pan_offset = egui::Vec2::ZERO;
        self.notification = Some(("New board created".to_string(), Instant::now()));
    }

    pub fn save_to_path(&mut self, path: &Path) -> bool {
        let state = CanvasState {
            version: "1.0".to_string(),
            shapes: self.canvas.shapes.clone(),
            background_color: [
                self.background_color.r(),
                self.background_color.g(),
                self.background_color.b(),
                self.background_color.a(),
            ],
            zoom: self.zoom,
            pan_offset: [self.pan_offset.x, self.pan_offset.y],
            next_id: self.canvas.next_id,
            dark_mode: self.dark_mode,
        };

        if KugelCloudPointer::is_kugelsh_path(path) {
            let title = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let room_id = self
                .net_client
                .as_ref()
                .map(|n| n.room_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let server_url = self
                .net_client
                .as_ref()
                .map(|n| n.server_url.clone())
                .unwrap_or_else(|| "ws://127.0.0.1:8765".to_string());

            let pointer = KugelCloudPointer::new(
                room_id,
                server_url,
                "public_token".to_string(),
                title,
                state,
            );

            if pointer.save_atomic(path).is_ok() {
                self.current_file_path = Some(path.to_path_buf());
                self.is_dirty = false;
                self.notification = Some(("Saved cloud board (.kugelsh)".to_string(), Instant::now()));
                return true;
            }
        } else {
            // Atomic save for .kugel
            let tmp_path = path.with_extension("kugel.tmp");
            if let Ok(json) = serde_json::to_string_pretty(&state) {
                if std::fs::write(&tmp_path, json).is_ok() {
                    if std::fs::rename(&tmp_path, path).is_ok() {
                        self.current_file_path = Some(path.to_path_buf());
                        self.is_dirty = false;
                        self.notification = Some(("Saved board state successfully".to_string(), Instant::now()));
                        return true;
                    }
                }
            }
        }
        self.notification = Some(("Saving board state failed".to_string(), Instant::now()));
        false
    }

    pub fn save_file_dialog(&mut self) -> bool {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Kugel Mood Board", &["kugel"])
            .add_filter("Kugel Cloud Board", &["kugelsh"])
            .save_file()
        {
            return self.save_to_path(&path);
        }
        false
    }

    pub fn create_cloud_room(&mut self, ctx: &egui::Context) {
        let server_url = crate::net::protocol::default_server_url();
        if server_url.contains("127.0.0.1") || server_url.contains("localhost") {
            ensure_local_server_running();
        }
        let room_id = uuid::Uuid::new_v4().to_string();
        let creds = CredentialsStore::load();

        self.net_client = Some(NetworkClient::connect(
            server_url,
            room_id.clone(),
            creds.auth_token,
            ctx.clone(),
        ));
        self.sync_status = SyncStatus::Connecting;

        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("Cloud Board.kugelsh")
            .add_filter("Kugel Cloud Shared Board", &["kugelsh"])
            .save_file()
        {
            self.current_file_path = Some(path.clone());
            self.save_to_path(&path);
        }

        // If local canvas already has shapes, broadcast them to server
        for shape in self.canvas.shapes.clone() {
            self.broadcast_shape_create(&shape);
        }

        self.notification = Some((format!("Created collaborative room: {}", &room_id[..8.min(room_id.len())]), Instant::now()));
    }

    pub fn join_cloud_room(&mut self, room_link_or_id: &str, ctx: &egui::Context) {
        let (room_id, server_url) = crate::net::protocol::parse_room_link(room_link_or_id);
        if server_url.contains("127.0.0.1") || server_url.contains("localhost") {
            ensure_local_server_running();
        }
        let creds = CredentialsStore::load();

        self.net_client = Some(NetworkClient::connect(
            server_url,
            room_id.clone(),
            creds.auth_token,
            ctx.clone(),
        ));
        self.sync_status = SyncStatus::Connecting;
        self.notification = Some((format!("Joining room: {}", &room_id[..room_id.len().min(8)]), Instant::now()));
    }

    pub fn open_file_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Kugel Mood Board", &["kugel", "kugelsh"])
            .pick_file()
        {
            if !self.open_kugel_file(&path, ctx) {
                self.notification = Some((
                    "Opening board state failed: invalid file".to_string(),
                    Instant::now(),
                ));
            }
        }
    }

    pub fn export_file_dialog(&mut self) {
        let filter_name = if self.export_jpeg {
            "JPEG Image"
        } else {
            "PNG Image"
        };
        let ext = if self.export_jpeg { "jpg" } else { "png" };

        let default_name = if let Some(file_path) = &self.current_file_path {
            file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|stem| format!("{}.{}", stem, ext))
                .unwrap_or_else(|| format!("Untitled.{}", ext))
        } else {
            format!("Untitled.{}", ext)
        };

        if let Some(path) = rfd::FileDialog::new()
            .add_filter(filter_name, &[ext])
            .set_file_name(&default_name)
            .save_file()
        {
            match export_canvas_to_image(
                &self.canvas.shapes,
                self.background_color,
                self.export_scale,
                &path,
                self.export_jpeg,
                self.export_quality,
            ) {
                Ok(_) => {
                    self.show_export_dialog = false;
                    self.notification = Some((
                        "Canvas exported successfully".to_string(),
                        Instant::now(),
                    ));
                }
                Err(e) => {
                    self.notification = Some((
                        format!("Canvas export failed: {}", e),
                        Instant::now(),
                    ));
                }
            }
        }
    }

    pub fn try_paste_clipboard_image(&mut self, ctx: &egui::Context) -> bool {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(image) = clipboard.get_image() {
                if let Some(rgba) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                    image.width as u32,
                    image.height as u32,
                    image.bytes.into_owned(),
                ) {
                    let dynamic_img = image::DynamicImage::ImageRgba8(rgba);
                    if let Ok((compressed_bytes, size)) = compress_and_scale(dynamic_img) {
                        let target_canvas = self.paste_target_canvas(ctx);
                        self.place_images_in_row(vec![(compressed_bytes, size)], target_canvas, ctx);
                        self.notification = Some((
                            "Pasted image from clipboard".to_string(),
                            Instant::now(),
                        ));
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Ok(image) = clipboard.get_image() {
                    if let Some(rgba) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                        image.width as u32,
                        image.height as u32,
                        image.bytes.into_owned(),
                    ) {
                        let dynamic_img = image::DynamicImage::ImageRgba8(rgba);
                        match compress_and_scale(dynamic_img) {
                            Ok((compressed_bytes, size)) => {
                                let target_canvas = self.paste_target_canvas(ctx);
                                self.place_images_in_row(vec![(compressed_bytes, size)], target_canvas, ctx);
                                self.notification = Some((
                                    "Pasted image from clipboard".to_string(),
                                    Instant::now(),
                                ));
                            }
                            Err(e) => {
                                self.notification = Some((
                                    format!("Pasting image failed: {}", e),
                                    Instant::now(),
                                ));
                            }
                        }
                    }
                } else if let Ok(text) = clipboard.get_text() {
                    let mut file_images = Vec::new();
                    for line in text.lines() {
                        let clean = line.trim();
                        let path_str = if clean.starts_with("file://") {
                            clean.strip_prefix("file://").unwrap_or(clean)
                        } else {
                            clean
                        };
                        let decoded = path_str.replace("%20", " ");
                        let p = Path::new(&decoded);
                        if p.exists() && p.is_file() {
                            file_images.extend(process_file_to_images(p));
                        }
                    }

                    if !file_images.is_empty() {
                        let count = file_images.len();
                        let target_canvas = self.paste_target_canvas(ctx);
                        self.place_images_in_row(file_images, target_canvas, ctx);
                        self.notification = Some((
                            if count == 1 {
                                "Pasted image/PDF page from clipboard".to_string()
                            } else {
                                format!("Pasted {} images/PDF pages in a row", count)
                            },
                            Instant::now(),
                        ));
                        return;
                    }

                    let label_text = if looks_like_markdown(&text) {
                        strip_markdown(&text)
                    } else {
                        text
                    };
                    let center_canvas = self.paste_target_canvas(ctx);
                    let idx = self
                        .canvas
                        .add_text(center_canvas, label_text, self.selected_color);
                    if let Some(shape) = self.canvas.shapes.get_mut(idx) {
                        if let ShapeData::Text { max_width, .. } = &mut shape.data {
                            *max_width = Some(600.0);
                        }
                    }
                    if let Some(shape) = self.canvas.shapes.get(idx) {
                        self.broadcast_shape_create(shape);
                    }
                    self.check_and_spawn_title_preview_for_shape(idx, ctx);
                    self.is_dirty = true;
                    self.select_single(idx);
                    self.tool = Tool::Select;
                    self.notification = Some((
                        "Pasted text from clipboard".to_string(),
                        Instant::now(),
                    ));
                } else {
                    self.notification = Some((
                        "Clipboard does not contain image or text data".to_string(),
                        Instant::now(),
                    ));
                }
            }
            Err(e) => {
                self.notification = Some((
                    format!("Failed to open clipboard: {}", e),
                    Instant::now(),
                ));
            }
        }
    }

    pub fn import_image_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Image Files", &["png", "jpg", "jpeg", "webp", "gif"])
            .pick_file()
        {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    match compress_and_scale(img) {
                        Ok((compressed_bytes, size)) => {
                            let center_canvas = self.paste_target_canvas(ctx);
                            let idx =
                                self.canvas
                                    .add_image(center_canvas, compressed_bytes, size, ctx);
                            if let Some(shape) = self.canvas.shapes.get(idx) {
                                self.broadcast_shape_create(shape);
                            }
                            self.is_dirty = true;
                            self.select_single(idx);
                            self.tool = Tool::Select;
                            self.notification = Some((
                                "Imported image successfully".to_string(),
                                Instant::now(),
                            ));
                        }
                        Err(e) => {
                            self.notification = Some((
                                format!("Loading image failed: {}", e),
                                Instant::now(),
                            ));
                        }
                    }
                }
            }
        }
    }

    pub fn place_images_in_row(
        &mut self,
        images: Vec<(Vec<u8>, [f32; 2])>,
        target_pos: egui::Pos2,
        ctx: &egui::Context,
    ) {
        if images.is_empty() {
            return;
        }

        let display_items: Vec<_> = images
            .into_iter()
            .map(|(bytes, sz)| {
                let disp_sz = fit_display_size(sz, 200.0, 260.0);
                (bytes, disp_sz)
            })
            .collect();

        let gap = (4.0 / self.zoom).clamp(2.0, 8.0);
        let total_width: f32 = display_items.iter().map(|(_, sz)| sz[0]).sum::<f32>()
            + gap * (display_items.len().saturating_sub(1) as f32);

        let start_x = target_pos.x - (total_width / 2.0);
        let start_y = target_pos.y;

        self.clear_selection();
        let mut x_offset = 0.0;
        let mut first_added = None;

        for (compressed_bytes, size) in display_items {
            let center_pos = egui::pos2(start_x + x_offset + size[0] / 2.0, start_y);
            let idx = self.canvas.add_image(center_pos, compressed_bytes, size, ctx);
            self.selected_shape_indices.insert(idx);
            if first_added.is_none() {
                first_added = Some(idx);
            }
            if let Some(shape) = self.canvas.shapes.get(idx) {
                self.broadcast_shape_create(shape);
            }
            x_offset += size[0] + gap;
        }

        self.primary_selected = first_added;
        self.is_dirty = true;
        self.tool = Tool::Select;
        ctx.request_repaint();
    }
}

pub fn ensure_local_server_running() {
    use std::net::TcpStream;
    use std::time::Duration;
    if TcpStream::connect_timeout(&"127.0.0.1:8765".parse().unwrap(), Duration::from_millis(50)).is_err() {
        if let Ok(server) = crate::server::KugelServer::new_in_memory() {
            let addr: std::net::SocketAddr = "127.0.0.1:8765".parse().unwrap();
            server.spawn_in_background(addr);
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_place_images_in_row_selection() {
        let mut app = App::default();
        let ctx = egui::Context::default();
        let images = vec![
            (vec![1, 2, 3], [100.0, 100.0]),
            (vec![4, 5, 6], [100.0, 100.0]),
            (vec![7, 8, 9], [100.0, 100.0]),
        ];
        app.place_images_in_row(images, egui::pos2(0.0, 0.0), &ctx);
        assert_eq!(app.selected_shape_indices.len(), 3);
        assert!(app.selected_shape_indices.contains(&0));
        assert!(app.selected_shape_indices.contains(&1));
        assert!(app.selected_shape_indices.contains(&2));
        assert_eq!(app.primary_selected, Some(0));
        assert_eq!(app.tool, Tool::Select);
    }
}
