pub mod font;
pub mod file_io;
pub mod link_preview;
pub mod selection;
pub mod ui;

use crate::canvas::Canvas;
use crate::icons::Icons;
use crate::net::client::{NetworkClient, SyncStatus};
use crate::net::kugelsh::{KugelCloudPointer, LocalRoomCache};
use crate::net::protocol::{ClientMessage, RemoteUser, ServerMessage};
use crate::app::ui::dashboard::DashboardModal;
use crate::app::ui::presence::RemoteCursorState;
use crate::shapes::{Shape, Tool};
use crate::updater::{spawn_update_check, UiEvent, UpdateState};
use eframe::egui;
use font::setup_custom_fonts;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

pub struct App {
    pub canvas: Canvas,
    pub tool: Tool,
    pub selected_color: egui::Color32,
    pub stroke_width: f32,
    pub filled_shapes: bool,
    pub zoom: f32,
    pub pan_offset: egui::Vec2,
    pub use_grid: bool,
    pub background_color: egui::Color32,

    // Real-Time Collaboration & Sync state
    pub net_client: Option<NetworkClient>,
    pub my_user_id: Option<String>,
    pub remote_cursors: HashMap<String, RemoteCursorState>,
    pub remote_users: Vec<RemoteUser>,
    pub locked_shapes: HashMap<usize, String>,
    pub dashboard: DashboardModal,
    pub sync_status: SyncStatus,
    pub last_cursor_send: Option<Instant>,
    pub last_cursor_pos: Option<egui::Pos2>,

    // Selection/Transform state
    pub selected_shape_indices: HashSet<usize>,
    pub primary_selected: Option<usize>,
    pub is_resizing: Option<usize>, // 0=TL, 1=TR, 2=BL, 3=BR
    pub is_dragging_shape: bool,
    pub drag_start_pos: egui::Pos2,
    pub snap_correction: egui::Vec2,
    pub marquee_start: Option<egui::Pos2>,

    // Copy / Paste buffer
    pub copied_shape: Option<Shape>,

    // Recoloring selection drag state
    pub recoloring_selection: bool,

    // Text editing state
    pub editing_text_index: Option<usize>,
    pub editing_text_buffer: String,
    pub request_text_focus: bool,

    // Export overlay
    pub show_export_dialog: bool,
    pub export_scale: f32,
    pub export_jpeg: bool,
    pub export_quality: i32,

    // Notifications
    pub notification: Option<(String, Instant)>,

    // Theme state
    pub dark_mode: bool,
    pub style_applied: bool,
    pub last_system_theme: Option<egui::Theme>,

    // File state
    pub current_file_path: Option<PathBuf>,
    pub is_dirty: bool,
    pub close_confirmed: bool,

    // UI state
    pub top_panel_collapsed: bool,

    // Icons
    pub icons: Option<Icons>,

    // Update state
    pub update_state: UpdateState,
    pub ui_event_tx: mpsc::Sender<UiEvent>,
    pub ui_event_rx: mpsc::Receiver<UiEvent>,
}

