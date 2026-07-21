pub mod font;
pub mod file_io;
pub mod link_preview;
pub mod selection;
pub mod ui;

use crate::canvas::Canvas;
use crate::icons::Icons;
use crate::shapes::{Shape, Tool};
use crate::updater::{spawn_update_check, UiEvent, UpdateState};
use eframe::egui;
use font::setup_custom_fonts;
use std::collections::HashSet;
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
                        }
                    }
                }
            }
        }
    }
}
