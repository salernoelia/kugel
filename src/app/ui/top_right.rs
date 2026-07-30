use crate::app::App;
use crate::net::client::SyncStatus;
use eframe::egui;

pub fn render_top_right_collaboration_header(app: &mut App, ctx: &egui::Context, is_dark: bool) {
    let panel_bg = if is_dark {
        egui::Color32::from_black_alpha(210)
    } else {
        egui::Color32::from_white_alpha(230)
    };
    let panel_stroke = if is_dark {
        egui::Stroke::new(1.0, egui::Color32::from_gray(60))
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_gray(180))
    };

    egui::Area::new(egui::Id::new("top_right_collaboration"))
        .anchor(egui::Align2::RIGHT_TOP, [-20.0, 20.0])
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(panel_bg)
                .stroke(panel_stroke)
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(net) = &app.net_client {
                            let room_id = net.room_id.clone();
                            let room_short = if room_id.len() > 8 {
                                &room_id[..8]
                            } else {
                                &room_id
                            };

                            let (badge_text, color) = match app.sync_status {
                                SyncStatus::Live => (format!("🟢 Live ({room_short})"), egui::Color32::from_rgb(34, 197, 94)),
                                SyncStatus::Syncing | SyncStatus::Connecting => (format!("🟡 Syncing ({room_short})"), egui::Color32::from_rgb(234, 179, 8)),
                                SyncStatus::Disconnected => ("🔴 Offline".to_string(), egui::Color32::from_rgb(239, 68, 68)),
                                SyncStatus::Error(_) => ("🔴 Error".to_string(), egui::Color32::from_rgb(239, 68, 68)),
                            };

                            ui.colored_label(color, badge_text);

                            // Participant Avatars
                            crate::app::ui::presence::render_remote_presence_avatars(ui, &app.remote_users);

                            // Copy Share Link Button
                            if ui.button("🔗 Share Link").on_hover_text("Copy room link to clipboard").clicked() {
                                let server_url = &net.server_url;
                                let share_url = format!("kugel://room/{room_id}?server={server_url}");
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    let _ = cb.set_text(share_url);
                                    app.notification = Some((
                                        "Room link copied to clipboard!".to_string(),
                                        std::time::Instant::now(),
                                    ));
                                }
                            }

                            // Disconnect / Leave Room Button
                            if ui.button("🔌 Leave").on_hover_text("Disconnect from cloud room").clicked() {
                                app.leave_room();
                            }
                        } else {
                            if ui.button("☁️ Start Collaboration").on_hover_text("Create or join a live collaborative room").clicked() {
                                app.create_cloud_room(ctx);
                            }
                        }
                    });
                });
        });
}