impl Default for App {
    fn default() -> Self {
        let (ui_event_tx, ui_event_rx) = mpsc::channel();
        Self {
            canvas: Canvas::default(),
            tool: Tool::Select,
            selected_color: egui::Color32::from_rgb(99, 102, 241), // Indigo accent
            stroke_width: 3.0,
            filled_shapes: false,
            zoom: 1.0,
            pan_offset: egui::Vec2::ZERO,
            use_grid: true,
            background_color: egui::Color32::from_rgb(20, 20, 23),
            net_client: None,
            my_user_id: None,
            remote_cursors: HashMap::new(),
            remote_users: Vec::new(),
            locked_shapes: HashMap::new(),
            dashboard: DashboardModal::default(),
            sync_status: SyncStatus::Disconnected,
            last_cursor_send: None,
            last_cursor_pos: None,
            selected_shape_indices: HashSet::new(),
            primary_selected: None,
            is_resizing: None,
            is_dragging_shape: false,
            drag_start_pos: egui::Pos2::ZERO,
            snap_correction: egui::Vec2::ZERO,
            copied_shape: None,
            recoloring_selection: false,
            editing_text_index: None,
            editing_text_buffer: String::new(),
            request_text_focus: false,
            show_export_dialog: false,
            export_scale: 2.0,
            export_jpeg: false,
            export_quality: 90,
            notification: None,
            marquee_start: None,
            dark_mode: true,
            style_applied: false,
            last_system_theme: None,
            current_file_path: None,
            is_dirty: false,
            close_confirmed: false,
            top_panel_collapsed: false,
            icons: None,
            update_state: UpdateState::Idle,
            ui_event_tx,
            ui_event_rx,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);

        let system_theme = cc.egui_ctx.input(|i| i.raw.system_theme);
        let dark_mode = match system_theme {
            Some(egui::Theme::Light) => false,
            _ => true,
        };

        let top_panel_collapsed = cc
            .storage
            .and_then(|s| eframe::get_value(s, "top_panel_collapsed"))
            .unwrap_or(false);

        let mut app = Self {
            dark_mode,
            last_system_theme: system_theme,
            top_panel_collapsed,
            icons: Some(Icons::new(&cc.egui_ctx)),
            ..Self::default()
        };

        if !dark_mode {
            app.background_color = egui::Color32::from_gray(240);
        }

        let mut opened = false;
        if let Some(path_str) = std::env::args().nth(1) {
            let path = std::path::Path::new(&path_str);
            if path.exists() && path.is_file() {
                opened = app.open_kugel_file(path, &cc.egui_ctx);
            }
        }

        if !opened {
            if let Some(path) = cc
                .storage
                .and_then(|s| eframe::get_value::<String>(s, "last_file_path"))
            {
                let path = PathBuf::from(path);
                if path.is_file() {
                    app.open_kugel_file(&path, &cc.egui_ctx);
                }
            }
        }

        app.update_state = UpdateState::Checking;
        spawn_update_check(app.ui_event_tx.clone(), cc.egui_ctx.clone());

        app
    }

    pub fn check_for_updates(&mut self, ctx: &egui::Context) {
        self.update_state = UpdateState::Checking;
        spawn_update_check(self.ui_event_tx.clone(), ctx.clone());
    }

