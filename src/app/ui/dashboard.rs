use eframe::egui;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RecentBoard {
    pub path: PathBuf,
    pub title: String,
    pub is_cloud: bool,
    pub last_opened_ms: u64,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct RecentsStore {
    pub boards: Vec<RecentBoard>,
}

impl RecentsStore {
    fn recents_path() -> PathBuf {
        let base = dirs_next::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"));
        base.join("kugel").join("recents.json")
    }

    pub fn load() -> Self {
        let path = Self::recents_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(store) = serde_json::from_str::<RecentsStore>(&content) {
                    return store;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::recents_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn add_recent(&mut self, path: &Path, title: String, is_cloud: bool) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Remove duplicate if already present
        self.boards.retain(|b| b.path != path);

        self.boards.insert(
            0,
            RecentBoard {
                path: path.to_path_buf(),
                title,
                is_cloud,
                last_opened_ms: now_ms,
            },
        );

        // Keep top 20 recent files
        if self.boards.len() > 20 {
            self.boards.truncate(20);
        }

        let _ = self.save();
    }
}

pub struct DashboardModal {
    pub show: bool,
    pub recents: RecentsStore,
    pub cloud_room_input: String,
}

impl Default for DashboardModal {
    fn default() -> Self {
        Self {
            show: false,
            recents: RecentsStore::load(),
            cloud_room_input: String::new(),
        }
    }
}

pub enum DashboardAction {
    NewLocalBoard,
    CreateCloudRoom,
    JoinRoom(String),
    OpenRecent(PathBuf),
    OpenFileDialog,
}

impl DashboardModal {
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<DashboardAction> {
        if !self.show {
            return None;
        }

        let mut action: Option<DashboardAction> = None;

        egui::Window::new("Kugel Dashboard")
            .collapsible(false)
            .resizable(false)
            .fixed_size([640.0, 480.0])
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.heading("Welcome to Kugel");
                ui.label("Minimalist, high-performance collaborative mood boards");

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new("📄 New Local Board").min_size(egui::vec2(140.0, 36.0)))
                        .clicked()
                    {
                        action = Some(DashboardAction::NewLocalBoard);
                        self.show = false;
                    }

                    if ui
                        .add(egui::Button::new("☁️ Create Cloud Room").min_size(egui::vec2(150.0, 36.0)))
                        .clicked()
                    {
                        action = Some(DashboardAction::CreateCloudRoom);
                        self.show = false;
                    }

                    if ui
                        .add(egui::Button::new("📂 Open File...").min_size(egui::vec2(120.0, 36.0)))
                        .clicked()
                    {
                        action = Some(DashboardAction::OpenFileDialog);
                        self.show = false;
                    }
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label("Room Code / Link:");
                    let text_response = ui.add(
                        egui::TextEdit::singleline(&mut self.cloud_room_input)
                            .hint_text("Paste room ID or kugel://room/...")
                            .desired_width(280.0),
                    );
                    let join_clicked = ui.button("Join Room").clicked()
                        || (text_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));

                    if join_clicked && !self.cloud_room_input.trim().is_empty() {
                        let mut room_id = self.cloud_room_input.trim();
                        if let Some(stripped) = room_id.strip_prefix("kugel://room/") {
                            room_id = stripped;
                        }
                        action = Some(DashboardAction::JoinRoom(room_id.to_string()));
                        self.show = false;
                    }
                });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Recent Boards").strong());

                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        if self.recents.boards.is_empty() {
                            ui.label("No recent boards found.");
                        } else {
                            for recent in self.recents.boards.clone() {
                                ui.horizontal(|ui| {
                                    let badge = if recent.is_cloud { "☁️ Cloud" } else { "📄 Local" };
                                    ui.colored_label(
                                        if recent.is_cloud {
                                            egui::Color32::from_rgb(129, 140, 248)
                                        } else {
                                            egui::Color32::from_rgb(156, 163, 175)
                                        },
                                        badge,
                                    );

                                    let name = recent
                                        .path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or(&recent.title);

                                    if ui.button(format!("{} ({})", recent.title, name)).clicked() {
                                        action = Some(DashboardAction::OpenRecent(recent.path));
                                        self.show = false;
                                    }
                                });
                                ui.add_space(4.0);
                            }
                        }
                    });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        self.show = false;
                    }
                });
            });

        action
    }
}

mod dirs_next {
    use std::path::PathBuf;
    pub fn data_local_dir() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = std::env::var_os("HOME") {
                return Some(PathBuf::from(home).join("Library").join("Application Support"));
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            if let Some(home) = std::env::var_os("HOME") {
                return Some(PathBuf::from(home).join(".local").join("share"));
            }
        }
        None
    }
}
