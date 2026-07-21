use crate::app::App;
use eframe::egui;

pub fn render_export_dialog(app: &mut App, ctx: &egui::Context) {
    if app.show_export_dialog {
        egui::Window::new("Export Canvas")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label("Export the active canvas bounds to an image file:");
                    ui.add(
                        egui::Slider::new(&mut app.export_scale, 0.5..=4.0)
                            .text("Resolution Scale"),
                    );
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut app.export_jpeg, false, "PNG (Lossless)");
                        ui.radio_value(&mut app.export_jpeg, true, "JPEG");
                    });
                    if app.export_jpeg {
                        ui.add(
                            egui::Slider::new(&mut app.export_quality, 10..=100)
                                .text("JPEG Quality"),
                        );
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Export to file").clicked() {
                            app.export_file_dialog();
                        }
                        if ui.button("Cancel").clicked() {
                            app.show_export_dialog = false;
                        }
                    });
                });
            });
    }
}