    pub fn perform_self_update(&mut self, download_url: String, ctx: &egui::Context) {
        self.update_state = UpdateState::Updating;
        let ui_tx = self.ui_event_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = crate::updater::do_self_update(&download_url);
            match res {
                Ok(()) => {
                    let _ = ui_tx.send(UiEvent::UpdateApplied);
                }
                Err(e) => {
                    let _ = ui_tx.send(UiEvent::UpdateInstallFailed(e));
                }
            }
            ctx_clone.request_repaint();
        });
    }

    pub fn apply_ui_events(&mut self) {
        while let Ok(event) = self.ui_event_rx.try_recv() {
            match event {
                UiEvent::UpdateAvailable {
                    version,
                    html_url,
                    download_url,
                } => {
                    self.update_state = UpdateState::UpdateAvailable {
                        version,
                        html_url,
                        download_url,
                    };
                }
                UiEvent::UpToDate => {
                    self.update_state = UpdateState::UpToDate;
                }
                UiEvent::UpdateCheckFailed(err) => {
                    self.update_state = UpdateState::Failed(err);
                }
                UiEvent::UpdateApplied => {
                    self.update_state = UpdateState::UpdateDone;
                    self.notification = Some((
                        "Update installed. Restart Kugel to use the new version.".to_string(),
                        Instant::now(),
                    ));
                }
                UiEvent::UpdateInstallFailed(err) => {
                    self.update_state = UpdateState::Failed(err.clone());
                    self.notification =
                        Some((format!("Update failed: {err}"), Instant::now()));
                }
                UiEvent::LinkTitleFetched { shape_id, url, title } => {
                    if let Some(shape) = self.canvas.shapes.iter_mut().find(|s| s.id == shape_id) {
                        if shape.data.link_url() == Some(&url) {
                            shape.data.set_link_title(Some(title));
                            self.is_dirty = true;
                            self.broadcast_shape_update(shape_id);
                        }
                    }
                }
            }
        }
    }

    pub fn poll_network_events(&mut self, ctx: &egui::Context) {
        if let Some(net) = &self.net_client {
            let messages = net.poll_messages();
            let has_messages = !messages.is_empty();
            for msg in messages {
                match msg {
                    ServerMessage::RoomState {
                        shapes,
                        users,
                        locked_shapes,
                        your_user_id,
                        ..
                    } => {
                        self.my_user_id = Some(your_user_id);
                        if !shapes.is_empty() {
                            self.canvas.shapes = shapes;
                            for shape in &mut self.canvas.shapes {
                                shape.data.load_textures(ctx, shape.id);
                            }
                            self.canvas.next_id = self.canvas.shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
                        } else if !self.canvas.shapes.is_empty() {
                            // Server room is empty, but client has local shapes. Push local shapes to server!
                            let local_shapes = self.canvas.shapes.clone();
                            for shape in &local_shapes {
                                self.broadcast_shape_create(shape);
                            }
                        }
                        self.remote_users = users;
                        self.locked_shapes = locked_shapes;
                        self.sync_status = SyncStatus::Live;
                    }
                    ServerMessage::UserJoined { user } => {
                        self.remote_users.retain(|u| u.id != user.id);
                        self.remote_users.push(user);
                    }
                    ServerMessage::UserLeft { user_id } => {
                        self.remote_users.retain(|u| u.id != user_id);
                        self.remote_cursors.remove(&user_id);
                        self.locked_shapes.retain(|_, owner| owner != &user_id);
                    }
                    ServerMessage::RemoteCursor {
                        user_id,
                        x,
                        y,
                        selected_ids,
                    } => {
                        self.remote_cursors.insert(
                            user_id.clone(),
                            RemoteCursorState {
                                user_id,
                                pos: egui::pos2(x, y),
                                selected_ids,
                                last_update: Instant::now(),
                            },
                        );
                    }
                    ServerMessage::LockGranted { shape_id, user_id } => {
                        self.locked_shapes.insert(shape_id, user_id);
                    }
                    ServerMessage::LockDenied { shape_id, owner_id } => {
                        self.notification = Some((
                            format!("Shape {shape_id} is locked by user {owner_id}"),
                            Instant::now(),
                        ));
                    }
                    ServerMessage::LockReleased { shape_id } => {
                        self.locked_shapes.remove(&shape_id);
                    }
                    ServerMessage::ShapeUpdated {
                        shape_id, data, ..
                    } => {
                        if let Some(shape) = self.canvas.shapes.iter_mut().find(|s| s.id == shape_id) {
                            shape.data = data;
                            shape.data.load_textures(ctx, shape.id);
                        }
                    }
                    ServerMessage::ShapeCreated { shape, .. } => {
                        let mut new_shape = shape;
                        new_shape.data.load_textures(ctx, new_shape.id);
                        self.canvas.next_id = self.canvas.next_id.max(new_shape.id + 1);
                        if !self.canvas.shapes.iter().any(|s| s.id == new_shape.id) {
                            self.canvas.shapes.push(new_shape);
                        }
                    }
                    ServerMessage::ShapesDeleted { shape_ids, .. } => {
                        self.canvas.shapes.retain(|s| !shape_ids.contains(&s.id));
                        self.selected_shape_indices.retain(|id| !shape_ids.contains(id));
                    }
                    ServerMessage::ShapesReordered { shape_ids, action, .. } => {
                        use crate::net::protocol::ZOrderAction;
                        match action {
                            ZOrderAction::BringToFront => {
                                let mut moved = Vec::new();
                                self.canvas.shapes.retain(|s| {
                                    if shape_ids.contains(&s.id) {
                                        moved.push(s.clone());
                                        false
                                    } else {
                                        true
                                    }
                                });
                                self.canvas.shapes.extend(moved);
                            }
                            ZOrderAction::SendToBack => {
                                let mut moved = Vec::new();
                                self.canvas.shapes.retain(|s| {
                                    if shape_ids.contains(&s.id) {
                                        moved.push(s.clone());
                                        false
                                    } else {
                                        true
                                    }
                                });
                                moved.extend(self.canvas.shapes.clone());
                                self.canvas.shapes = moved;
                            }
                            ZOrderAction::BringForward => {
                                for i in (0..self.canvas.shapes.len().saturating_sub(1)).rev() {
                                    if shape_ids.contains(&self.canvas.shapes[i].id) {
                                        self.canvas.shapes.swap(i, i + 1);
                                    }
                                }
                            }
                            ZOrderAction::SendBackward => {
                                for i in 1..self.canvas.shapes.len() {
                                    if shape_ids.contains(&self.canvas.shapes[i].id) {
                                        self.canvas.shapes.swap(i, i - 1);
                                    }
                                }
                            }
                        }
                    }
                    ServerMessage::Error { message } => {
                        self.notification = Some((format!("Sync Error: {message}"), Instant::now()));
                    }
                    _ => {}
                }
            }

            if has_messages {
                ctx.request_repaint();
            }

            self.remote_cursors.retain(|_, state| state.last_update.elapsed().as_secs() < 5);

            // Save live edits to local internal cache (~/.local/share/kugel/cache/<room_id>.json)
            if let Some(path) = &self.current_file_path {
                if KugelCloudPointer::is_kugelsh_path(path) {
                    let state = crate::state::CanvasState {
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
                    let _ = LocalRoomCache::save_cache(&net.room_id, &state);
                }
            }
        }
    }

    pub fn broadcast_shape_create(&self, shape: &Shape) {
        if let Some(net) = &self.net_client {
            net.send(ClientMessage::CreateShape {
                shape: shape.clone(),
            });
        }
    }

    pub fn broadcast_shape_update(&self, shape_id: usize) {
        if let Some(net) = &self.net_client {
            if let Some(shape) = self.canvas.shapes.iter().find(|s| s.id == shape_id) {
                net.send(ClientMessage::UpdateShape {
                    shape_id,
                    data: shape.data.clone(),
                });
            }
        }
    }

    pub fn broadcast_delete_shapes(&self, shape_ids: &[usize]) {
        if let Some(net) = &self.net_client {
            net.send(ClientMessage::DeleteShapes {
                shape_ids: shape_ids.to_vec(),
            });
        }
    }

    pub fn broadcast_reorder_shapes(&self, shape_ids: &[usize], action: crate::net::protocol::ZOrderAction) {
        if let Some(net) = &self.net_client {
            net.send(ClientMessage::ReorderShapes {
                shape_ids: shape_ids.to_vec(),
                action,
            });
        }
    }

    pub fn sync_canvas_diff(&self, old_shapes: &[crate::shapes::Shape]) {
        let old_map: std::collections::HashMap<usize, &crate::shapes::Shape> =
            old_shapes.iter().map(|s| (s.id, s)).collect();
        let new_map: std::collections::HashMap<usize, &crate::shapes::Shape> =
            self.canvas.shapes.iter().map(|s| (s.id, s)).collect();

        let deleted_ids: Vec<usize> = old_shapes
            .iter()
            .filter(|s| !new_map.contains_key(&s.id))
            .map(|s| s.id)
            .collect();
        if !deleted_ids.is_empty() {
            self.broadcast_delete_shapes(&deleted_ids);
        }

        for shape in &self.canvas.shapes {
            if let Some(old_shape) = old_map.get(&shape.id) {
                if old_shape.data != shape.data {
                    self.broadcast_shape_update(shape.id);
                }
            } else {
                self.broadcast_shape_create(shape);
            }
        }
    }

    pub fn leave_room(&mut self) {
        if let Some(net) = &self.net_client {
            net.send(ClientMessage::LeaveRoom);
        }
        self.net_client = None;
        self.remote_cursors.clear();
        self.remote_users.clear();
        self.locked_shapes.clear();
        self.sync_status = SyncStatus::Disconnected;
        self.notification = Some(("Disconnected from room".to_string(), Instant::now()));
    }
}
