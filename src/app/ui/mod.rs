pub mod canvas_view;
pub mod export_dialog;
pub mod text_editor;
pub mod toast;
pub mod toolbar;
pub mod top_left;

use crate::app::App;
use crate::shapes::Tool;
use canvas_view::render_central_canvas;
use export_dialog::render_export_dialog;
use text_editor::render_inline_text_editor;
use toast::render_toast_notification;
use toolbar::render_bottom_toolbar;
use top_left::render_top_left_controls;
use eframe::egui;
use std::time::Instant;

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "top_panel_collapsed", &self.top_panel_collapsed);
        if let Some(path) = &self.current_file_path {
            eframe::set_value(
                storage,
                "last_file_path",
                &path.to_string_lossy().to_string(),
            );
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_ui_events();

        #[cfg(target_os = "macos")]
        for path in crate::macos_open::take_pending() {
            if path.exists() && path.is_file() {
                self.open_kugel_file(&path, ctx);
            }
        }

        // Handle window close request with unsaved changes prompt
        if ctx.input(|i| i.viewport().close_requested()) {
            let empty_unsaved = self.canvas.shapes.is_empty() && self.current_file_path.is_none();
            if self.close_confirmed || !self.is_dirty || empty_unsaved {
                // Allow close
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);

                let confirm = rfd::MessageDialog::new()
                    .set_title("Unsaved Changes")
                    .set_description("Do you want to save the current board before exiting?")
                    .set_buttons(rfd::MessageButtons::YesNoCancel)
                    .show();

                match confirm {
                    rfd::MessageDialogResult::Yes => {
                        if self.save() {
                            self.close_confirmed = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    rfd::MessageDialogResult::No => {
                        self.close_confirmed = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    _ => {}
                }
            }
        }

        // Dynamic system theme transitions
        if let Some(sys_theme) = ctx.input(|i| i.raw.system_theme) {
            if self.last_system_theme != Some(sys_theme) {
                self.last_system_theme = Some(sys_theme);
                let wants_dark = sys_theme == egui::Theme::Dark;
                if wants_dark != self.dark_mode {
                    self.dark_mode = wants_dark;
                    if self.dark_mode {
                        if self.background_color == egui::Color32::from_gray(240) {
                            self.background_color = egui::Color32::from_rgb(20, 20, 23);
                        }
                    } else {
                        if self.background_color == egui::Color32::from_rgb(20, 20, 23) {
                            self.background_color = egui::Color32::from_gray(240);
                        }
                    }
                }
            }
        }

        // Global Paste Shortcut
        let has_paste = self.editing_text_index.is_none()
            && ctx.input(|i| {
                (i.modifiers.command || i.modifiers.ctrl) && i.key_released(egui::Key::V)
            });

        if has_paste {
            if self.try_paste_clipboard_image(ctx) {
                self.is_dirty = true;
            } else if let Some(mut shape) = self.copied_shape.clone() {
                self.canvas.history.push(self.canvas.shapes.clone());
                self.canvas.undo_history.clear();
                self.is_dirty = true;

                let target = self.paste_target_canvas(ctx);
                let center = shape.data.get_bounds().center();
                shape.data.translate(target - center);
                shape.id = self.canvas.next_id;
                self.canvas.next_id += 1;
                shape.data.load_textures(ctx, shape.id);

                self.canvas.shapes.push(shape);
                self.select_single(self.canvas.shapes.len() - 1);
                self.tool = Tool::Select;
                self.notification = Some(("Pasted shape".to_string(), Instant::now()));
            } else {
                self.paste_from_clipboard(ctx);
                self.is_dirty = true;
            }
        }

        let current_visuals_dark = ctx.style().visuals.dark_mode;
        if !self.style_applied || self.dark_mode != current_visuals_dark {
            self.style_applied = true;
            if self.dark_mode {
                ctx.set_visuals(egui::Visuals::dark());
            } else {
                ctx.set_visuals(egui::Visuals::light());
            }

            let mut style = (*ctx.style()).clone();
            style.visuals.window_corner_radius = egui::CornerRadius::same(12);
            style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(8);

            if !self.dark_mode {
                style.visuals.window_stroke = egui::Stroke::NONE;
                style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
                style.visuals.widgets.inactive.bg_stroke =
                    egui::Stroke::new(1.0, egui::Color32::from_gray(200));
            } else {
                style.visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(60));
                style.visuals.widgets.noninteractive.bg_stroke =
                    egui::Stroke::new(1.0, egui::Color32::from_gray(60));
                style.visuals.widgets.inactive.bg_stroke =
                    egui::Stroke::new(1.0, egui::Color32::from_gray(60));
            }
            ctx.set_style(style);
        }

        let icons = self.icons.clone().expect("Icons not initialized");

        let is_dark = ctx.style().visuals.dark_mode;
        let panel_bg = if is_dark {
            egui::Color32::from_black_alpha(200)
        } else {
            egui::Color32::from_white_alpha(225)
        };
        let panel_stroke = if is_dark {
            egui::Stroke::new(1.0, egui::Color32::from_gray(60))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_gray(180))
        };

        if let Some((_, time)) = &self.notification {
            if time.elapsed().as_secs() >= 3 {
                self.notification = None;
            }
        }

        render_top_left_controls(self, ctx, &icons, panel_bg, panel_stroke, is_dark);
        render_bottom_toolbar(self, ctx, &icons, panel_bg, panel_stroke, is_dark);
        render_central_canvas(self, ctx, is_dark);
        render_inline_text_editor(self, ctx);
        render_export_dialog(self, ctx);
        render_toast_notification(self, ctx);
    }
}
