mod canvas;
mod export;
#[cfg(target_os = "macos")]
mod macos_open;
mod shapes;

use canvas::Canvas;
use eframe::egui;
use shapes::{Shape, ShapeData, Tool};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

fn main() -> eframe::Result<()> {
    // Register for the .kugel open-documents Apple Event before the event loop
    // starts, so a double-click that cold-launches the app is not dropped.
    #[cfg(target_os = "macos")]
    macos_open::register();

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
            // Apply custom visual styles to egui
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

/// Detect whether a string looks like Markdown (has syntax we'd want to strip).
fn looks_like_markdown(text: &str) -> bool {
    text.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with('#')            // headings
            || t.starts_with("- ")    // unordered list
            || t.starts_with("* ")
            || t.starts_with("+ ")
            || t.starts_with("> ")    // blockquote
            || t.starts_with("```")   // fenced code
            || t.starts_with("|")     // table
    }) || text.contains("**")          // bold
        || text.contains("__")         // bold/underline
        || text.contains("`")          // inline code
        || (text.contains("](") && text.contains('[')) // links/images
}

/// Convert Markdown into plain text by removing the syntax that makes it Markdown.
/// Best-effort, line based; keeps the readable content, drops the markup.
fn strip_markdown(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;

    for raw in text.lines() {
        let trimmed = raw.trim_start();

        // Fenced code blocks: drop the ``` fences, keep the code lines verbatim.
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            out.push(raw.to_string());
            continue;
        }

        let indent = &raw[..raw.len() - trimmed.len()];
        let mut line = trimmed.to_string();

        // Headings: strip leading #'s and any trailing closing #'s.
        if line.starts_with('#') {
            line = line.trim_start_matches('#').trim_start().to_string();
            line = line.trim_end_matches('#').trim_end().to_string();
        }

        // Blockquotes: strip leading > markers.
        while line.starts_with('>') {
            line = line[1..].trim_start().to_string();
        }

        // List markers: "- ", "* ", "+ ", or "1. ".
        if let Some(rest) = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("+ "))
        {
            line = format!("• {}", rest);
        } else if let Some(pos) = line.find(". ") {
            if line[..pos].chars().all(|c| c.is_ascii_digit()) && pos > 0 {
                line = line[pos + 2..].to_string();
            }
        }

        // Horizontal rules -> blank line.
        if line == "---" || line == "***" || line == "___" {
            line.clear();
        }

        line = strip_inline_markdown(&line);
        out.push(format!("{}{}", indent, line));
    }

    out.join("\n")
}

/// Remove inline Markdown markup: emphasis, code spans, and links/images.
fn strip_inline_markdown(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            // Image / link: ![alt](url) or [text](url) -> keep alt/text.
            '!' if chars.get(i + 1) == Some(&'[') => {
                i += 1; // skip '!', fall through handles '['
                continue;
            }
            '[' => {
                if let Some(close) = chars[i..].iter().position(|&x| x == ']') {
                    let end = i + close;
                    // Must be followed by "(...)" to count as a link.
                    if chars.get(end + 1) == Some(&'(') {
                        if let Some(paren) = chars[end + 1..].iter().position(|&x| x == ')') {
                            out.extend(&chars[i + 1..end]);
                            i = end + 1 + paren + 1;
                            continue;
                        }
                    }
                }
                out.push(c);
                i += 1;
            }
            // Emphasis / bold markers: skip runs of * or _.
            '*' | '_' => {
                while i < chars.len() && (chars[i] == '*' || chars[i] == '_') {
                    i += 1;
                }
            }
            // Inline code: skip backticks, keep contents.
            '`' => {
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    out
}

#[derive(Clone)]
enum UiEvent {
    UpdateAvailable {
        version: String,
        html_url: String,
        download_url: String,
    },
    UpToDate,
    UpdateCheckFailed(String),
    UpdateApplied,
    UpdateInstallFailed(String),
    LinkTitleFetched {
        shape_id: usize,
        url: String,
        title: String,
    },
}

#[derive(Default, Clone)]
enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpdateAvailable {
        version: String,
        html_url: String,
        download_url: String,
    },
    UpToDate,
    Updating,
    UpdateDone,
    Failed(String),
}

#[derive(Clone)]
struct IconPair {
    light: egui::TextureHandle,
    dark: egui::TextureHandle,
}

#[derive(Clone)]
struct Icons {
    select: IconPair,
    pen: IconPair,
    line: IconPair,
    rectangle: IconPair,
    circle: IconPair,
    text: IconPair,
    note: IconPair,
    section: IconPair,
    import: IconPair,
    undo: IconPair,
    redo: IconPair,
    clear: IconPair,
    save: IconPair,
    open: IconPair,
    export: IconPair,
    theme_dark: IconPair,
    theme_light: IconPair,
    settings: IconPair,
    paintbrush: IconPair,
    wallpaper: IconPair,
}

impl Icons {
    fn new(ctx: &egui::Context) -> Self {
        let load = |name: &str, bytes: &[u8]| -> IconPair {
            let img = image::load_from_memory(bytes).expect("Failed to load icon from memory");
            let rgba = img.to_rgba8();
            let color_img_light = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                &rgba.into_raw(),
            );

            let mut color_img_dark = color_img_light.clone();
            for pixel in &mut color_img_dark.pixels {
                *pixel = egui::Color32::from_rgba_unmultiplied(
                    255 - pixel.r(),
                    255 - pixel.g(),
                    255 - pixel.b(),
                    pixel.a(),
                );
            }

            let light = ctx.load_texture(
                format!("icon_light_{}", name),
                color_img_light,
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    mipmap_mode: Some(egui::TextureFilter::Linear),
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                },
            );

            let dark = ctx.load_texture(
                format!("icon_dark_{}", name),
                color_img_dark,
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    mipmap_mode: Some(egui::TextureFilter::Linear),
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                },
            );

            IconPair { light, dark }
        };

        Self {
            select: load(
                "select",
                include_bytes!("../assets/icons/mouse-pointer-2.png"),
            ),
            pen: load("pen", include_bytes!("../assets/icons/pen.png")),
            line: load("line", include_bytes!("../assets/icons/line.png")),
            rectangle: load(
                "rectangle",
                include_bytes!("../assets/icons/rectangle-horizontal.png"),
            ),
            circle: load("circle", include_bytes!("../assets/icons/circle.png")),
            text: load("text", include_bytes!("../assets/icons/text-initial.png")),
            note: load("note", include_bytes!("../assets/icons/sticky-note.png")),
            section: load(
                "section",
                include_bytes!("../assets/icons/square-dashed.png"),
            ),
            import: load("import", include_bytes!("../assets/icons/import.png")),
            undo: load("undo", include_bytes!("../assets/icons/undo.png")),
            redo: load("redo", include_bytes!("../assets/icons/redo.png")),
            clear: load("clear", include_bytes!("../assets/icons/trash.png")),
            save: load("save", include_bytes!("../assets/icons/save.png")),
            open: load("open", include_bytes!("../assets/icons/folder-open.png")),
            export: load("export", include_bytes!("../assets/icons/file-down.png")),
            theme_dark: load("theme_dark", include_bytes!("../assets/icons/moon.png")),
            theme_light: load("theme_light", include_bytes!("../assets/icons/sun.png")),
            settings: load("settings", include_bytes!("../assets/icons/settings.png")),
            paintbrush: load(
                "paintbrush",
                include_bytes!("../assets/icons/paintbrush.png"),
            ),
            wallpaper: load("wallpaper", include_bytes!("../assets/icons/wallpaper.png")),
        }
    }

    fn icon_button(&self, ui: &mut egui::Ui, pair: &IconPair, tooltip: &str) -> egui::Response {
        let is_dark = ui.visuals().dark_mode;
        let texture = if is_dark { &pair.dark } else { &pair.light };
        let image = egui::Image::new(texture).fit_to_exact_size(egui::vec2(22.0, 22.0));
        ui.add(egui::ImageButton::new(image).frame(false))
            .on_hover_text(tooltip)
    }

    fn selectable_icon_button(
        &self,
        ui: &mut egui::Ui,
        selected: bool,
        pair: &IconPair,
        tooltip: &str,
    ) -> egui::Response {
        let is_dark = ui.visuals().dark_mode;
        let texture = if is_dark { &pair.dark } else { &pair.light };
        let image = egui::Image::new(texture).fit_to_exact_size(egui::vec2(22.0, 22.0));
        ui.add(
            egui::ImageButton::new(image)
                .selected(selected)
                .frame(false),
        )
        .on_hover_text(tooltip)
    }
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
    is_resizing: Option<usize>,      // Selected handle index: 0=TL, 1=TR, 2=BL, 3=BR
    is_dragging_shape: bool,
    drag_start_pos: egui::Pos2,
    snap_correction: egui::Vec2,
    marquee_start: Option<egui::Pos2>,

    // Copy / Paste buffer
    copied_shape: Option<Shape>,

    // True while a single color-picker drag is recoloring the selection,
    // so the whole drag collapses into one undo step.
    recoloring_selection: bool,

    // Text editing state
    editing_text_index: Option<usize>,
    editing_text_buffer: String,
    request_text_focus: bool,

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

    // UI state
    top_panel_collapsed: bool,

    // Icons
    icons: Option<Icons>,

    // Update state
    update_state: UpdateState,
    ui_event_tx: std::sync::mpsc::Sender<UiEvent>,
    ui_event_rx: std::sync::mpsc::Receiver<UiEvent>,
}

