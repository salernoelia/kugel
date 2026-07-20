mod canvas;
mod export;
mod shapes;

use canvas::Canvas;
use eframe::egui;
use shapes::{Shape, ShapeData, Tool};
use std::collections::HashSet;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Kugel"),
        ..Default::default()
    };

    eframe::run_native(
        "Kugel",
        options,
        Box::new(|cc| {
            // Apply custom premium visual styles to egui
            let ctx = &cc.egui_ctx;
            ctx.set_visuals(egui::Visuals::dark());

            let mut style = (*ctx.style()).clone();
            style.visuals.window_corner_radius = egui::CornerRadius::same(12);
            style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(8);
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            ctx.set_style(style);

            Ok(Box::new(App::new(cc)))
        }),
    )
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CanvasState {
    version: String,
    shapes: Vec<Shape>,
    background_color: [u8; 4],
    zoom: f32,
    pan_offset: [f32; 2],
    next_id: usize,
    #[serde(default = "default_true")]
    dark_mode: bool,
}

fn default_true() -> bool {
    true
}

struct App {
    canvas: Canvas,
    tool: Tool,
    selected_color: egui::Color32,
    stroke_width: f32,
    filled_shapes: bool,
    zoom: f32,
    pan_offset: egui::Vec2,
    use_grid: bool,
    background_color: egui::Color32,

    // Selection/Transform state
    selected_shape_indices: HashSet<usize>,
    primary_selected: Option<usize>, // The "main" shape for resize handles, copy, etc.
    is_resizing: Option<usize>, // Selected handle index: 0=TL, 1=TR, 2=BL, 3=BR
    is_dragging_shape: bool,
    drag_start_pos: egui::Pos2,
    marquee_start: Option<egui::Pos2>,

    // Copy / Paste buffer
    copied_shape: Option<Shape>,

    // Text editing state
    editing_text_index: Option<usize>,
    editing_text_buffer: String,

    // Export overlay
    show_export_dialog: bool,
    export_scale: f32,
    export_jpeg: bool,
    export_quality: i32,

    // Notifications
    notification: Option<(String, std::time::Instant)>,

    // Theme state
    dark_mode: bool,
    style_applied: bool,
    last_system_theme: Option<egui::Theme>,

    // File state
    current_file_path: Option<std::path::PathBuf>,
    is_dirty: bool,
    close_confirmed: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            canvas: Canvas::default(),
            tool: Tool::Select,
            selected_color: egui::Color32::from_rgb(99, 102, 241), // Indigo accent
            stroke_width: 3.0,
            filled_shapes: false,
            zoom: 1.0,
            pan_offset: egui::Vec2::ZERO,
            use_grid: true,
            background_color: egui::Color32::from_rgb(20, 20, 23), // Darker gray
            selected_shape_indices: HashSet::new(),
            primary_selected: None,
            is_resizing: None,
            is_dragging_shape: false,
            drag_start_pos: egui::Pos2::ZERO,
            copied_shape: None,
            editing_text_index: None,
            editing_text_buffer: String::new(),
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
        }
    }
}

impl App {
    /// Clear selection and select a single shape.
    fn select_single(&mut self, idx: usize) {
        self.selected_shape_indices.clear();
        self.selected_shape_indices.insert(idx);
        self.primary_selected = Some(idx);
    }

    /// Clear all selection.
    fn clear_selection(&mut self) {
        self.selected_shape_indices.clear();
        self.primary_selected = None;
    }

    /// Check if any shape is selected.
    fn has_selection(&self) -> bool {
        !self.selected_shape_indices.is_empty()
    }

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let system_theme = cc.egui_ctx.input(|i| i.raw.system_theme);
        let dark_mode = match system_theme {
            Some(egui::Theme::Light) => false,
            _ => true,
        };

        let mut app = Self {
            dark_mode,
            last_system_theme: system_theme,
            ..Self::default()
        };

        // Align canvas background default if system is light mode
        if !dark_mode {
            app.background_color = egui::Color32::from_gray(240);
        }

        // Check if a file path was passed as a command-line argument (for double-clicking files)
        if let Some(path_str) = std::env::args().nth(1) {
            let path = std::path::Path::new(&path_str);
            if path.exists() && path.is_file() {
                app.open_kugel_file(path, &cc.egui_ctx);
            }
        }

