use crate::net::protocol::RemoteUser;
use eframe::egui;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct RemoteCursorState {
    pub user_id: String,
    pub pos: egui::Pos2,
    pub selected_ids: Vec<usize>,
    pub last_update: Instant,
}

pub fn render_remote_presence_avatars(ui: &mut egui::Ui, remote_users: &[RemoteUser]) {
    if remote_users.is_empty() {
        return;
    }

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        for user in remote_users {
            let color = egui::Color32::from_rgba_unmultiplied(
                user.color[0],
                user.color[1],
                user.color[2],
                user.color[3],
            );

            let initial = user
                .display_name
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();

            let (rect, _response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 12.0, color);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                initial,
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        }
    });
}

pub fn render_remote_cursors(
    painter: &egui::Painter,
    remote_cursors: &HashMap<String, RemoteCursorState>,
    remote_users: &[RemoteUser],
    zoom: f32,
    pan_offset: egui::Vec2,
) {
    let now = Instant::now();
    for cursor in remote_cursors.values() {
        // Hide stale cursors (> 10s without update)
        if now.duration_since(cursor.last_update).as_secs() > 10 {
            continue;
        }

        let user = remote_users.iter().find(|u| u.id == cursor.user_id);
        let color = if let Some(u) = user {
            egui::Color32::from_rgba_unmultiplied(u.color[0], u.color[1], u.color[2], u.color[3])
        } else {
            egui::Color32::from_rgb(239, 68, 68) // Red fallback
        };

        let screen_pos = cursor.pos * zoom + pan_offset;

        // Draw cursor pointer triangle
        let points = vec![
            screen_pos,
            screen_pos + egui::vec2(0.0, 16.0),
            screen_pos + egui::vec2(12.0, 12.0),
        ];

        painter.add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        ));

        // Draw name label tag
        let display_name = user
            .map(|u| u.display_name.as_str())
            .unwrap_or(&cursor.user_id);

        let label_pos = screen_pos + egui::vec2(14.0, 14.0);
        let text_rect = egui::Rect::from_min_size(label_pos, egui::vec2(80.0, 18.0));

        painter.rect_filled(
            text_rect.expand(2.0),
            egui::CornerRadius::same(4),
            color.linear_multiply(0.85),
        );
        painter.text(
            text_rect.min + egui::vec2(4.0, 2.0),
            egui::Align2::LEFT_TOP,
            display_name,
            egui::FontId::proportional(11.0),
            egui::Color32::WHITE,
        );
    }
}
