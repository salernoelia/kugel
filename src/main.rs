mod app;
mod canvas;
mod export;
mod icons;
mod image_utils;
#[cfg(target_os = "macos")]
mod macos_open;
mod markdown;
mod shapes;
mod state;
mod updater;
mod url_utils;

use app::App;
use eframe::egui;

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