        app
    }

    fn open_kugel_file(&mut self, path: &std::path::Path, ctx: &egui::Context) -> bool {
        if let Ok(json) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str::<CanvasState>(&json) {
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
                self.notification = Some((
                    format!("Opened board: {}", path.file_name().unwrap_or_default().to_string_lossy()),
                    std::time::Instant::now(),
                ));
                return true;
            }
        }
        false
    }

    fn screen_to_canvas(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (screen_pos.x - self.pan_offset.x) / self.zoom,
            (screen_pos.y - self.pan_offset.y) / self.zoom,
        )
    }

    fn canvas_to_screen(&self, canvas_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            canvas_pos.x * self.zoom + self.pan_offset.x,
            canvas_pos.y * self.zoom + self.pan_offset.y,
        )
    }

    fn hit_test(&self, canvas_pos: egui::Pos2) -> Option<usize> {
        let tolerance = 5.0;
        for (idx, shape) in self.canvas.shapes.iter().enumerate().rev() {
            if shape.data.contains_point(canvas_pos, tolerance) {
                return Some(idx);
            }
        }
        None
    }

    fn get_handle_under_mouse(&self, shape_idx: usize, mouse_pos: egui::Pos2) -> Option<usize> {
        let shape = &self.canvas.shapes[shape_idx];
        let bounds = shape.data.get_bounds();
        let screen_bounds = egui::Rect::from_min_max(
            self.canvas_to_screen(bounds.min),
            self.canvas_to_screen(bounds.max),
        );
        let is_text_or_sticky = matches!(
            shape.data,
            ShapeData::Text { .. } | ShapeData::StickyNote { .. }
        );

        if is_text_or_sticky {
            // Only allow right_top (1) and right_bottom (3) handles
            let handles = [(1, screen_bounds.right_top()), (3, screen_bounds.right_bottom())];
            for &(h_idx, pos) in &handles {
                if mouse_pos.distance(pos) <= 8.0 {
                    return Some(h_idx);
                }
            }
        } else {
            let handle_positions = [
                screen_bounds.left_top(),     // 0
                screen_bounds.right_top(),    // 1
                screen_bounds.left_bottom(),  // 2
                screen_bounds.right_bottom(), // 3
            ];
            for (h_idx, &pos) in handle_positions.iter().enumerate() {
                if mouse_pos.distance(pos) <= 8.0 {
                    return Some(h_idx);
                }
            }
        }
        None
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle window close request with unsaved changes prompt
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.close_confirmed || !self.is_dirty {
                // Allow close
            } else {
                // Intercept close
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
                    _ => {} // Cancel -> keeps window open
                }
            }
        }
        // Auto-detect and switch to match system theme transitions dynamically
        if let Some(sys_theme) = ctx.input(|i| i.raw.system_theme) {
            if self.last_system_theme != Some(sys_theme) {
                self.last_system_theme = Some(sys_theme);
                let wants_dark = sys_theme == egui::Theme::Dark;
                if wants_dark != self.dark_mode {
                    self.dark_mode = wants_dark;
                    // Automatically adjust default background color to match
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

        // Global Paste Shortcut (checked at the top to avoid widget consumption)
        // Combine all checks in a single ctx.input closure so events are read atomically.
        let has_paste = self.editing_text_index.is_none() && ctx.input(|i| {
            let cmd_v = (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::V);
            let paste_event = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
            cmd_v || paste_event
        });

        if has_paste {
            if let Some(mut shape) = self.copied_shape.clone() {
                self.canvas.history.push(self.canvas.shapes.clone());
                self.canvas.undo_history.clear();
                self.is_dirty = true;

                shape.data.translate(egui::vec2(20.0, 20.0));
                shape.id = self.canvas.next_id;
                self.canvas.next_id += 1;
                shape.data.load_textures(ctx, shape.id);

                self.canvas.shapes.push(shape);
                self.select_single(self.canvas.shapes.len() - 1);
                self.tool = Tool::Select; // Auto-switch!
                self.notification = Some(("Pasted shape".to_string(), std::time::Instant::now()));
            } else {
                self.paste_from_clipboard(ctx);
                self.is_dirty = true;
            }
        }
        // Apply custom styles and handle theme toggling dynamically
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

            // General border cleanup to prevent weird white/light borders in light mode:
            if !self.dark_mode {
                style.visuals.window_stroke = egui::Stroke::NONE;
                style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
                style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(200));
            } else {
                style.visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(60));
                style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(60));
                style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(60));
            }
            ctx.set_style(style);
        }

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

        // Notification banner check
        if let Some((_, time)) = &self.notification {
            if time.elapsed().as_secs() >= 3 {
                self.notification = None;
            }
        }

        // 1. TOP-LEFT CONTROLS PANEL
        egui::Area::new(egui::Id::new("top_left_controls"))
            .anchor(egui::Align2::LEFT_TOP, [20.0, 20.0])
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(panel_bg)
                    .stroke(panel_stroke)
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Bg Color:");
                                egui::color_picker::color_edit_button_srgba(
                                    ui,
                                    &mut self.background_color,
                                    egui::color_picker::Alpha::Opaque,
                                );
                            });
                            ui.checkbox(&mut self.use_grid, "Show Grid");
                            ui.separator();
                            ui.horizontal(|ui| {
                                let theme_icon = if self.dark_mode { "🌙 Dark" } else { "☀ Light" };
                                if ui.button(theme_icon).clicked() {
                                    self.dark_mode = !self.dark_mode;
                                    // Smoothly toggle background color if default
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
                                if ui.button("Reset View").clicked() {
                                    self.zoom = 1.0;
                                    self.pan_offset = egui::Vec2::ZERO;
                                }
                            });
                        });
                    });
            });



        // 3. FLOATING BOTTOM TOOLBAR
        egui::Area::new(egui::Id::new("bottom_toolbar"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -20.0])
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(panel_bg)
                    .stroke(panel_stroke)
                    .corner_radius(egui::CornerRadius::same(14))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Tools
                            let tools = [
                                (Tool::Select, "➤ Select"),
                                (Tool::Pen, "✏ Pen"),
                                (Tool::Rectangle, "▭ Rect"),
                                (Tool::Circle, "○ Circle"),
                                (Tool::Text, "🖹 Text"),
                                (Tool::StickyNote, "📝 Note"),
                            ];
                            for &(t, label) in &tools {
                                let selected = self.tool == t;
                                if ui.selectable_label(selected, label).clicked() {
                                    self.tool = t;
                                    self.clear_selection();
                                    self.editing_text_index = None;
                                }
                            }

                            ui.separator();

                            if ui.button("🖼 Import").clicked() {
                                self.import_image_dialog(ctx);
                            }

                            ui.separator();

                            // Colors & Properties
                            ui.label("Size:");
                            ui.add(egui::Slider::new(&mut self.stroke_width, 1.0..=20.0).show_value(false));

                            ui.label("Color:");
                            egui::color_picker::color_edit_button_srgba(
                                ui,
                                &mut self.selected_color,
                                egui::color_picker::Alpha::Opaque,
                            );

                            ui.checkbox(&mut self.filled_shapes, "Fill");

                            ui.separator();

                            // Undo / Redo
                            if ui.button("⮪").clicked() {
                                self.canvas.undo();
                                self.clear_selection();
                                self.editing_text_index = None;
                                self.is_dirty = true;
                            }
                            if ui.button("⮫").clicked() {
                                self.canvas.redo();
                                self.clear_selection();
                                self.editing_text_index = None;
                                self.is_dirty = true;
                            }
                            if ui.button("🗑 Clear").clicked() {
                                self.canvas.clear();
                                self.clear_selection();
                                self.editing_text_index = None;
                                self.is_dirty = true;
                            }

                            ui.separator();

                            // File & Export
                            if ui.button("💾 Save").clicked() {
                                self.save();
                            }
                            if ui.button("📁 Open").clicked() {
                                self.open_file_dialog(ctx);
                            }
                            if ui.button("📤 Export").clicked() {
                                self.show_export_dialog = true;
                            }
                        });
                    });
            });

        // 4. CENTRAL CANVAS PANEL
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
            let (response, mut painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

            // Render background
            painter.rect_filled(response.rect, 0.0, self.background_color);

            // Draw grid dots
            if self.use_grid {
                let grid_spacing = 50.0 * self.zoom;
                let grid_color = if is_dark {
                    egui::Color32::from_gray(95).gamma_multiply(0.45)
                } else {
                    egui::Color32::from_gray(130).gamma_multiply(0.6)
                };

                if grid_spacing > 8.0 {
                    let min_x = ((response.rect.min.x - self.pan_offset.x) / grid_spacing).floor()
                        * grid_spacing
                        + self.pan_offset.x;
                    let min_y = ((response.rect.min.y - self.pan_offset.y) / grid_spacing).floor()
                        * grid_spacing
                        + self.pan_offset.y;

                    let mut y = min_y;
                    while y < response.rect.max.y {
                        let mut x = min_x;
                        while x < response.rect.max.x {
                            painter.circle_filled(egui::pos2(x, y), 1.0, grid_color);
                            x += grid_spacing;
                        }
                        y += grid_spacing;
                    }
                }
            }

            // Zoom camera updates (direct scroll wheel zoom)
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            let zoom_delta = ui.input(|i| i.zoom_delta());

            if zoom_delta != 1.0 || scroll_delta.y != 0.0 {
                let pointer_pos = response.hover_pos().unwrap_or(response.rect.center());
                let zoom_factor = if zoom_delta != 1.0 {
                    zoom_delta
                } else {
                    1.0 + scroll_delta.y * 0.003
                };

                if zoom_factor != 1.0 {
                    let old_zoom = self.zoom;
                    self.zoom = (self.zoom * zoom_factor).clamp(0.1, 10.0);

                    let zoom_change = self.zoom / old_zoom;
                    self.pan_offset = pointer_pos.to_vec2()
                        + (self.pan_offset - pointer_pos.to_vec2()) * zoom_change;
                }
            }

            // Keyboard Shortcuts
            let has_shortcut = |ui: &egui::Ui, key: egui::Key, cmd: bool| {
                ui.input(|i| {
                    i.events.iter().any(|e| match e {
                        egui::Event::Key { key: k, pressed: true, modifiers, .. } if *k == key => {
                            !cmd || modifiers.command || modifiers.ctrl
                        }
                        egui::Event::Paste(_) if key == egui::Key::V && cmd => true,
                        egui::Event::Copy if key == egui::Key::C && cmd => true,
                        _ => false,
                    })
                })
            };

            if self.editing_text_index.is_none() {
                // Helper: bare key press without any modifier (Cmd/Ctrl/Alt/Shift).
                // This prevents Cmd+V from triggering the "V" tool-switch, etc.
                let bare_key = |ui: &egui::Ui, key: egui::Key| -> bool {
                    ui.input(|i| {
                        i.key_pressed(key)
                            && !i.modifiers.command
                            && !i.modifiers.ctrl
                            && !i.modifiers.alt
                    })
                };

                if bare_key(ui, egui::Key::V) {
                    self.tool = Tool::Select;
                    self.clear_selection();
                }
                if bare_key(ui, egui::Key::P) {
                    self.tool = Tool::Pen;
                    self.clear_selection();
                }
                if bare_key(ui, egui::Key::R) {
                    self.tool = Tool::Rectangle;
                    self.clear_selection();
                }
                if bare_key(ui, egui::Key::O) {
                    self.tool = Tool::Circle;
                    self.clear_selection();
                }
                if bare_key(ui, egui::Key::T) {
                    self.tool = Tool::Text;
                    self.clear_selection();
                }
                if bare_key(ui, egui::Key::N) {
                    self.tool = Tool::StickyNote;
                    self.clear_selection();
                }

                if bare_key(ui, egui::Key::I) {
                    self.import_image_dialog(ctx);
                }
            }

            if has_shortcut(ui, egui::Key::Z, true) {
                self.canvas.undo();
                self.clear_selection();
                self.editing_text_index = None;
                self.is_dirty = true;
            }
            if has_shortcut(ui, egui::Key::Y, true) {
                self.canvas.redo();
                self.clear_selection();
                self.editing_text_index = None;
                self.is_dirty = true;
            }
            if has_shortcut(ui, egui::Key::S, true) {
                self.save();
            }
            if has_shortcut(ui, egui::Key::O, true) {
                self.open_file_dialog(ctx);
            }
            if has_shortcut(ui, egui::Key::E, true) {
                self.show_export_dialog = true;
            }

            // Duplicate selection (Cmd/Ctrl + D)
            if has_shortcut(ui, egui::Key::D, true) {
                if let Some(&idx) = self.primary_selected.as_ref() {
                    if idx < self.canvas.shapes.len() {
                        self.canvas.history.push(self.canvas.shapes.clone());
                        self.canvas.undo_history.clear();
                        self.is_dirty = true;

                        let mut dup = self.canvas.shapes[idx].clone();
                        dup.data.translate(egui::vec2(20.0, 20.0));
                        dup.id = self.canvas.next_id;
                        self.canvas.next_id += 1;
                        dup.data.load_textures(ctx, dup.id);

                        self.canvas.shapes.push(dup);
                        self.select_single(self.canvas.shapes.len() - 1);
                        self.notification = Some(("Duplicated selection".to_string(), std::time::Instant::now()));
                    }
                }
            }

            // Copy selection (Cmd/Ctrl + C)
            if has_shortcut(ui, egui::Key::C, true) {
                if let Some(&idx) = self.primary_selected.as_ref() {
                    if idx < self.canvas.shapes.len() {
                        self.copied_shape = Some(self.canvas.shapes[idx].clone());
                        self.notification = Some(("Copied shape to buffer".to_string(), std::time::Instant::now()));
                    }
                }
            }



            // Delete Selection (supports multi-select)
            if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                if self.editing_text_index.is_none() && self.has_selection() {
                    self.canvas.history.push(self.canvas.shapes.clone());
                    self.canvas.undo_history.clear();
                    self.is_dirty = true;
                    // Remove in reverse order to keep indices valid
                    let mut indices: Vec<usize> = self.selected_shape_indices.iter().copied().collect();
                    indices.sort_unstable_by(|a, b| b.cmp(a));
                    for idx in indices {
                        if idx < self.canvas.shapes.len() {
                            self.canvas.shapes.remove(idx);
                        }
                    }
                    self.clear_selection();
                    self.notification = Some(("Deleted shape(s)".to_string(), std::time::Instant::now()));
                }
            }

            // Selection nudge controls (Arrow keys) — nudges ALL selected shapes
            if self.editing_text_index.is_none() && self.has_selection() {
                let shift = ui.input(|i| i.modifiers.shift);
                let dist = if shift { 10.0 } else { 1.0 };
                let mut nudge_delta = egui::Vec2::ZERO;

                if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) { nudge_delta.x = -dist; }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) { nudge_delta.x = dist; }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) { nudge_delta.y = -dist; }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) { nudge_delta.y = dist; }

                if nudge_delta != egui::Vec2::ZERO {
                    self.canvas.history.push(self.canvas.shapes.clone());
                    self.canvas.undo_history.clear();
                    self.is_dirty = true;
                    for &idx in &self.selected_shape_indices {
                        if idx < self.canvas.shapes.len() {
                            self.canvas.shapes[idx].data.translate(nudge_delta);
                        }
                    }
                }
            }

            // Drag and drop images/boards pipeline
            let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
            if !dropped_files.is_empty() {
                for file in dropped_files {
                    if let Some(path) = &file.path {
                        if path.extension().map_or(false, |ext| ext == "kugel") {
                            // Dropped a Kugel file! Prompt to save current board first
                            let mut proceed = true;
                            if !self.canvas.shapes.is_empty() {
                                let confirm = rfd::MessageDialog::new()
                                    .set_title("Unsaved Changes")
                                    .set_description("Do you want to save your current board first?")
                                    .set_buttons(rfd::MessageButtons::YesNoCancel)
                                    .show();
                                
                                match confirm {
                                    rfd::MessageDialogResult::Yes => {
                                        proceed = self.save();
                                    }
                                    rfd::MessageDialogResult::No => {
                                        proceed = true;
                                    }
                                    _ => {
                                        proceed = false; // Cancel or close aborts
                                    }
                                }
                            }

                            if proceed {
                                self.open_kugel_file(path, ctx);
                            }
                        } else {
                            // Try loading as image...
                            if let Ok(bytes) = std::fs::read(path) {
                                if let Ok(img) = image::load_from_memory(&bytes) {
                                    if let Ok((compressed_bytes, size)) = self.compress_and_scale(img) {
                                        let center_screen = ctx.screen_rect().center();
                                        let center_canvas = self.screen_to_canvas(center_screen);
                                        let idx = self.canvas.add_image(center_canvas, compressed_bytes, size, ctx);
                                        self.is_dirty = true;
                                        self.select_single(idx);
                                        self.tool = Tool::Select; // Auto-switch!
                                        self.notification = Some(("Imported image".to_string(), std::time::Instant::now()));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Handle middle click or space key panning
            let is_panning = ui.input(|i| {
                i.pointer.middle_down()
                    || (i.key_down(egui::Key::Space) && i.pointer.primary_down())
            });

            if is_panning && response.dragged() {
                self.pan_offset += response.drag_delta();
            } else if !is_panning {
                // 1. Resolve release/drag stop events GLOBALLY (even if cursor left the canvas response area!)
                if self.tool == Tool::Select && ui.input(|i| i.pointer.any_released()) {
                    if let Some(start_canvas) = self.marquee_start {
                        let latest_pos = ui.input(|i| i.pointer.latest_pos());
                        let end_canvas = if let Some(p) = latest_pos {
                            self.screen_to_canvas(p)
                        } else {
                            if let Some(pos) = response.hover_pos().or(response.interact_pointer_pos()) {
                                self.screen_to_canvas(pos)
                            } else {
                                start_canvas // fallback
                            }
                        };
                        
                        let marquee_box = egui::Rect::from_two_pos(start_canvas, end_canvas);
                        if marquee_box.width() > 2.0 && marquee_box.height() > 2.0 {
                            // Select ALL shapes intersecting the marquee (not just frontmost)
                            self.clear_selection();
                            for (idx, shape) in self.canvas.shapes.iter().enumerate() {
                                if marquee_box.intersects(shape.data.get_bounds()) {
                                    self.selected_shape_indices.insert(idx);
                                    self.primary_selected = Some(idx);
                                }
                            }
                        }
                    }
                    self.is_resizing = None;
                    self.is_dragging_shape = false;
                    self.marquee_start = None;
                    // Whenever we finish interacting, it might have been a drag or resize, so mark dirty
                    self.is_dirty = true;
                }

                let pointer_pos = response.hover_pos().or(response.interact_pointer_pos());
                if let Some(pos) = pointer_pos {
                    let canvas_pos = self.screen_to_canvas(pos);

                    if self.tool == Tool::Select {
                        // 1. Instant Pointer Down selection / resize start / marquee start
                        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
                        let press_pos = ui.input(|i| i.pointer.press_origin());
                        
                        if primary_pressed && press_pos.is_some() && response.rect.contains(press_pos.unwrap()) {
                            let click_pos = press_pos.unwrap();
                            let click_canvas_pos = self.screen_to_canvas(click_pos);
                            let mut clicked_handle = false;
                            
                            if let Some(selected_idx) = self.primary_selected {
                                if selected_idx < self.canvas.shapes.len() {
                                    if let Some(handle_idx) = self.get_handle_under_mouse(selected_idx, click_pos) {
                                        self.is_resizing = Some(handle_idx);
                                        self.drag_start_pos = click_pos;
                                        clicked_handle = true;
                                    }
                                }
                            }

                            if !clicked_handle {
                                if let Some(idx) = self.hit_test(click_canvas_pos) {
                                    self.select_single(idx);
                                    self.is_dragging_shape = true;
                                    self.drag_start_pos = click_pos;
                                    self.marquee_start = None;
                                } else {
                                    // Clicking empty space: clear selection and start marquee
                                    self.clear_selection();
                                    self.marquee_start = Some(click_canvas_pos);
                                }
                            }
                        }

                        // 2. Click to Deselect / Select fallback
                        if response.clicked() {
                            if let Some(idx) = self.hit_test(canvas_pos) {
                                self.select_single(idx);
                                self.marquee_start = None;
                            } else {
                                self.clear_selection();
                            }
                        }

                        // 3. Double Click Text shape to Edit
                        if ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)) && response.hovered() {
                            if let Some(idx) = self.hit_test(canvas_pos) {
                                match &self.canvas.shapes[idx].data {
                                    ShapeData::Text { text, .. } => {
                                        self.editing_text_index = Some(idx);
                                        self.editing_text_buffer = text.clone();
                                        self.select_single(idx);
                                        self.marquee_start = None;
                                    }
                                    ShapeData::StickyNote { text, .. } => {
                                        self.editing_text_index = Some(idx);
                                        self.editing_text_buffer = text.clone();
                                        self.select_single(idx);
                                        self.marquee_start = None;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        // 3. Dragging — moves ALL selected shapes together
                        if response.dragged() {
                            let delta = response.drag_delta() / self.zoom;
                            if let Some(handle_idx) = self.is_resizing {
                                if let Some(primary_idx) = self.primary_selected {
                                    if primary_idx < self.canvas.shapes.len() {
                                        self.canvas.shapes[primary_idx].data.resize(handle_idx, delta, canvas_pos);
                                    }
                                }
                            } else if self.is_dragging_shape {
                                for &idx in &self.selected_shape_indices {
                                    if idx < self.canvas.shapes.len() {
                                        self.canvas.shapes[idx].data.translate(delta);
                                    }
                                }
                            }
                        }



                        // 5. Draw Marquee Rectangle
                        if let Some(start_canvas) = self.marquee_start {
                            let start_screen = self.canvas_to_screen(start_canvas);
                            let rect_screen = egui::Rect::from_two_pos(start_screen, pos);
                            painter.rect_filled(
                                rect_screen,
                                2.0,
                                egui::Color32::from_rgb(99, 102, 241).gamma_multiply(0.12),
                            );
                            painter.rect_stroke(
                                rect_screen,
                                2.0,
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(99, 102, 241)),
                                egui::StrokeKind::Outside,
                            );
                        }

                        // Adjust cursor if hovering over handles (primary selection only)
                        if let Some(selected_idx) = self.primary_selected {
                            if selected_idx < self.canvas.shapes.len() {
                                if let Some(handle_idx) = self.get_handle_under_mouse(selected_idx, pos) {
                                    let is_text_or_sticky = matches!(
                                        self.canvas.shapes[selected_idx].data,
                                        ShapeData::Text { .. } | ShapeData::StickyNote { .. }
                                    );
                                    let cursor = if is_text_or_sticky {
                                        egui::CursorIcon::ResizeHorizontal
                                    } else {
                                        match handle_idx {
                                            0 | 3 => egui::CursorIcon::ResizeNwSe,
                                            1 | 2 => egui::CursorIcon::ResizeNeSw,
                                            _ => egui::CursorIcon::Default,
                                        }
                                    };
                                    ctx.set_cursor_icon(cursor);
                                }
                            }
                        }
                    } else {
                        // Drawing shapes tool: Text starts on clicked(), others on drag_started()
                        // Drawing shapes tool: Text and StickyNote start on clicked(), others on drag_started()
                        if (self.tool == Tool::Text || self.tool == Tool::StickyNote) && response.clicked() {
                            if let Some(idx) = self.hit_test(canvas_pos) {
                                // Clicked existing shape: if it's text or sticky note, edit it
                                match &self.canvas.shapes[idx].data {
                                    ShapeData::Text { text, .. } => {
                                        self.editing_text_index = Some(idx);
                                        self.editing_text_buffer = text.clone();
                                        self.select_single(idx);
                                        self.marquee_start = None;
                                    }
                                    ShapeData::StickyNote { text, .. } => {
                                        self.editing_text_index = Some(idx);
                                        self.editing_text_buffer = text.clone();
                                        self.select_single(idx);
                                        self.marquee_start = None;
                                    }
                                    _ => {}
                                }
                            } else {
                                // Clicked empty space: start new text or sticky note shape
                                let edit_idx = self.canvas.start_shape(
                                    self.tool,
                                    canvas_pos,
                                    self.selected_color,
                                    self.stroke_width,
                                    self.filled_shapes,
                                );
                                if let Some(idx) = edit_idx {
                                    self.editing_text_index = Some(idx);
                                    self.editing_text_buffer = String::new();
                                    self.select_single(idx);
                                    self.tool = Tool::Select;
                                }
                            }
                        } else if self.tool != Tool::Text && self.tool != Tool::StickyNote && response.drag_started() {
                            // Start Pen, Rectangle, Circle on drag start
                            let edit_idx = self.canvas.start_shape(
                                self.tool,
                                canvas_pos,
                                self.selected_color,
                                self.stroke_width,
                                self.filled_shapes,
                            );
                            if let Some(idx) = edit_idx {
                                self.editing_text_index = Some(idx);
                                self.editing_text_buffer = String::new();
                                self.select_single(idx);
                            }
                        }

                        if response.dragged() {
                            self.canvas.update_current_shape(canvas_pos);
                        }

                        if response.drag_stopped() {
                            self.canvas.finish_shape();
                            self.is_dirty = true;
                        }
                    }
                }
            }

            // Draw canvas elements
            painter.set_clip_rect(response.rect);
            self.canvas.render(&painter, self.zoom, self.pan_offset, self.editing_text_index);

            // Draw selection box & resize handles for ALL selected shapes
            if self.tool == Tool::Select {
                for &idx in &self.selected_shape_indices {
                    if idx < self.canvas.shapes.len() {
                        let bounds = self.canvas.shapes[idx].data.get_bounds();
                        if bounds.is_positive() {
                            let screen_bounds = egui::Rect::from_min_max(
                                self.canvas_to_screen(bounds.min),
                                self.canvas_to_screen(bounds.max),
                            );

                            // Bounding rect outline
                            painter.rect_stroke(
                                screen_bounds,
                                0.0,
                                egui::Stroke::new(1.5, egui::Color32::from_rgb(99, 102, 241)),
                                egui::StrokeKind::Outside,
                            );

                            // Resize handles only on the primary selected shape
                            if self.primary_selected == Some(idx) {
                                let is_text_or_sticky = matches!(
                                    self.canvas.shapes[idx].data,
                                    ShapeData::Text { .. } | ShapeData::StickyNote { .. }
                                );
                                let handle_positions = if is_text_or_sticky {
                                    vec![screen_bounds.right_top(), screen_bounds.right_bottom()]
                                } else {
                                    vec![
                                        screen_bounds.left_top(),
                                        screen_bounds.right_top(),
                                        screen_bounds.left_bottom(),
                                        screen_bounds.right_bottom(),
                                    ]
                                };
                                for &h_pos in &handle_positions {
                                    painter.rect(
                                        egui::Rect::from_center_size(h_pos, egui::vec2(8.0, 8.0)),
                                        2.0,
                                        egui::Color32::WHITE,
                                        egui::Stroke::new(1.5, egui::Color32::from_rgb(99, 102, 241)),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Dynamic text dimensions caching & StickyNote bottom auto-resizing
            for shape in &mut self.canvas.shapes {
                match &mut shape.data {
                    ShapeData::Text { text, size, max_width, cached_size, .. } => {
                        let font_id = egui::FontId::proportional(*size);
                        let galley = if let Some(mw) = max_width {
                            ui.fonts(|f| f.layout(text.clone(), font_id, egui::Color32::WHITE, *mw))
                        } else {
                            ui.fonts(|f| f.layout_no_wrap(text.clone(), font_id, egui::Color32::WHITE))
                        };
                        *cached_size = Some(galley.size());
                    }
                    ShapeData::StickyNote { rect, text, text_size, .. } => {
                        let padding = 16.0;
                        let text_width = (rect.width() - padding).max(10.0);
                        let font_id = egui::FontId::proportional(*text_size);
                        let galley = ui.fonts(|f| f.layout(text.clone(), font_id, egui::Color32::WHITE, text_width));
                        let text_height = galley.size().y;
                        
                        let min_height = 140.0;
                        let required_height = (text_height + padding).max(min_height);
                        if (rect.height() - required_height).abs() > 0.1 {
                            rect.max.y = rect.min.y + required_height;
                            self.is_dirty = true;
                        }
                    }
                    _ => {}
                }
            }
        });

        // 5. INLINE TEXT EDITOR
        if let Some(idx) = self.editing_text_index {
            if idx < self.canvas.shapes.len() {
                let (canvas_pos, text_size, text_color) = match &self.canvas.shapes[idx].data {
                    ShapeData::Text { pos, size, color, .. } => (*pos, *size, *color),
                    ShapeData::StickyNote { rect, text_size, text_color, .. } => {
                        (rect.min + egui::vec2(8.0, 8.0), *text_size, *text_color)
                    }
                    _ => (egui::Pos2::ZERO, 24.0, egui::Color32::WHITE),
                };
                let screen_pos = self.canvas_to_screen(canvas_pos);

                egui::Area::new(egui::Id::new("inline_text_edit"))
                    .fixed_pos(screen_pos)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        let font_id = egui::FontId::proportional(text_size * self.zoom);
                        
                        let mut text_edit = egui::TextEdit::multiline(&mut self.editing_text_buffer)
                            .font(font_id)
                            .text_color(text_color)
                            .frame(false)
                            .margin(egui::Margin::same(0));

                        if let ShapeData::StickyNote { rect, .. } = &self.canvas.shapes[idx].data {
                            let text_width = (rect.width() - 16.0) * self.zoom;
                            text_edit = text_edit.desired_width(text_width);
                        } else if let ShapeData::Text { max_width: Some(mw), .. } = &self.canvas.shapes[idx].data {
                            let text_width = mw * self.zoom;
                            text_edit = text_edit.desired_width(text_width);
                        }

                        let response = ui.add(text_edit);
                        response.request_focus();

                        // Live-update the canvas text as the user types
                        match &mut self.canvas.shapes[idx].data {
                            ShapeData::Text { text, .. } => {
                                *text = self.editing_text_buffer.clone();
                            }
                            ShapeData::StickyNote { text, .. } => {
                                *text = self.editing_text_buffer.clone();
                            }
                            _ => {}
                        }

                        // Close editor on escape, lost focus, or cmd+enter
                        let pressed_esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                        let pressed_cmd_enter = ui.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::Enter));
                        
                        if response.lost_focus() || pressed_esc || pressed_cmd_enter {
                            let is_empty = self.editing_text_buffer.trim().is_empty();
                            match &mut self.canvas.shapes[idx].data {
                                ShapeData::Text { text, .. } => {
                                    if is_empty {
                                        self.canvas.shapes.remove(idx);
                                        self.clear_selection();
                                    } else {
                                        *text = self.editing_text_buffer.clone();
                                    }
                                }
                                ShapeData::StickyNote { text, .. } => {
                                    // Don't delete sticky notes for empty text
                                    *text = self.editing_text_buffer.clone();
                                }
                                _ => {}
                            }
                            self.is_dirty = true;
                            self.editing_text_index = None;
                            self.tool = Tool::Select; // Auto-switch to Select tool!
                        }
                    });
            } else {
                self.editing_text_index = None;
            }
        }

        // 6. EXPORT DIALOG OVERLAY WINDOW
        if self.show_export_dialog {
            egui::Window::new("Export Canvas")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Export the active canvas bounds to an image file:");
                        ui.add(egui::Slider::new(&mut self.export_scale, 0.5..=4.0).text("Resolution Scale"));
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.export_jpeg, false, "PNG (Lossless)");
                            ui.radio_value(&mut self.export_jpeg, true, "JPEG");
                        });
                        if self.export_jpeg {
                            ui.add(egui::Slider::new(&mut self.export_quality, 10..=100).text("JPEG Quality"));
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Export to file").clicked() {
                                self.export_file_dialog();
                            }
                            if ui.button("Cancel").clicked() {
                                self.show_export_dialog = false;
                            }
                        });
                    });
                });
        }

        // 7. TOAST NOTIFICATION CARD
        if let Some((msg, _)) = &self.notification {
            egui::Area::new(egui::Id::new("notification"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 20.0])
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(31, 41, 55)) // charcoal
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(75, 85, 99)))
                        .corner_radius(egui::CornerRadius::same(20)) // pill shape
                        .inner_margin(egui::Margin::symmetric(16, 8)) // horizontal padding
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(msg).color(egui::Color32::WHITE).strong()).truncate());
                        });
                });
        }
    }
}

