use crate::app::App;
use eframe::egui;

pub fn render_toast_notification(app: &App, ctx: &egui::Context) {
    if let Some((msg, _)) = &app.notification {
        egui::Area::new(egui::Id::new("notification"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, 20.0])
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(31, 41, 55))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(75, 85, 99)))
                    .corner_radius(egui::CornerRadius::same(20))
                    .inner_margin(egui::Margin::symmetric(20, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(320.0);
                        ui.set_max_width(600.0);
                        let font_id = egui::FontId::proportional(14.0);
                        let mut job = egui::text::LayoutJob::simple(
                            msg.clone(),
                            font_id,
                            egui::Color32::WHITE,
                            600.0,
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