impl Default for App {
    fn default() -> Self {
        let (ui_event_tx, ui_event_rx) = std::sync::mpsc::channel();
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

/// Install OpenSans as the global default font for both proportional and
/// monospace families, matching the bundled font used on export.
fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "OpenSans".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/OpenSans-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "OpenSans".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "OpenSans".to_owned());
    ctx.set_fonts(fonts);
}

impl App {
    /// Clear selection and select a single shape.
    fn select_single(&mut self, idx: usize) {
        self.selected_shape_indices.clear();
        self.selected_shape_indices.insert(idx);
        self.primary_selected = Some(idx);
    }

    /// Select all shapes.
    fn select_all(&mut self) {
        self.selected_shape_indices.clear();
        for idx in 0..self.canvas.shapes.len() {
            self.selected_shape_indices.insert(idx);
        }
        if !self.canvas.shapes.is_empty() {
            self.primary_selected = Some(self.canvas.shapes.len() - 1);
        } else {
            self.primary_selected = None;
        }
    }

    /// Duplicate all selected shapes in place and select the copies.
    fn duplicate_selection(&mut self, ctx: &egui::Context) {
        if self.selected_shape_indices.is_empty() {
            return;
        }
        self.canvas.history.push(self.canvas.shapes.clone());
        self.canvas.undo_history.clear();
        self.is_dirty = true;

        let mut indices: Vec<usize> = self.selected_shape_indices.iter().copied().collect();
        indices.sort_unstable();
        self.clear_selection();

        for idx in indices {
            if idx < self.canvas.shapes.len() {
                let mut dup = self.canvas.shapes[idx].clone();
                dup.id = self.canvas.next_id;
                self.canvas.next_id += 1;
                dup.data.load_textures(ctx, dup.id);
                self.canvas.shapes.push(dup);
                let new_idx = self.canvas.shapes.len() - 1;
                self.selected_shape_indices.insert(new_idx);
                self.primary_selected = Some(new_idx);
            }
        }
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

        // Align canvas background default if system is light mode
        if !dark_mode {
            app.background_color = egui::Color32::from_gray(240);
        }

        // Check if a file path was passed as a command-line argument (for double-clicking files)
        let mut opened = false;
        if let Some(path_str) = std::env::args().nth(1) {
            let path = std::path::Path::new(&path_str);
            if path.exists() && path.is_file() {
                opened = app.open_kugel_file(path, &cc.egui_ctx);
            }
        }

        // Otherwise reopen the most recently used board, if it still exists.
        // (On macOS an explicit double-click arrives later via the openFiles
        // Apple Event and will replace whatever we restore here.)
        if !opened {
            if let Some(path) = cc
                .storage
                .and_then(|s| eframe::get_value::<String>(s, "last_file_path"))
            {
                let path = std::path::PathBuf::from(path);
                if path.is_file() {
                    app.open_kugel_file(&path, &cc.egui_ctx);
                }
            }
        }

        // Start checking for updates in the background
        app.update_state = UpdateState::Checking;
        spawn_update_check(app.ui_event_tx.clone(), cc.egui_ctx.clone());

        app
    }

    fn check_for_updates(&mut self, ctx: &egui::Context) {
        self.update_state = UpdateState::Checking;
        spawn_update_check(self.ui_event_tx.clone(), ctx.clone());
    }