impl App {
    fn compress_and_scale(&self, img: image::DynamicImage) -> Result<(Vec<u8>, [f32; 2]), String> {
        let width = img.width();
        let height = img.height();
        let short_side = width.min(height);

        let scaled_img = if short_side > 2000 {
            let scale = 2000.0 / short_side as f32;
            let new_w = (width as f32 * scale) as u32;
            let new_h = (height as f32 * scale) as u32;
            img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        let mut compressed_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut compressed_bytes);
        scaled_img
            .write_with_encoder(encoder)
            .map_err(|e| e.to_string())?;

        Ok((compressed_bytes, [scaled_img.width() as f32, scaled_img.height() as f32]))
    }

    fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Ok(image) = clipboard.get_image() {
                    if let Some(rgba) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                        image.width as u32,
                        image.height as u32,
                        image.bytes.into_owned(),
                    ) {
                        let dynamic_img = image::DynamicImage::ImageRgba8(rgba);
                        match self.compress_and_scale(dynamic_img) {
                            Ok((compressed_bytes, size)) => {
                                let center_screen = ctx.screen_rect().center();
                                let center_canvas = self.screen_to_canvas(center_screen);
                                let idx = self.canvas.add_image(center_canvas, compressed_bytes, size, ctx);
                                self.select_single(idx);
                                self.tool = Tool::Select; // Auto-switch!
                                self.notification = Some((
                                    "Pasted image from clipboard".to_string(),
                                    std::time::Instant::now(),
                                ));
                            }
                            Err(e) => {
                                self.notification = Some((
                                    format!("Pasting image failed: {}", e),
                                    std::time::Instant::now(),
                                ));
                            }
                        }
                    }
                } else if let Ok(text) = clipboard.get_text() {
                    // Check if text is a file path or file:// URL (commonly copied from Finder/Explorer)
                    let clean_text = text.trim();
                    let path_str = if clean_text.starts_with("file://") {
                        clean_text.strip_prefix("file://").unwrap_or(clean_text)
                    } else {
                        clean_text
                    };

                    let decoded_path = path_str.replace("%20", " ");
                    let path = std::path::Path::new(&decoded_path);

                    if path.exists() && path.is_file() {
                        if let Ok(bytes) = std::fs::read(path) {
                            if let Ok(img) = image::load_from_memory(&bytes) {
                                match self.compress_and_scale(img) {
                                    Ok((compressed_bytes, size)) => {
                                        let center_screen = ctx.screen_rect().center();
                                        let center_canvas = self.screen_to_canvas(center_screen);
                                        let idx = self.canvas.add_image(center_canvas, compressed_bytes, size, ctx);
                                        self.select_single(idx);
                                        self.tool = Tool::Select; // Auto-switch!
                                        self.notification = Some((
                                            "Pasted image file from clipboard".to_string(),
                                            std::time::Instant::now(),
                                        ));
                                        return;
                                    }
                                    Err(_) => {}
                                }
                            }
                        }
                    }

                    // Fallback to normal text label
                    let center_screen = ctx.screen_rect().center();
                    let center_canvas = self.screen_to_canvas(center_screen);
                    let idx = self.canvas.add_text(center_canvas, text, self.selected_color);
                    self.is_dirty = true;
                    self.select_single(idx);
                    self.tool = Tool::Select; // Auto-switch!
                    self.notification = Some((
                        "Pasted text from clipboard".to_string(),
                        std::time::Instant::now(),
                    ));
                } else {
                    self.notification = Some((
                        "Clipboard does not contain image or text data".to_string(),
                        std::time::Instant::now(),
                    ));
                }
            }
            Err(e) => {
                self.notification = Some((
                    format!("Failed to open clipboard: {}", e),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    fn import_image_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Image Files", &["png", "jpg", "jpeg", "webp", "gif"])
            .pick_file()
        {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    match self.compress_and_scale(img) {
                        Ok((compressed_bytes, size)) => {
                            let center_screen = ctx.screen_rect().center();
                            let center_canvas = self.screen_to_canvas(center_screen);
                            let idx = self.canvas.add_image(center_canvas, compressed_bytes, size, ctx);
                            self.is_dirty = true;
                            self.select_single(idx);
                            self.tool = Tool::Select; // Auto-switch!
                            self.notification = Some((
                                "Imported image successfully".to_string(),
                                std::time::Instant::now(),
                            ));
                        }
                        Err(e) => {
                            self.notification = Some((
                                format!("Loading image failed: {}", e),
                                std::time::Instant::now(),
                            ));
                        }
                    }
                }
            }
        }
    }

    /// Save to current_file_path if known, otherwise prompt with file dialog.
    fn save(&mut self) -> bool {
        if let Some(path) = self.current_file_path.clone() {
            self.save_to_path(&path)
        } else {
            self.save_file_dialog()
        }
    }

    fn save_to_path(&mut self, path: &std::path::Path) -> bool {
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
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            if std::fs::write(path, json).is_ok() {
                self.current_file_path = Some(path.to_path_buf());
                self.is_dirty = false;
                self.notification = Some((
                    "Saved board state successfully".to_string(),
                    std::time::Instant::now(),
                ));
                return true;
            }
        }
        self.notification = Some((
            "Saving board state failed".to_string(),
            std::time::Instant::now(),
        ));
        false
    }

    fn save_file_dialog(&mut self) -> bool {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Kugel Mood Board", &["kugel"])
            .save_file()
        {
            return self.save_to_path(&path);
        }
        false
    }

    fn open_file_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Kugel Mood Board", &["kugel"])
            .pick_file()
        {
            if !self.open_kugel_file(&path, ctx) {
                self.notification = Some((
                    "Opening board state failed: invalid file".to_string(),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    fn export_file_dialog(&mut self) {
        let filter_name = if self.export_jpeg { "JPEG Image" } else { "PNG Image" };
        let ext = if self.export_jpeg { "jpg" } else { "png" };

        if let Some(path) = rfd::FileDialog::new()
            .add_filter(filter_name, &[ext])
            .save_file()
        {
            match export::export_canvas_to_image(
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
                        std::time::Instant::now(),
                    ));
                }
                Err(e) => {
                    self.notification = Some((
                        format!("Canvas export failed: {}", e),
                        std::time::Instant::now(),
                    ));
                }
            }
        }
    }
}
