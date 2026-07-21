use crate::app::App;
use crate::icons::Icons;
use crate::updater::UpdateState;
use eframe::egui;

pub fn render_top_left_controls(
    app: &mut App,
    ctx: &egui::Context,
    icons: &Icons,
    panel_bg: egui::Color32,
    panel_stroke: egui::Stroke,
    is_dark: bool,
) {
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
                                    if app.top_panel_collapsed {
                                        "Show Settings"
                                    } else {
                                        "Hide Settings"
                                    },
                                )
                                .clicked()
                            {
                                app.top_panel_collapsed = !app.top_panel_collapsed;
                            }
                        });
                        if app.top_panel_collapsed {
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
                                &mut app.background_color,
                                egui::color_picker::Alpha::Opaque,
                            );
                        });
                        ui.checkbox(&mut app.use_grid, "Show Grid");
                        ui.horizontal(|ui| {
                            let theme_icon = if app.dark_mode {
                                &icons.theme_light
                            } else {
                                &icons.theme_dark
                            };
                            if icons
                                .icon_button(
                                    ui,
                                    theme_icon,
                                    if app.dark_mode {
                                        "Switch to Light Theme"
                                    } else {
                                        "Switch to Dark Theme"
                                    },
                                )
                                .clicked()
                            {
                                app.dark_mode = !app.dark_mode;
                                if app.dark_mode {
                                    if app.background_color == egui::Color32::from_gray(240) {
                                        app.background_color =
                                            egui::Color32::from_rgb(20, 20, 23);
                                    }
                                } else {
                                    if app.background_color
                                        == egui::Color32::from_rgb(20, 20, 23)
                                    {
                                        app.background_color = egui::Color32::from_gray(240);
                                    }
                                }
                                app.style_applied = false;
                            }
                            if ui.button("Reset View").clicked() {
                                app.zoom = 1.0;
                                app.pan_offset = egui::Vec2::ZERO;
                            }
                        });

                        enum UpdateAction {
                            None,
                            CheckUpdates,
                            PerformUpdate(String),
                        }
                        let busy = matches!(
                            app.update_state,
                            UpdateState::Checking | UpdateState::Updating
                        );
                        let update_info: Option<(String, String, String)> =
                            if let UpdateState::UpdateAvailable {
                                version,
                                html_url,
                                download_url,
                            } = &app.update_state
                            {
                                Some((version.clone(), html_url.clone(), download_url.clone()))
                            } else {
                                None
                            };
                        let update_err: Option<String> =
                            if let UpdateState::Failed(e) = &app.update_state {
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
                            match &app.update_state {
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
                            UpdateAction::CheckUpdates => app.check_for_updates(ctx),
                            UpdateAction::PerformUpdate(url) => {
                                app.perform_self_update(url, ctx)
                            }
                            UpdateAction::None => {}
                        }
                    });
                });
        });
}
