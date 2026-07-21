use eframe::egui;
use std::sync::Arc;

/// Install OpenSans as the global default font for both proportional and
/// monospace families, matching the bundled font used on export.
pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "OpenSans".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/OpenSans-Regular.ttf"
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