    fn perform_self_update(&mut self, download_url: String, ctx: &egui::Context) {
        self.update_state = UpdateState::Updating;
        let ui_tx = self.ui_event_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = do_self_update(&download_url);
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

    fn apply_ui_events(&mut self) {
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
                        std::time::Instant::now(),
                    ));
                }
                UiEvent::UpdateInstallFailed(err) => {
                    self.update_state = UpdateState::Failed(err.clone());
                    self.notification =
                        Some((format!("Update failed: {err}"), std::time::Instant::now()));
                }
                UiEvent::LinkTitleFetched { shape_id, url, title } => {
                    if let Some(shape) = self.canvas.shapes.iter_mut().find(|s| s.id == shape_id) {
                        if shape.data.link_url() == Some(&url) {
                            shape.data.set_link_title(Some(title));
                            self.is_dirty = true;
                        }
                    }
                }
            }
        }
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
                self.generate_missing_link_previews(ctx);
                self.current_file_path = Some(path.to_path_buf());
                self.is_dirty = false;
                self.notification = Some((
                    format!(
                        "Opened board: {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    std::time::Instant::now(),
                ));
                return true;
            }
        }
        false
    }

    /// Alignment snapping: compare the moving bounds' edges/centers against every
    /// non-selected shape and return the nearest correction (canvas units) plus the
    /// guide segments to draw. `threshold` is in canvas units.
    fn compute_alignment_snap(
        &self,
        moving: egui::Rect,
        threshold: f32,
    ) -> (egui::Vec2, Vec<(egui::Pos2, egui::Pos2)>) {
        let m_xs = [moving.min.x, moving.center().x, moving.max.x];
        let m_ys = [moving.min.y, moving.center().y, moving.max.y];

        let mut best_dx = threshold;
        let mut best_dy = threshold;
        let mut corr_x = 0.0;
        let mut corr_y = 0.0;
        let mut snap_x: Option<(f32, egui::Rect)> = None;
        let mut snap_y: Option<(f32, egui::Rect)> = None;

        for (i, shape) in self.canvas.shapes.iter().enumerate() {
            if self.selected_shape_indices.contains(&i) {
                continue;
            }
            let t = shape.data.get_bounds();
            if !t.is_positive() {
                continue;
            }
            for &tx in &[t.min.x, t.center().x, t.max.x] {
                for &mx in &m_xs {
                    let d = (tx - mx).abs();
                    if d < best_dx {
                        best_dx = d;
                        corr_x = tx - mx;
                        snap_x = Some((tx, t));
                    }
                }
            }
            for &ty in &[t.min.y, t.center().y, t.max.y] {
                for &my in &m_ys {
                    let d = (ty - my).abs();
                    if d < best_dy {
                        best_dy = d;
                        corr_y = ty - my;
                        snap_y = Some((ty, t));
                    }
                }
            }
        }

        let correction = egui::vec2(corr_x, corr_y);
        let corrected = moving.translate(correction);
        let mut guides = Vec::new();
        if let Some((tx, t)) = snap_x {
            let y0 = corrected.min.y.min(t.min.y);
            let y1 = corrected.max.y.max(t.max.y);
            guides.push((egui::pos2(tx, y0), egui::pos2(tx, y1)));
        }
        if let Some((ty, t)) = snap_y {
            let x0 = corrected.min.x.min(t.min.x);
            let x1 = corrected.max.x.max(t.max.x);
            guides.push((egui::pos2(x0, ty), egui::pos2(x1, ty)));
        }
        (correction, guides)
    }

    fn screen_to_canvas(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (screen_pos.x - self.pan_offset.x) / self.zoom,
            (screen_pos.y - self.pan_offset.y) / self.zoom,
        )
    }

    /// Canvas-space point where pasted content should land: current mouse
    /// position, falling back to the viewport center when no pointer is known.
    fn paste_target_canvas(&self, ctx: &egui::Context) -> egui::Pos2 {
        let screen = ctx
            .input(|i| i.pointer.latest_pos())
            .unwrap_or_else(|| ctx.screen_rect().center());
        self.screen_to_canvas(screen)
    }

    fn canvas_to_screen(&self, canvas_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            canvas_pos.x * self.zoom + self.pan_offset.x,
            canvas_pos.y * self.zoom + self.pan_offset.y,
        )
    }

    /// If a text shape's content is nothing but a single URL, return it
    /// (normalized to an https:// prefix for bare `www.` links).
    fn text_shape_url(&self, idx: usize) -> Option<String> {
        let ShapeData::Text { text, .. } = &self.canvas.shapes.get(idx)?.data else {
            return None;
        };
        let t = text.trim();
        if t.is_empty() || t.split_whitespace().count() != 1 {
            return None;
        }
        if t.starts_with("http://") || t.starts_with("https://") {
            Some(t.to_string())
        } else if t.starts_with("www.") {
            Some(format!("https://{t}"))
        } else {
            None
        }
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
            let handles = [
                (1, screen_bounds.right_top()),
                (3, screen_bounds.right_bottom()),
            ];
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

    /// Union bounds (canvas) of all selected shapes.
    fn selection_bounds(&self) -> Option<egui::Rect> {
        let mut acc: Option<egui::Rect> = None;
        for &idx in &self.selected_shape_indices {
            if idx < self.canvas.shapes.len() {
                let b = self.canvas.shapes[idx].data.get_bounds();
                if b.is_positive() {
                    acc = Some(acc.map_or(b, |a| a.union(b)));
                }
            }
        }
        acc
    }

    /// Corner handle of the group selection box under the mouse (screen pos).
    fn group_handle_under_mouse(&self, mouse_pos: egui::Pos2) -> Option<usize> {
        let bounds = self.selection_bounds()?;
        let screen_bounds = egui::Rect::from_min_max(
            self.canvas_to_screen(bounds.min),
            self.canvas_to_screen(bounds.max),
        );
        let handle_positions = [
            screen_bounds.left_top(),
            screen_bounds.right_top(),
            screen_bounds.left_bottom(),
            screen_bounds.right_bottom(),
        ];
        for (h_idx, &pos) in handle_positions.iter().enumerate() {
            if mouse_pos.distance(pos) <= 8.0 {
                return Some(h_idx);
            }
        }
        None
    }
}

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

        // macOS delivers double-clicked / "Open With" files via an Apple Event
        // rather than argv; open whatever was queued since the last frame.
        #[cfg(target_os = "macos")]
        for path in macos_open::take_pending() {
            if path.exists() && path.is_file() {
                self.open_kugel_file(&path, ctx);
            }
        }

        // Handle window close request with unsaved changes prompt
        if ctx.input(|i| i.viewport().close_requested()) {
            // A brand-new, never-saved board with nothing on it has nothing worth
            // saving — don't nag on exit.
            let empty_unsaved = self.canvas.shapes.is_empty() && self.current_file_path.is_none();
            if self.close_confirmed || !self.is_dirty || empty_unsaved {
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

                // Move the pasted shape so its center lands under the cursor.
                let target = self.paste_target_canvas(ctx);
                let center = shape.data.get_bounds().center();
                shape.data.translate(target - center);
                shape.id = self.canvas.next_id;
                self.canvas.next_id += 1;
                shape.data.load_textures(ctx, shape.id);

                self.canvas.shapes.push(shape);
                self.select_single(self.canvas.shapes.len() - 1);
                self.tool = Tool::Select;
                self.notification = Some(("Pasted shape".to_string(), std::time::Instant::now()));
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
                                if icons
                                    .icon_button(
                                        ui,
                                        &icons.settings,
                                        if self.top_panel_collapsed {
                                            "Show Settings"
                                        } else {
                                            "Hide Settings"
                                        },
                                    )
                                    .clicked()
                                {
                                    self.top_panel_collapsed = !self.top_panel_collapsed;
                                }
                            });
                            if self.top_panel_collapsed {
                                return;
                            }
                            ui.horizontal(|ui| {
                                let wallpaper_tex = if is_dark {
                                    &icons.wallpaper.dark
                                } else {
                                    &icons.wallpaper.light
                                };
                                let wallpaper_image = egui::Image::new(wallpaper_tex)
                                    .fit_to_exact_size(egui::vec2(18.0, 18.0));
                                ui.add(wallpaper_image).on_hover_text("Background Color");
                                egui::color_picker::color_edit_button_srgba(
                                    ui,
                                    &mut self.background_color,
                                    egui::color_picker::Alpha::Opaque,
                                );
                            });
                            ui.checkbox(&mut self.use_grid, "Show Grid");
                            ui.horizontal(|ui| {
                                let theme_icon = if self.dark_mode {
                                    &icons.theme_light
                                } else {
                                    &icons.theme_dark
                                };
                                if icons
                                    .icon_button(
                                        ui,
                                        theme_icon,
                                        if self.dark_mode {
                                            "Switch to Light Theme"
                                        } else {
                                            "Switch to Dark Theme"
                                        },
                                    )
                                    .clicked()
                                {
                                    self.dark_mode = !self.dark_mode;
                                    // Smoothly toggle background color if default
                                    if self.dark_mode {
                                        if self.background_color == egui::Color32::from_gray(240) {
                                            self.background_color =
                                                egui::Color32::from_rgb(20, 20, 23);
                                        }
                                    } else {
                                        if self.background_color
                                            == egui::Color32::from_rgb(20, 20, 23)
                                        {
                                            self.background_color = egui::Color32::from_gray(240);
                                        }
                                    }
                                    self.style_applied = false;
                                }
                                if ui.button("Reset View").clicked() {
                                    self.zoom = 1.0;
                                    self.pan_offset = egui::Vec2::ZERO;
                                }
                            });

                            // Collect update state data before drawing to avoid borrow conflicts.
                            enum UpdateAction {
                                None,
                                CheckUpdates,
                                PerformUpdate(String),
                            }
                            let busy = matches!(
                                self.update_state,
                                UpdateState::Checking | UpdateState::Updating
                            );
                            let update_info: Option<(String, String, String)> =
                                if let UpdateState::UpdateAvailable {
                                    version,
                                    html_url,
                                    download_url,
                                } = &self.update_state
                                {
                                    Some((version.clone(), html_url.clone(), download_url.clone()))
                                } else {
                                    None
                                };
                            let update_err: Option<String> =
                                if let UpdateState::Failed(e) = &self.update_state {
                                    Some(e.clone())
                                } else {
                                    None
                                };

                            let mut action = UpdateAction::None;
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(!busy, egui::Button::new("Check for updates"))
                                    .clicked()
                                {
                                    action = UpdateAction::CheckUpdates;
                                }
                                match &self.update_state {
                                    UpdateState::Idle => {}
                                    UpdateState::Checking => {
                                        ui.spinner();
                                        ui.label(
                                            egui::RichText::new("Checking...")
                                                .color(ui.visuals().weak_text_color()),
                                        );
                                    }
                                    UpdateState::UpToDate => {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "v{} is up to date",
                                                env!("CARGO_PKG_VERSION")
                                            ))
                                            .color(ui.visuals().weak_text_color()),
                                        );
                                    }
                                    UpdateState::UpdateAvailable { .. } => {
                                        if let Some((version, html_url, download_url)) =
                                            &update_info
                                        {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "v{version} available!"
                                                ))
                                                .color(egui::Color32::from_rgb(240, 180, 60)),
                                            );
                                            ui.hyperlink_to("Release notes", html_url);
                                            if !download_url.is_empty()
                                                && ui
                                                    .button(
                                                        egui::RichText::new("Update now").color(
                                                            egui::Color32::from_rgb(120, 217, 120),
                                                        ),
                                                    )
                                                    .clicked()
                                            {
                                                action = UpdateAction::PerformUpdate(
                                                    download_url.clone(),
                                                );
                                            }
                                        }
                                    }
                                    UpdateState::Updating => {
                                        ui.spinner();
                                        ui.label(
                                            egui::RichText::new("Installing update...")
                                                .color(ui.visuals().weak_text_color()),
                                        );
                                    }
                                    UpdateState::UpdateDone => {
                                        ui.label(
                                            egui::RichText::new("Restart to apply update")
                                                .color(egui::Color32::from_rgb(120, 217, 120)),
                                        );
                                    }
                                    UpdateState::Failed(_) => {
                                        if let Some(err) = &update_err {
                                            ui.label(
                                                egui::RichText::new(format!("Failed: {err}"))
                                                    .color(ui.visuals().error_fg_color)
                                                    .small(),
                                            );
                                        }
                                    }
                                }
                            });
                            match action {
                                UpdateAction::CheckUpdates => self.check_for_updates(ctx),
                                UpdateAction::PerformUpdate(url) => {
                                    self.perform_self_update(url, ctx)
                                }
                                UpdateAction::None => {}
                            }
                        });
                    });
            });

        // 3. FLOATING BOTTOM TOOLBAR
        let compact_toolbar = ctx.screen_rect().width() < 1080.0;
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
                                (Tool::Select, &icons.select, "Select (V)"),
                                (Tool::Pen, &icons.pen, "Pen (P)"),
                                (Tool::Line, &icons.line, "Line (L)"),
                                (Tool::Rectangle, &icons.rectangle, "Rectangle (R)"),
                                (Tool::Circle, &icons.circle, "Circle (O)"),
                                (Tool::Text, &icons.text, "Text (T)"),
                                (Tool::StickyNote, &icons.note, "Sticky Note (N)"),
                                (Tool::Section, &icons.section, "Section (F)"),
                            ];
                            if compact_toolbar {
                                ui.spacing_mut().button_padding = egui::vec2(6.0, 6.0);
                                ui.spacing_mut().item_spacing.x = 4.0;
                            }
                            for &(t, icon_tex, tooltip) in &tools {
                                let selected = self.tool == t;
                                if icons
                                    .selectable_icon_button(ui, selected, icon_tex, tooltip)
                                    .clicked()
                                {
                                    self.tool = t;
                                    self.clear_selection();
                                    self.editing_text_index = None;
                                }
                            }

                            if compact_toolbar {
                                return;
                            }

                            ui.separator();

                            if icons
                                .icon_button(ui, &icons.import, "Import Image (I)")
                                .clicked()
                            {
                                self.import_image_dialog(ctx);
                            }

                            ui.separator();

                            // Colors & Properties
                            let paintbrush_tex = if is_dark {
                                &icons.paintbrush.dark
                            } else {
                                &icons.paintbrush.light
                            };
                            let size_image = egui::Image::new(paintbrush_tex)
                                .fit_to_exact_size(egui::vec2(18.0, 18.0));
                            ui.add(size_image).on_hover_text("Stroke Size");
                            ui.add(
                                egui::Slider::new(&mut self.stroke_width, 1.0..=20.0)
                                    .show_value(false),
                            );

                            let color_resp = egui::color_picker::color_edit_button_srgba(
                                ui,
                                &mut self.selected_color,
                                egui::color_picker::Alpha::Opaque,
                            )
                            .on_hover_text("Stroke Color");

                            // Recolor any selected shapes live as the picker changes.
                            if color_resp.changed() && !self.selected_shape_indices.is_empty() {
                                if !self.recoloring_selection {
                                    self.canvas.history.push(self.canvas.shapes.clone());
                                    self.canvas.undo_history.clear();
                                    self.recoloring_selection = true;
                                }
                                for &idx in &self.selected_shape_indices {
                                    if let Some(shape) = self.canvas.shapes.get_mut(idx) {
                                        shape.data.set_color(self.selected_color);
                                    }
                                }
                                self.is_dirty = true;
                            }
                            if self.recoloring_selection && ui.input(|i| i.pointer.any_released()) {
                                self.recoloring_selection = false;
                            }

                            ui.checkbox(&mut self.filled_shapes, "Fill");

                            ui.separator();

                            // Undo / Redo
                            if icons.icon_button(ui, &icons.undo, "Undo (Cmd+Z)").clicked() {
                                self.canvas.undo();
                                self.clear_selection();
                                self.editing_text_index = None;
                                self.is_dirty = true;
                            }
                            if icons.icon_button(ui, &icons.redo, "Redo (Cmd+Y)").clicked() {
                                self.canvas.redo();
                                self.clear_selection();
                                self.editing_text_index = None;
                                self.is_dirty = true;
                            }
                            if icons
                                .icon_button(ui, &icons.clear, "Clear Canvas")
                                .clicked()
                            {
                                self.canvas.clear();
                                self.clear_selection();
                                self.editing_text_index = None;
                                self.is_dirty = true;
                            }

                            ui.separator();

                            // File & Export
                            if icons
                                .icon_button(ui, &icons.save, "Save Board (Cmd+S)")
                                .clicked()
                            {
                                self.save();
                            }
                            if icons
                                .icon_button(ui, &icons.open, "Open Board (Cmd+O)")
                                .clicked()
                            {
                                self.open_file_dialog(ctx);
                            }
                            if icons
                                .icon_button(ui, &icons.export, "Export Board (Cmd+E)")
                                .clicked()
                            {
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

                let mut alignment_guides: Vec<(egui::Pos2, egui::Pos2)> = Vec::new();

                // Render background
                painter.rect_filled(response.rect, 0.0, self.background_color);

                // Draw grid dots
                if self.use_grid {
                    let mut grid_spacing = 50.0 * self.zoom;
                    while grid_spacing < 24.0 {
                        grid_spacing *= 2.0;
                    }
                    let grid_color = if is_dark {
                        egui::Color32::from_gray(95).gamma_multiply(0.45)
                    } else {
                        egui::Color32::from_gray(130).gamma_multiply(0.6)
                    };

                    if grid_spacing > 8.0 {
                        let min_x = ((response.rect.min.x - self.pan_offset.x) / grid_spacing)
                            .floor()
                            * grid_spacing
                            + self.pan_offset.x;
                        let min_y = ((response.rect.min.y - self.pan_offset.y) / grid_spacing)
                            .floor()
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

                // Trackpad two-finger scroll pans; mouse wheel (or cmd/ctrl+scroll) zooms.
                // egui already routes cmd/ctrl+scroll and pinch into zoom_delta().
                let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
                let zoom_delta = ui.input(|i| i.zoom_delta());
                let (mut has_wheel, mut has_trackpad) = (false, false);
                ui.input(|i| {
                    for e in &i.events {
                        if let egui::Event::MouseWheel {
                            unit, modifiers, ..
                        } = e
                        {
                            if modifiers.command || modifiers.ctrl {
                                continue;
                            }
                            match unit {
                                egui::MouseWheelUnit::Point => has_trackpad = true,
                                _ => has_wheel = true,
                            }
                        }
                    }
                });

                if has_trackpad && !has_wheel {
                    self.pan_offset += scroll_delta;
                }

                let wheel_zoom = if has_wheel { scroll_delta.y } else { 0.0 };
                if zoom_delta != 1.0 || wheel_zoom != 0.0 {
                    let pointer_pos = response.hover_pos().unwrap_or(response.rect.center());
                    let zoom_factor = if zoom_delta != 1.0 {
                        zoom_delta
                    } else {
                        1.0 + wheel_zoom * 0.003
                    };

                    if zoom_factor != 1.0 {
                        let old_zoom = self.zoom;
                        self.zoom = (self.zoom * zoom_factor).clamp(0.5, 10.0);

                        let zoom_change = self.zoom / old_zoom;
                        self.pan_offset = pointer_pos.to_vec2()
                            + (self.pan_offset - pointer_pos.to_vec2()) * zoom_change;
                    }
                }

                // Keyboard Shortcuts
                let has_shortcut = |ui: &egui::Ui, key: egui::Key, cmd: bool| {
                    ui.input(|i| {
                        i.events.iter().any(|e| match e {
                            egui::Event::Key {
                                key: k,
                                pressed: true,
                                modifiers,
                                ..
                            } if *k == key => !cmd || modifiers.command || modifiers.ctrl,
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

                    if bare_key(ui, egui::Key::V) || bare_key(ui, egui::Key::W) {
                        self.tool = Tool::Select;
                        self.clear_selection();
                    }
                    if bare_key(ui, egui::Key::P) {
                        self.tool = Tool::Pen;
                        self.clear_selection();
                    }
                    if bare_key(ui, egui::Key::L) {
                        self.tool = Tool::Line;
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
                    if bare_key(ui, egui::Key::F) {
                        self.tool = Tool::Section;
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
                if has_shortcut(ui, egui::Key::N, true) {
                    self.new_board();
                }
                if has_shortcut(ui, egui::Key::E, true) {
                    self.show_export_dialog = true;
                }

                // Select all (Cmd/Ctrl + A)
                if self.editing_text_index.is_none() && has_shortcut(ui, egui::Key::A, true) {
                    self.tool = Tool::Select;
                    self.select_all();
                    self.notification = Some((
                        format!("Selected {} shape(s)", self.canvas.shapes.len()),
                        std::time::Instant::now(),
                    ));
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
                            self.notification = Some((
                                "Duplicated selection".to_string(),
                                std::time::Instant::now(),
                            ));
                        }
                    }
                }

                // Copy selection (Cmd/Ctrl + C)
                if has_shortcut(ui, egui::Key::C, true) {
                    if let Some(&idx) = self.primary_selected.as_ref() {
                        if idx < self.canvas.shapes.len() {
                            self.copied_shape = Some(self.canvas.shapes[idx].clone());
                            self.notification = Some((
                                "Copied shape to buffer".to_string(),
                                std::time::Instant::now(),
                            ));
                        }
                    }
                }

                // Delete Selection (supports multi-select)
                if ui.input(|i| {
                    i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                }) {
                    if self.editing_text_index.is_none() && self.has_selection() {
                        self.canvas.history.push(self.canvas.shapes.clone());
                        self.canvas.undo_history.clear();
                        self.is_dirty = true;
                        // Remove in reverse order to keep indices valid
                        let mut indices: Vec<usize> =
                            self.selected_shape_indices.iter().copied().collect();
                        indices.sort_unstable_by(|a, b| b.cmp(a));
                        for idx in indices {
                            if idx < self.canvas.shapes.len() {
                                self.canvas.shapes.remove(idx);
                            }
                        }
                        self.clear_selection();
                        self.notification =
                            Some(("Deleted shape(s)".to_string(), std::time::Instant::now()));
                    }
                }

                // Selection nudge controls (Arrow keys) — nudges ALL selected shapes
                if self.editing_text_index.is_none() && self.has_selection() {
                    let shift = ui.input(|i| i.modifiers.shift);
                    let dist = if shift { 10.0 } else { 1.0 };
                    let mut nudge_delta = egui::Vec2::ZERO;

                    if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                        nudge_delta.x = -dist;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                        nudge_delta.x = dist;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        nudge_delta.y = -dist;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        nudge_delta.y = dist;
                    }

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
                    // Split kugel boards (handled one at a time, may prompt) from
                    // image files (decoded + compressed in parallel below).
                    let mut image_paths: Vec<std::path::PathBuf> = Vec::new();
                    for file in dropped_files {
                        if let Some(path) = &file.path {
                            if path.extension().map_or(false, |ext| ext == "kugel") {
                                // Dropped a Kugel file! Prompt to save current board first
                                let mut proceed = true;
                                if !self.canvas.shapes.is_empty() {
                                    let confirm = rfd::MessageDialog::new()
                                        .set_title("Unsaved Changes")
                                        .set_description(
                                            "Do you want to save your current board first?",
                                        )
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
                                image_paths.push(path.clone());
                            }
                        }
                    }

                    if !image_paths.is_empty() {
                        // Decode + resize + JPEG-encode is CPU-bound and slow per
                        // image. Run one worker thread per file so a multi-image
                        // drop finishes in ~one image's time instead of the sum.
                        let results: Vec<Option<(Vec<u8>, [f32; 2])>> = std::thread::scope(|s| {
                            let handles: Vec<_> = image_paths
                                .iter()
                                .map(|path| {
                                    s.spawn(move || {
                                        let bytes = std::fs::read(path).ok()?;
                                        let img = image::load_from_memory(&bytes).ok()?;
                                        Self::compress_and_scale(img).ok()
                                    })
                                })
                                .collect();
                            handles.into_iter().map(|h| h.join().unwrap_or(None)).collect()
                        });

                        let center_screen = ctx.screen_rect().center();
                        let center_canvas = self.screen_to_canvas(center_screen);
                        let mut last_idx = None;
                        let mut count = 0;
                        for result in results {
                            if let Some((compressed_bytes, size)) = result {
                                // Cascade multiple drops so they don't stack exactly.
                                let offset = egui::vec2(20.0 * count as f32, 20.0 * count as f32);
                                let idx = self.canvas.add_image(
                                    center_canvas + offset,
                                    compressed_bytes,
                                    size,
                                    ctx,
                                );
                                last_idx = Some(idx);
                                count += 1;
                            }
                        }

                        if let Some(idx) = last_idx {
                            self.is_dirty = true;
                            self.select_single(idx);
                            self.tool = Tool::Select;
                            self.notification = Some((
                                if count == 1 {
                                    "Imported image".to_string()
                                } else {
                                    format!("Imported {} images", count)
                                },
                                std::time::Instant::now(),
                            ));
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
                                if let Some(pos) =
                                    response.hover_pos().or(response.interact_pointer_pos())
                                {
                                    self.screen_to_canvas(pos)
                                } else {
                                    start_canvas // fallback
                                }
                            };

                            let marquee_box = egui::Rect::from_two_pos(start_canvas, end_canvas);
                            if marquee_box.width() > 2.0 && marquee_box.height() > 2.0 {
                                // Select ALL shapes intersecting the marquee, but frames (SectionBox)
                                // are only selected if the marquee fully encloses the frame.
                                self.clear_selection();
                                for (idx, shape) in self.canvas.shapes.iter().enumerate() {
                                    let is_section = matches!(shape.data, ShapeData::SectionBox { .. });
                                    let shape_bounds = shape.data.get_bounds();
                                    let selected = if is_section {
                                        marquee_box.contains_rect(shape_bounds)
                                    } else {
                                        marquee_box.intersects(shape_bounds)
                                    };
                                    if selected {
                                        self.selected_shape_indices.insert(idx);
                                        self.primary_selected = Some(idx);
                                    }
                                }
                            }
                        }
                        self.is_resizing = None;
                        self.is_dragging_shape = false;
                        self.snap_correction = egui::Vec2::ZERO;
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

                            if primary_pressed
                                && press_pos.is_some()
                                && response.rect.contains(press_pos.unwrap())
                            {
                                let click_pos = press_pos.unwrap();
                                let click_canvas_pos = self.screen_to_canvas(click_pos);

                                // If currently editing a note/text, stop editing if user clicked outside it
                                if let Some(edit_idx) = self.editing_text_index {
                                    let clicked_edited_shape = edit_idx < self.canvas.shapes.len()
                                        && self.canvas.shapes[edit_idx]
                                            .data
                                            .contains_point(click_canvas_pos, 5.0);
                                    if !clicked_edited_shape {
                                        self.editing_text_index = None;
                                        self.request_text_focus = false;
                                        self.tool = Tool::Select;
                                    }
                                }

                                let mut clicked_handle = false;

                                if self.selected_shape_indices.len() > 1 {
                                    if let Some(handle_idx) =
                                        self.group_handle_under_mouse(click_pos)
                                    {
                                        self.is_resizing = Some(handle_idx);
                                        self.drag_start_pos = click_pos;
                                        clicked_handle = true;
                                    }
                                } else if let Some(selected_idx) = self.primary_selected {
                                    if selected_idx < self.canvas.shapes.len() {
                                        if let Some(handle_idx) =
                                            self.get_handle_under_mouse(selected_idx, click_pos)
                                        {
                                            self.is_resizing = Some(handle_idx);
                                            self.drag_start_pos = click_pos;
                                            clicked_handle = true;
                                        }
                                    }
                                }

                                if !clicked_handle {
                                    if let Some(idx) = self.hit_test(click_canvas_pos) {
                                        let shift = ui.input(|i| i.modifiers.shift);
                                        if shift {
                                            if self.selected_shape_indices.contains(&idx) {
                                                self.selected_shape_indices.remove(&idx);
                                                if self.primary_selected == Some(idx) {
                                                    self.primary_selected =
                                                        self.selected_shape_indices.iter().next().copied();
                                                }
                                            } else {
                                                self.selected_shape_indices.insert(idx);
                                                self.primary_selected = Some(idx);
                                            }
                                        } else if !self.selected_shape_indices.contains(&idx) {
                                            self.select_single(idx);
                                        }
                                        if ui.input(|i| i.modifiers.alt) {
                                            self.duplicate_selection(ctx);
                                        }
                                        self.is_dragging_shape = true;
                                        self.drag_start_pos = click_pos;
                                        self.snap_correction = egui::Vec2::ZERO;
                                        self.marquee_start = None;
                                    } else {
                                        // Clicking empty space: clear selection and start marquee
                                        if !ui.input(|i| i.modifiers.shift) {
                                            self.clear_selection();
                                        }
                                        self.marquee_start = Some(click_canvas_pos);
                                    }
                                }
                            }

                            // Hint that a link-only text is clickable while Cmd/Ctrl is held.
                            if response.hovered() {
                                let cmd = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
                                if cmd {
                                    if let Some(idx) = self.hit_test(canvas_pos) {
                                        if self.text_shape_url(idx).is_some() {
                                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                    }
                                }
                            }

                            // 2. Click to Deselect / Select fallback.
                            //    Cmd/Ctrl+click on a link-only text opens it in the browser.
                            if response.clicked() {
                                if let Some(idx) = self.hit_test(canvas_pos) {
                                    let cmd = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
                                    let shift = ui.input(|i| i.modifiers.shift);
                                    let url = if cmd { self.text_shape_url(idx) } else { None };
                                    if let Some(url) = url {
                                        ctx.open_url(egui::OpenUrl::new_tab(url));
                                    } else if !shift {
                                        self.select_single(idx);
                                    }
                                    self.marquee_start = None;
                                } else if !ui.input(|i| i.modifiers.shift) {
                                    self.clear_selection();
                                }
                            }

                            // 3. Double Click Text shape to Edit
                            if ui.input(|i| {
                                i.pointer
                                    .button_double_clicked(egui::PointerButton::Primary)
                            }) && response.hovered()
                            {
                                if let Some(idx) = self.hit_test(canvas_pos) {
                                    match &self.canvas.shapes[idx].data {
                                        ShapeData::Text { text, .. } => {
                                            self.editing_text_index = Some(idx);
                                            self.editing_text_buffer = text.clone();
                                            self.request_text_focus = true;
                                            self.select_single(idx);
                                            self.tool = Tool::Select;
                                            self.marquee_start = None;
                                        }
                                        ShapeData::StickyNote { text, .. } => {
                                            self.editing_text_index = Some(idx);
                                            self.editing_text_buffer = text.clone();
                                            self.request_text_focus = true;
                                            self.select_single(idx);
                                            self.tool = Tool::Select;
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
                                    if self.selected_shape_indices.len() > 1 {
                                        if let Some(bounds) = self.selection_bounds() {
                                            let anchor = match handle_idx {
                                                0 => bounds.right_bottom(),
                                                1 => bounds.left_bottom(),
                                                2 => bounds.right_top(),
                                                _ => bounds.left_top(),
                                            };
                                            let corner = match handle_idx {
                                                0 => bounds.left_top(),
                                                1 => bounds.right_top(),
                                                2 => bounds.left_bottom(),
                                                _ => bounds.right_bottom(),
                                            };
                                            let old_dist = corner.distance(anchor);
                                            let new_dist = canvas_pos.distance(anchor);
                                            if old_dist > 1.0 {
                                                let factor = (new_dist / old_dist).clamp(0.2, 5.0);
                                                if (factor - 1.0).abs() > 0.0001 {
                                                    for &idx in &self.selected_shape_indices {
                                                        if idx < self.canvas.shapes.len() {
                                                            self.canvas.shapes[idx]
                                                                .data
                                                                .scale_about(anchor, factor);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else if let Some(primary_idx) = self.primary_selected {
                                        if primary_idx < self.canvas.shapes.len() {
                                            self.canvas.shapes[primary_idx]
                                                .data
                                                .resize(handle_idx, delta, canvas_pos);
                                        }
                                    }
                                } else if self.is_dragging_shape {
                                    // Move to the raw (unsnapped) position: apply this frame's
                                    // delta and undo last frame's snap correction, so snapping
                                    // never accumulates or sticks.
                                    let to_raw = delta - self.snap_correction;
                                    for &idx in &self.selected_shape_indices {
                                        if idx < self.canvas.shapes.len() {
                                            self.canvas.shapes[idx].data.translate(to_raw);
                                        }
                                    }
                                    let mut correction = egui::Vec2::ZERO;
                                    if let Some(p) = self.primary_selected {
                                        if p < self.canvas.shapes.len() {
                                            let moving = self.canvas.shapes[p].data.get_bounds();
                                            let (corr, guides) = self
                                                .compute_alignment_snap(moving, 6.0 / self.zoom);
                                            correction = corr;
                                            alignment_guides = guides;
                                        }
                                    }
                                    if correction != egui::Vec2::ZERO {
                                        for &idx in &self.selected_shape_indices {
                                            if idx < self.canvas.shapes.len() {
                                                self.canvas.shapes[idx].data.translate(correction);
                                            }
                                        }
                                    }
                                    self.snap_correction = correction;
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

                            // Adjust cursor if hovering over handles
                            if self.selected_shape_indices.len() > 1 {
                                if let Some(handle_idx) = self.group_handle_under_mouse(pos) {
                                    let cursor = match handle_idx {
                                        0 | 3 => egui::CursorIcon::ResizeNwSe,
                                        _ => egui::CursorIcon::ResizeNeSw,
                                    };
                                    ctx.set_cursor_icon(cursor);
                                }
                            } else if let Some(selected_idx) = self.primary_selected {
                                if selected_idx < self.canvas.shapes.len() {
                                    if let Some(handle_idx) =
                                        self.get_handle_under_mouse(selected_idx, pos)
                                    {
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
                            if (self.tool == Tool::Text || self.tool == Tool::StickyNote)
                                && response.clicked()
                            {
                                // Only edit-in-place when the click lands on an
                                // existing text or sticky note; clicking any other
                                // object (or empty space) starts a fresh one.
                                let edit_existing = self.hit_test(canvas_pos).filter(|&idx| {
                                    matches!(
                                        self.canvas.shapes[idx].data,
                                        ShapeData::Text { .. } | ShapeData::StickyNote { .. }
                                    )
                                });
                                if let Some(idx) = edit_existing {
                                    let text = match &self.canvas.shapes[idx].data {
                                        ShapeData::Text { text, .. }
                                        | ShapeData::StickyNote { text, .. } => text.clone(),
                                        _ => String::new(),
                                    };
                                    self.editing_text_index = Some(idx);
                                    self.editing_text_buffer = text;
                                    self.request_text_focus = true;
                                    self.select_single(idx);
                                    self.tool = Tool::Select;
                                    self.marquee_start = None;
                                } else {
                                    // Empty space or a non-text object: start a new shape
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
                                        self.request_text_focus = true;
                                        self.select_single(idx);
                                        self.tool = Tool::Select;
                                    }
                                }
                            } else if self.tool != Tool::Text
                                && self.tool != Tool::StickyNote
                                && response.drag_started()
                            {
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
                                    self.request_text_focus = true;
                                    self.select_single(idx);
                                }
                            }

                            if response.dragged() {
                                self.canvas.update_current_shape(canvas_pos);
                            }

                            if response.drag_stopped() {
                                if let Some(idx) = self.canvas.finish_shape() {
                                    self.select_single(idx);
                                    self.tool = Tool::Select;
                                }
                                self.is_dirty = true;
                            }
                        }
                    }
                }

                // Draw canvas elements
                painter.set_clip_rect(response.rect);
                self.canvas.render(
                    &painter,
                    self.zoom,
                    self.pan_offset,
                    self.editing_text_index,
                );

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

                                // Resize handles on the primary shape (single-select only)
                                if self.primary_selected == Some(idx)
                                    && self.selected_shape_indices.len() == 1
                                {
                                    let is_text_or_sticky = matches!(
                                        self.canvas.shapes[idx].data,
                                        ShapeData::Text { .. } | ShapeData::StickyNote { .. }
                                    );
                                    let handle_positions = if is_text_or_sticky {
                                        vec![
                                            screen_bounds.right_top(),
                                            screen_bounds.right_bottom(),
                                        ]
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
                                            egui::Rect::from_center_size(
                                                h_pos,
                                                egui::vec2(8.0, 8.0),
                                            ),
                                            2.0,
                                            egui::Color32::WHITE,
                                            egui::Stroke::new(
                                                1.5,
                                                egui::Color32::from_rgb(99, 102, 241),
                                            ),
                                            egui::StrokeKind::Outside,
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // Group selection box + corner resize handles for multi-select
                    if self.selected_shape_indices.len() > 1 {
                        if let Some(bounds) = self.selection_bounds() {
                            let screen_bounds = egui::Rect::from_min_max(
                                self.canvas_to_screen(bounds.min),
                                self.canvas_to_screen(bounds.max),
                            );
                            painter.rect_stroke(
                                screen_bounds,
                                0.0,
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(99, 102, 241)),
                                egui::StrokeKind::Outside,
                            );
                            let corners = [
                                screen_bounds.left_top(),
                                screen_bounds.right_top(),
                                screen_bounds.left_bottom(),
                                screen_bounds.right_bottom(),
                            ];
                            for &c in &corners {
                                painter.rect(
                                    egui::Rect::from_center_size(c, egui::vec2(8.0, 8.0)),
                                    2.0,
                                    egui::Color32::WHITE,
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(99, 102, 241)),
                                    egui::StrokeKind::Outside,
                                );
                            }
                        }
                    }
                }

                // Alignment guides (drawn on top while dragging)
                for (a, b) in &alignment_guides {
                    painter.line_segment(
                        [self.canvas_to_screen(*a), self.canvas_to_screen(*b)],
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 60, 120)),
                    );
                }

                // Dynamic text dimensions caching & StickyNote bottom auto-resizing
                for shape in &mut self.canvas.shapes {
                    match &mut shape.data {
                        ShapeData::Text {
                            text,
                            size,
                            max_width,
                            cached_size,
                            cache_key,
                            ..
                        } => {
                            let mut hasher = DefaultHasher::new();
                            text.hash(&mut hasher);
                            size.to_bits().hash(&mut hasher);
                            max_width.map(|w| w.to_bits()).hash(&mut hasher);
                            let key = hasher.finish();
                            if *cache_key != Some(key) || cached_size.is_none() {
                                let font_id = egui::FontId::proportional(*size);
                                let galley = if let Some(mw) = max_width {
                                    ui.fonts(|f| {
                                        f.layout(text.clone(), font_id, egui::Color32::WHITE, *mw)
                                    })
                                } else {
                                    ui.fonts(|f| {
                                        f.layout_no_wrap(
                                            text.clone(),
                                            font_id,
                                            egui::Color32::WHITE,
                                        )
                                    })
                                };
                                *cached_size = Some(galley.size());
                                *cache_key = Some(key);
                            }
                        }
                        ShapeData::StickyNote {
                            rect,
                            text,
                            text_size,
                            cached_height,
                            cache_key,
                            ..
                        } => {
                            let padding = 16.0;
                            let text_width = (rect.width() - padding).max(10.0);
                            let mut hasher = DefaultHasher::new();
                            text.hash(&mut hasher);
                            text_size.to_bits().hash(&mut hasher);
                            text_width.to_bits().hash(&mut hasher);
                            let key = hasher.finish();
                            let required_height = if *cache_key == Some(key) {
                                cached_height.unwrap_or(140.0)
                            } else {
                                let font_id = egui::FontId::proportional(*text_size);
                                let galley = ui.fonts(|f| {
                                    f.layout(
                                        text.clone(),
                                        font_id,
                                        egui::Color32::WHITE,
                                        text_width,
                                    )
                                });
                                let h = (galley.size().y + padding).max(140.0);
                                *cached_height = Some(h);
                                *cache_key = Some(key);
                                h
                            };
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
                    ShapeData::Text {
                        pos, size, color, ..
                    } => (*pos, *size, *color),
                    ShapeData::StickyNote {
                        rect,
                        text_size,
                        text_color,
                        bg_color,
                        ..
                    } => {
                        let dark_mode = ctx.style().visuals.dark_mode;
                        let tc = if dark_mode && *bg_color == egui::Color32::from_rgb(255, 243, 176) {
                            egui::Color32::from_rgb(245, 235, 205)
                        } else {
                            *text_color
                        };
                        (rect.min + egui::vec2(8.0, 8.0), *text_size, tc)
                    }
                    _ => (egui::Pos2::ZERO, 24.0, egui::Color32::WHITE),
                };
                let screen_pos = self.canvas_to_screen(canvas_pos);

                egui::Area::new(egui::Id::new("inline_text_edit"))
                    .fixed_pos(screen_pos)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        let font_id = egui::FontId::proportional(text_size * self.zoom);

                        // Force the wrap width to exactly match the shape's inner width.
                        // egui otherwise clamps multiline wrap to the Area's available
                        // width, which shrinks near screen edges and mismatches the note.
                        let wrap_px: Option<f32> = match &self.canvas.shapes[idx].data {
                            ShapeData::StickyNote { rect, .. } => {
                                Some((rect.width() - 16.0) * self.zoom)
                            }
                            ShapeData::Text {
                                max_width: Some(mw),
                                ..
                            } => Some(mw * self.zoom),
                            _ => None,
                        };

                        let layout_font = font_id.clone();
                        let mut layouter =
                            move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _w: f32| {
                                let job = egui::text::LayoutJob::simple(
                                    buf.as_str().to_owned(),
                                    layout_font.clone(),
                                    text_color,
                                    wrap_px.unwrap_or(f32::INFINITY),
                                );
                                ui.fonts(|f| f.layout_job(job))
                            };

                        let mut text_edit =
                            egui::TextEdit::multiline(&mut self.editing_text_buffer)
                                .font(font_id)
                                .text_color(text_color)
                                .frame(false)
                                .margin(egui::Margin::same(0))
                                .layouter(&mut layouter);

                        if let Some(w) = wrap_px {
                            text_edit = text_edit.desired_width(w);
                        }

                        let response = ui.add(text_edit);
                        if self.request_text_focus {
                            response.request_focus();
                            self.request_text_focus = false;
                        }

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
                        self.check_and_spawn_title_preview_for_shape(idx, ctx);

                        // Close editor on escape, lost focus, or cmd+enter
                        let pressed_esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                        let pressed_cmd_enter = ui.input(|i| {
                            (i.modifiers.command || i.modifiers.ctrl)
                                && i.key_pressed(egui::Key::Enter)
                        });

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
                            self.check_and_spawn_title_preview_for_shape(idx, ctx);
                            self.editing_text_index = None;
                            self.tool = Tool::Select;
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
                        ui.add(
                            egui::Slider::new(&mut self.export_scale, 0.5..=4.0)
                                .text("Resolution Scale"),
                        );
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.export_jpeg, false, "PNG (Lossless)");
                            ui.radio_value(&mut self.export_jpeg, true, "JPEG");
                        });
                        if self.export_jpeg {
                            ui.add(
                                egui::Slider::new(&mut self.export_quality, 10..=100)
                                    .text("JPEG Quality"),
                            );
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
                        .inner_margin(egui::Margin::symmetric(20, 8)) // horizontal and vertical padding
                        .show(ui, |ui| {
                            ui.set_min_width(320.0);
                            ui.set_max_width(600.0);
                            let font_id = egui::FontId::proportional(14.0);
                            let mut job = egui::text::LayoutJob::simple(
                                msg.clone(),
                                font_id,
                                egui::Color32::WHITE,
                                600.0, // wrap width
                            );
                            job.halign = egui::Align::Center;
                            job.wrap.max_rows = 2;
                            job.wrap.break_anywhere = false;
                            job.wrap.overflow_character = Some('…');
                            ui.add(egui::Label::new(job));
                        });
                });
        }
    }
}

impl App {
    fn compress_and_scale(img: image::DynamicImage) -> Result<(Vec<u8>, [f32; 2]), String> {
        // Whether any pixel is actually non-opaque. An RGBA source with every
        // alpha == 255 is treated as opaque so it can go down the JPEG path.
        fn has_transparency(img: &image::DynamicImage) -> bool {
            use image::GenericImageView;
            img.pixels().any(|(_, _, px)| px.0[3] != 255)
        }

        let width = img.width();
        let height = img.height();
        let short_side = width.min(height);

        // Scale DOWN only — never enlarge. Cap the short side so huge camera
        // originals shrink to a screen-reasonable size before encoding.
        const MAX_SHORT_SIDE: u32 = 1600;
        let scaled_img = if short_side > MAX_SHORT_SIDE {
            let scale = MAX_SHORT_SIDE as f32 / short_side as f32;
            let new_w = (width as f32 * scale) as u32;
            let new_h = (height as f32 * scale) as u32;
            img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        let out_w = scaled_img.width();
        let out_h = scaled_img.height();
        let mut compressed_bytes = Vec::new();

        // Opaque images -> JPEG (strong compression). Images with real
        // transparency -> PNG, since JPEG can't carry an alpha channel.
        if scaled_img.color().has_alpha() && has_transparency(&scaled_img) {
            let encoder = image::codecs::png::PngEncoder::new(&mut compressed_bytes);
            scaled_img
                .write_with_encoder(encoder)
                .map_err(|e| e.to_string())?;
        } else {
            const JPEG_QUALITY: u8 = 75;
            let rgb = scaled_img.to_rgb8();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut compressed_bytes, JPEG_QUALITY);
            encoder
                .encode_image(&rgb)
                .map_err(|e| e.to_string())?;
        }

        Ok((compressed_bytes, [out_w as f32, out_h as f32]))
    }

    fn try_paste_clipboard_image(&mut self, ctx: &egui::Context) -> bool {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(image) = clipboard.get_image() {
                if let Some(rgba) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                    image.width as u32,
                    image.height as u32,
                    image.bytes.into_owned(),
                ) {
                    let dynamic_img = image::DynamicImage::ImageRgba8(rgba);
                    if let Ok((compressed_bytes, size)) = Self::compress_and_scale(dynamic_img) {
                        let center_canvas = self.paste_target_canvas(ctx);
                        let idx = self
                            .canvas
                            .add_image(center_canvas, compressed_bytes, size, ctx);
                        self.select_single(idx);
                        self.tool = Tool::Select;
                        self.notification = Some((
                            "Pasted image from clipboard".to_string(),
                            std::time::Instant::now(),
                        ));
                        return true;
                    }
                }
            }
        }
        false
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
                        match Self::compress_and_scale(dynamic_img) {
                            Ok((compressed_bytes, size)) => {
                                let center_canvas = self.paste_target_canvas(ctx);
                                let idx = self.canvas.add_image(
                                    center_canvas,
                                    compressed_bytes,
                                    size,
                                    ctx,
                                );
                                self.select_single(idx);
                                self.tool = Tool::Select;
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
                                match Self::compress_and_scale(img) {
                                    Ok((compressed_bytes, size)) => {
                                        let center_canvas = self.paste_target_canvas(ctx);
                                        let idx = self.canvas.add_image(
                                            center_canvas,
                                            compressed_bytes,
                                            size,
                                            ctx,
                                        );
                                        self.select_single(idx);
                                        self.tool = Tool::Select;
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

                    // Check if pasted text contains a web link for title preview
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
                    self.check_and_spawn_title_preview_for_shape(idx, ctx);
                    self.is_dirty = true;
                    self.select_single(idx);
                    self.tool = Tool::Select;
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

    fn generate_missing_link_previews(&mut self, ctx: &egui::Context) {
        let count = self.canvas.shapes.len();
        for idx in 0..count {
            self.check_and_spawn_title_preview_for_shape(idx, ctx);
        }
    }

    fn check_and_spawn_title_preview_for_shape(&mut self, shape_idx: usize, ctx: &egui::Context) {
        if shape_idx >= self.canvas.shapes.len() {
            return;
        }
        let shape = &mut self.canvas.shapes[shape_idx];
        let text_content = match &shape.data {
            ShapeData::Text { text, .. } | ShapeData::StickyNote { text, .. } => text.clone(),
            _ => return,
        };

        let is_url = extract_url(&text_content);
        if let Some(url) = is_url {
            let current_url = shape.data.link_url().map(String::from);
            if current_url != Some(url.clone()) {
                shape.data.set_link_url(Some(url.clone()));
                let fallback = format!("🌐 {}", truncate_title(&domain_fallback(&url), 55));
                shape.data.set_link_title(Some(fallback));
                self.is_dirty = true;

                let shape_id = shape.id;
                let ui_tx = self.ui_event_tx.clone();
                let ctx_clone = ctx.clone();
                let url_clone = url;

                std::thread::spawn(move || {
                    let raw_title = fetch_website_title(&url_clone)
                        .unwrap_or_else(|| domain_fallback(&url_clone));
                    let title = truncate_title(&raw_title, 55);
                    let _ = ui_tx.send(UiEvent::LinkTitleFetched {
                        shape_id,
                        url: url_clone,
                        title: format!("🌐 {title}"),
                    });
                    ctx_clone.request_repaint();
                });
            }
        } else if shape.data.link_title().is_some() || shape.data.link_url().is_some() {
            shape.data.set_link_title(None);
            shape.data.set_link_url(None);
            self.is_dirty = true;
        }
    }

    fn import_image_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Image Files", &["png", "jpg", "jpeg", "webp", "gif"])
            .pick_file()
        {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    match Self::compress_and_scale(img) {
                        Ok((compressed_bytes, size)) => {
                            let center_canvas = self.paste_target_canvas(ctx);
                            let idx =
                                self.canvas
                                    .add_image(center_canvas, compressed_bytes, size, ctx);
                            self.is_dirty = true;
                            self.select_single(idx);
                            self.tool = Tool::Select;
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

    /// Start a fresh, empty board. Prompts to save the current one first if it
    /// has unsaved changes.
    fn new_board(&mut self) {
        let empty_unsaved = self.canvas.shapes.is_empty() && self.current_file_path.is_none();
        if self.is_dirty && !empty_unsaved {
            let confirm = rfd::MessageDialog::new()
                .set_title("Unsaved Changes")
                .set_description("Do you want to save the current board before creating a new one?")
                .set_buttons(rfd::MessageButtons::YesNoCancel)
                .show();
            match confirm {
                rfd::MessageDialogResult::Yes => {
                    // Abort if the save is cancelled or fails, keeping the board.
                    if !self.save() {
                        return;
                    }
                }
                rfd::MessageDialogResult::No => {}
                _ => return, // Cancel: keep the current board
            }
        }

        self.canvas = Canvas::default();
        self.current_file_path = None;
        self.is_dirty = false;
        self.clear_selection();
        self.editing_text_index = None;
        self.zoom = 1.0;
        self.pan_offset = egui::Vec2::ZERO;
        self.notification = Some(("New board created".to_string(), std::time::Instant::now()));
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
        let filter_name = if self.export_jpeg {
            "JPEG Image"
        } else {
            "PNG Image"
        };
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

fn platform_asset_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "kugel-macos.app.tar.gz"
    }
    #[cfg(target_os = "linux")]
    {
        "kugel-linux"
    }
    #[cfg(target_os = "windows")]
    {
        "kugel-windows.exe"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        ""
    }
}

fn spawn_update_check(ui_tx: std::sync::mpsc::Sender<UiEvent>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let result = (|| -> Result<(String, String, String), String> {
            let client = reqwest::blocking::Client::builder()
                .user_agent("kugel-updater")
                .build()
                .map_err(|e| e.to_string())?;
            let resp: serde_json::Value = client
                .get("https://api.github.com/repos/salernoelia/kugel/releases/latest")
                .send()
                .map_err(|e| e.to_string())?
                .json()
                .map_err(|e| e.to_string())?;
            let tag = resp["tag_name"]
                .as_str()
                .ok_or("Missing tag_name")?
                .trim_start_matches('v')
                .to_string();
            let html_url = resp["html_url"]
                .as_str()
                .ok_or("Missing html_url")?
                .to_string();
            let asset_name = platform_asset_name();
            let download_url = resp["assets"]
                .as_array()
                .and_then(|assets| {
                    assets
                        .iter()
                        .find(|a| a["name"].as_str().map(|n| n == asset_name).unwrap_or(false))
                })
                .and_then(|a| a["browser_download_url"].as_str())
                .unwrap_or("")
                .to_string();
            Ok((tag, html_url, download_url))
        })();

        match result {
            Ok((latest, html_url, download_url)) => {
                let current = env!("CARGO_PKG_VERSION");
                if latest != current {
                    let _ = ui_tx.send(UiEvent::UpdateAvailable {
                        version: latest,
                        html_url,
                        download_url,
                    });
                } else {
                    let _ = ui_tx.send(UiEvent::UpToDate);
                }
            }
            Err(e) => {
                let _ = ui_tx.send(UiEvent::UpdateCheckFailed(e));
            }
        }
        ctx.request_repaint();
    });
}

fn do_self_update(download_url: &str) -> Result<(), String> {
    let tmp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        let archive_path = tmp_dir.path().join("kugel-update.tar.gz");
        let mut archive_file = std::fs::File::create(&archive_path).map_err(|e| e.to_string())?;
        let client = reqwest::blocking::Client::builder()
            .user_agent("kugel-updater")
            .build()
            .map_err(|e| e.to_string())?;
        let bytes = client
            .get(download_url)
            .send()
            .and_then(|r| r.bytes())
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut bytes.as_ref(), &mut archive_file).map_err(|e| e.to_string())?;

        let file = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);

        let extract_to = tmp_dir.path().join("kugel_bin");
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let entry_path = entry.path().map_err(|e| e.to_string())?;
            let file_name = entry_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if file_name == "kugel" {
                entry.unpack(&extract_to).map_err(|e| e.to_string())?;
                break;
            }
        }

        if !extract_to.exists() {
            return Err("Could not find kugel binary inside update archive".to_string());
        }

        self_replace::self_replace(&extract_to).map_err(|e| e.to_string())?;

        // Re-sign the .app bundle after binary replacement.
        if let Ok(exe_path) = std::env::current_exe() {
            let mut current = exe_path.as_path();
            let mut bundle_path: Option<std::path::PathBuf> = None;
            loop {
                if current.extension().and_then(|e| e.to_str()) == Some("app") {
                    bundle_path = Some(current.to_path_buf());
                    break;
                }
                match current.parent() {
                    Some(p) => current = p,
                    None => break,
                }
            }
            if let Some(bundle) = bundle_path {
                let _ = std::process::Command::new("codesign")
                    .args(["-s", "-", "--deep", "--force"])
                    .arg(&bundle)
                    .output();
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let bin_path = tmp_dir.path().join("kugel_new");
        let client = reqwest::blocking::Client::builder()
            .user_agent("kugel-updater")
            .build()
            .map_err(|e| e.to_string())?;
        let bytes = client
            .get(download_url)
            .send()
            .and_then(|r| r.bytes())
            .map_err(|e| e.to_string())?;
        std::fs::write(&bin_path, &bytes).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms).map_err(|e| e.to_string())?;
        }

        self_replace::self_replace(&bin_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Extract the first URL found in text, if any.
fn extract_url(text: &str) -> Option<String> {
    for word in text.trim().split_whitespace() {
        let clean = word.trim_matches(|c: char| {
            c == '('
                || c == ')'
                || c == '<'
                || c == '>'
                || c == '"'
                || c == '\''
                || c == ','
                || c == ';'
                || c == '!'
                || c == '?'
        });
        if clean.starts_with("http://") || clean.starts_with("https://") {
            return Some(clean.to_string());
        } else if clean.starts_with("www.") {
            return Some(format!("https://{clean}"));
        } else if clean.contains('.') && !clean.contains('@') && !clean.ends_with('.') {
            let parts: Vec<&str> = clean.split('/').next().unwrap_or("").split('.').collect();
            if parts.len() >= 2 {
                let tld = parts.last().unwrap_or(&"");
                if matches!(
                    *tld,
                    "com"
                        | "org"
                        | "net"
                        | "io"
                        | "dev"
                        | "app"
                        | "ai"
                        | "co"
                        | "uk"
                        | "de"
                        | "fr"
                        | "it"
                        | "es"
                        | "ca"
                        | "me"
                        | "info"
                        | "tech"
                        | "xyz"
                ) {
                    return Some(format!("https://{clean}"));
                }
            }
        }
    }
    None
}

/// Fallback domain name from a URL for immediate preview.
fn domain_fallback(url: &str) -> String {
    let clean = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    let host = clean.split('/').next().unwrap_or(clean);
    if host.is_empty() {
        "website".to_string()
    } else {
        host.to_string()
    }
}

/// Truncate a title to a maximum number of characters cleanly.
fn truncate_title(title: &str, max_chars: usize) -> String {
    let clean = title.trim();
    if clean.chars().count() > max_chars {
        let truncated: String = clean.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated.trim())
    } else {
        clean.to_string()
    }
}

/// Fetch website HTML title in background thread.
fn fetch_website_title(url: &str) -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .ok()?;

    let response = client.get(url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().ok()?;

    let lower_body = body.to_lowercase();
    let start_idx = lower_body.find("<title")?;
    let rest = &body[start_idx..];
    let tag_end = rest.find('>')? + 1;
    let content_rest = &rest[tag_end..];
    let end_idx = content_rest.to_lowercase().find("</title")?;

    let raw_title = &content_rest[..end_idx];
    let cleaned = raw_title
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace('\r', " ")
        .replace('\n', " ");

    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let title = words.join(" ");
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_url() {
        assert_eq!(
            extract_url("Check out https://github.com/salernoelia/kugel!"),
            Some("https://github.com/salernoelia/kugel".to_string())
        );
        assert_eq!(
            extract_url("www.example.com/test"),
            Some("https://www.example.com/test".to_string())
        );
        assert_eq!(
            extract_url("Visit google.com for search"),
            Some("https://google.com".to_string())
        );
        assert_eq!(extract_url("just plain text without link"), None);
    }

    #[test]
    fn test_domain_fallback() {
        assert_eq!(
            domain_fallback("https://github.com/salernoelia/kugel"),
            "github.com"
        );
        assert_eq!(
            domain_fallback("https://www.news.ycombinator.com/item?id=123"),
            "news.ycombinator.com"
        );
    }

    #[test]
    fn test_truncate_title() {
        assert_eq!(truncate_title("Short Title", 20), "Short Title");
        assert_eq!(
            truncate_title("Very Long Website Title That Exceeds Limit", 20),
            "Very Long Website..."
        );
    }

    #[test]
    fn test_shift_toggle_selection() {
        let mut app = App::default();
        app.canvas.add_text(egui::pos2(0.0, 0.0), "Item 1".into(), egui::Color32::WHITE);
        app.canvas.add_text(egui::pos2(100.0, 100.0), "Item 2".into(), egui::Color32::WHITE);

        // Select item 0
        app.select_single(0);
        assert!(app.selected_shape_indices.contains(&0));
        assert!(!app.selected_shape_indices.contains(&1));

        // Shift-add item 1
        app.selected_shape_indices.insert(1);
        assert!(app.selected_shape_indices.contains(&0));
        assert!(app.selected_shape_indices.contains(&1));

        // Shift-remove item 0 (deselect single shape)
        app.selected_shape_indices.remove(&0);
        assert!(!app.selected_shape_indices.contains(&0));
        assert!(app.selected_shape_indices.contains(&1));
    }

    #[test]
    fn test_link_preview_cleared_when_not_link() {
        let mut app = App::default();
        let ctx = egui::Context::default();
        let idx = app.canvas.add_text(egui::pos2(0.0, 0.0), "https://github.com".into(), egui::Color32::WHITE);
        app.check_and_spawn_title_preview_for_shape(idx, &ctx);
        assert!(app.canvas.shapes[idx].data.link_title().is_some());

        // Change text to non-link
        if let ShapeData::Text { text, .. } = &mut app.canvas.shapes[idx].data {
            *text = "Just plain text without a link".to_string();
        }
        app.check_and_spawn_title_preview_for_shape(idx, &ctx);
        assert!(app.canvas.shapes[idx].data.link_title().is_none());
        assert!(app.canvas.shapes[idx].data.link_url().is_none());
    }

    #[test]
    fn test_link_preview_updated_when_url_changes() {
        let mut app = App::default();
        let ctx = egui::Context::default();
        let idx = app.canvas.add_text(egui::pos2(0.0, 0.0), "https://github.com".into(), egui::Color32::WHITE);
        app.check_and_spawn_title_preview_for_shape(idx, &ctx);
        assert_eq!(app.canvas.shapes[idx].data.link_url(), Some("https://github.com"));

        // Change text to different URL
        if let ShapeData::Text { text, .. } = &mut app.canvas.shapes[idx].data {
            *text = "https://wikipedia.org".to_string();
        }
        app.check_and_spawn_title_preview_for_shape(idx, &ctx);
        assert_eq!(app.canvas.shapes[idx].data.link_url(), Some("https://wikipedia.org"));
        assert!(app.canvas.shapes[idx].data.link_title().unwrap().contains("wikipedia.org"));
    }
}


