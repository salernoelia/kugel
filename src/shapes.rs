use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Tool {
    Select,
    Pen,
    Rectangle,
    Circle,
    Text,
    StickyNote,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Shape {
    pub id: usize,
    pub data: ShapeData,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum ShapeData {
    Pen {
        points: Vec<egui::Pos2>,
        color: egui::Color32,
        stroke_width: f32,
    },
    Rectangle {
        rect: egui::Rect,
        color: egui::Color32,
        stroke_width: f32,
        filled: bool,
    },
    Circle {
        center: egui::Pos2,
        radius: f32,
        color: egui::Color32,
        stroke_width: f32,
        filled: bool,
    },
    Text {
        pos: egui::Pos2,
        text: String,
        color: egui::Color32,
        size: f32,
        #[serde(default)]
        max_width: Option<f32>,
        #[serde(skip)]
        cached_size: Option<egui::Vec2>,
    },
    Image {
        rect: egui::Rect,
        bytes: Vec<u8>,
        original_size: [f32; 2],
        #[serde(skip)]
        texture: Option<egui::TextureHandle>,
    },
    StickyNote {
        rect: egui::Rect,
        text: String,
        bg_color: egui::Color32,
        text_color: egui::Color32,
        text_size: f32,
    },
}

impl Shape {
    pub fn new_pen(id: usize, points: Vec<egui::Pos2>, color: egui::Color32, stroke_width: f32) -> Self {
        Self {
            id,
            data: ShapeData::Pen {
                points,
                color,
                stroke_width,
            },
        }
    }

    pub fn new_rect(id: usize, rect: egui::Rect, color: egui::Color32, stroke_width: f32, filled: bool) -> Self {
        Self {
            id,
            data: ShapeData::Rectangle {
                rect,
                color,
                stroke_width,
                filled,
            },
        }
    }

    pub fn new_circle(id: usize, center: egui::Pos2, radius: f32, color: egui::Color32, stroke_width: f32, filled: bool) -> Self {
        Self {
            id,
            data: ShapeData::Circle {
                center,
                radius,
                color,
                stroke_width,
                filled,
            },
        }
    }

    pub fn new_text(id: usize, pos: egui::Pos2, text: String, color: egui::Color32, size: f32) -> Self {
        Self {
            id,
            data: ShapeData::Text {
                pos,
                text,
                color,
                size,
                max_width: None,
                cached_size: None,
            },
        }
    }

    pub fn new_image(id: usize, rect: egui::Rect, bytes: Vec<u8>, original_size: [f32; 2], texture: Option<egui::TextureHandle>) -> Self {
        Self {
            id,
            data: ShapeData::Image {
                rect,
                bytes,
                original_size,
                texture,
            },
        }
    }

    pub fn new_sticky_note(id: usize, rect: egui::Rect, text: String, bg_color: egui::Color32, text_color: egui::Color32, text_size: f32) -> Self {
        Self {
            id,
            data: ShapeData::StickyNote {
                rect,
                text,
                bg_color,
                text_color,
                text_size,
            },
        }
    }
}

impl ShapeData {
    #[allow(dead_code)]
    pub fn icon(&self) -> &str {
        match self {
            ShapeData::Pen { .. } => "✏ Pen",
            ShapeData::Rectangle { .. } => "▭ Rect",
            ShapeData::Circle { .. } => "○ Circle",
            ShapeData::Text { .. } => "🖹 Text",
            ShapeData::Image { .. } => "🖼 Image",
            ShapeData::StickyNote { .. } => "📝 Note",
        }
    }

    pub fn get_bounds(&self) -> egui::Rect {
        match self {
            ShapeData::Pen { points, .. } => {
                if points.is_empty() {
                    egui::Rect::NOTHING
                } else {
                    let mut rect = egui::Rect::from_two_pos(points[0], points[0]);
                    for &p in points.iter().skip(1) {
                        rect.extend_with(p);
                    }
                    rect
                }
            }
            ShapeData::Rectangle { rect, .. } => *rect,
            ShapeData::Circle { center, radius, .. } => {
                egui::Rect::from_center_size(*center, egui::Vec2::splat(radius * 2.0))
            }
            ShapeData::Text { pos, max_width, cached_size, .. } => {
                let size = cached_size.unwrap_or(egui::vec2(max_width.unwrap_or(100.0), 24.0));
                egui::Rect::from_min_size(*pos, size)
            }
            ShapeData::Image { rect, .. } => *rect,
            ShapeData::StickyNote { rect, .. } => *rect,
        }
    }

    pub fn translate(&mut self, delta: egui::Vec2) {
        match self {
            ShapeData::Pen { points, .. } => {
                for p in points {
                    *p += delta;
                }
            }
            ShapeData::Rectangle { rect, .. } => {
                *rect = rect.translate(delta);
            }
            ShapeData::Circle { center, .. } => {
                *center += delta;
            }
            ShapeData::Text { pos, .. } => {
                *pos += delta;
            }
            ShapeData::Image { rect, .. } => {
                *rect = rect.translate(delta);
            }
            ShapeData::StickyNote { rect, .. } => {
                *rect = rect.translate(delta);
            }
        }
    }

    pub fn resize(&mut self, handle_index: usize, delta: egui::Vec2, mouse_pos: egui::Pos2) {
        let bounds = self.get_bounds();
        match self {
            ShapeData::Pen { points, .. } => {
                let w = bounds.width();
                let h = bounds.height();
                if w > 0.0 && h > 0.0 {
                    match handle_index {
                        3 => { // Bottom-Right
                            let new_w = (mouse_pos.x - bounds.min.x).max(10.0);
                            let new_h = (mouse_pos.y - bounds.min.y).max(10.0);
                            let scale_x = new_w / w;
                            let scale_y = new_h / h;
                            for p in points {
                                p.x = bounds.min.x + (p.x - bounds.min.x) * scale_x;
                                p.y = bounds.min.y + (p.y - bounds.min.y) * scale_y;
                            }
                        }
                        0 => { // Top-Left
                            let new_w = (bounds.max.x - mouse_pos.x).max(10.0);
                            let new_h = (bounds.max.y - mouse_pos.y).max(10.0);
                            let scale_x = new_w / w;
                            let scale_y = new_h / h;
                            for p in points {
                                p.x = bounds.max.x - (bounds.max.x - p.x) * scale_x;
                                p.y = bounds.max.y - (bounds.max.y - p.y) * scale_y;
                            }
                        }
                        1 => { // Top-Right
                            let new_w = (mouse_pos.x - bounds.min.x).max(10.0);
                            let new_h = (bounds.max.y - mouse_pos.y).max(10.0);
                            let scale_x = new_w / w;
                            let scale_y = new_h / h;
                            for p in points {
                                p.x = bounds.min.x + (p.x - bounds.min.x) * scale_x;
                                p.y = bounds.max.y - (bounds.max.y - p.y) * scale_y;
                            }
                        }
                        2 => { // Bottom-Left
                            let new_w = (bounds.max.x - mouse_pos.x).max(10.0);
                            let new_h = (mouse_pos.y - bounds.min.y).max(10.0);
                            let scale_x = new_w / w;
                            let scale_y = new_h / h;
                            for p in points {
                                p.x = bounds.max.x - (bounds.max.x - p.x) * scale_x;
                                p.y = bounds.min.y + (p.y - bounds.min.y) * scale_y;
                            }
                        }
                        _ => {}
                    }
                }
            }
            ShapeData::Rectangle { rect, .. } => {
                match handle_index {
                    3 => { // Bottom-Right
                        let new_w = (mouse_pos.x - rect.min.x).max(10.0);
                        let new_h = (mouse_pos.y - rect.min.y).max(10.0);
                        rect.max = rect.min + egui::vec2(new_w, new_h);
                    }
                    0 => { // Top-Left
                        let new_w = (rect.max.x - mouse_pos.x).max(10.0);
                        let new_h = (rect.max.y - mouse_pos.y).max(10.0);
                        rect.min = rect.max - egui::vec2(new_w, new_h);
                    }
                    1 => { // Top-Right
                        let new_w = (mouse_pos.x - rect.min.x).max(10.0);
                        let new_h = (rect.max.y - mouse_pos.y).max(10.0);
                        rect.max.x = rect.min.x + new_w;
                        rect.min.y = rect.max.y - new_h;
                    }
                    2 => { // Bottom-Left
                        let new_w = (rect.max.x - mouse_pos.x).max(10.0);
                        let new_h = (mouse_pos.y - rect.min.y).max(10.0);
                        rect.min.x = rect.max.x - new_w;
                        rect.max.y = rect.min.y + new_h;
                    }
                    _ => {}
                }
            }
            ShapeData::Circle { center, radius, .. } => {
                // Circle resize: radius changes relative to center based on handle distance
                let dist = center.distance(mouse_pos);
                *radius = dist.max(5.0);
            }
            ShapeData::Text { pos, size, max_width, .. } => {
                match handle_index {
                    1 | 3 => {
                        // Right-side handles: set max_width
                        let new_w = (mouse_pos.x - pos.x).max(30.0);
                        *max_width = Some(new_w);
                    }
                    _ => {
                        // Other handles: change font size
                        let change = (delta.x + delta.y) * 0.5;
                        *size = (*size + change).clamp(8.0, 200.0);
                    }
                }
            }
            ShapeData::Image { rect, original_size, .. } => {
                let aspect = original_size[0] / original_size[1];
                match handle_index {
                    3 => { // Bottom-Right
                        let new_w = (mouse_pos.x - rect.min.x).max(10.0);
                        let new_h = new_w / aspect;
                        rect.max = rect.min + egui::vec2(new_w, new_h);
                    }
                    0 => { // Top-Left
                        let new_w = (rect.max.x - mouse_pos.x).max(10.0);
                        let new_h = new_w / aspect;
                        rect.min = rect.max - egui::vec2(new_w, new_h);
                    }
                    1 => { // Top-Right
                        let new_w = (mouse_pos.x - rect.min.x).max(10.0);
                        let new_h = new_w / aspect;
                        rect.max.x = rect.min.x + new_w;
                        rect.min.y = rect.max.y - new_h;
                    }
                    2 => { // Bottom-Left
                        let new_w = (rect.max.x - mouse_pos.x).max(10.0);
                        let new_h = new_w / aspect;
                        rect.min.x = rect.max.x - new_w;
                        rect.max.y = rect.min.y + new_h;
                    }
                    _ => {}
                }
            }
            ShapeData::StickyNote { rect, .. } => {
                match handle_index {
                    1 | 3 => { // Right-side handles: adjust width only
                        let new_w = (mouse_pos.x - rect.min.x).max(50.0);
                        rect.max.x = rect.min.x + new_w;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn contains_point(&self, point: egui::Pos2, tolerance: f32) -> bool {
        match self {
            ShapeData::Pen { points, stroke_width, .. } => {
                if points.len() < 2 {
                    return false;
                }
                let check_dist = (tolerance + stroke_width / 2.0).max(8.0);
                for i in 0..points.len() - 1 {
                    if dist_to_segment(point, points[i], points[i+1]) <= check_dist {
                        return true;
                    }
                }
                false
            }
            ShapeData::Rectangle { rect, .. } => {
                rect.expand(tolerance).contains(point)
            }
            ShapeData::Circle { center, radius, .. } => {
                center.distance(point) <= radius + tolerance
            }
            ShapeData::Text { pos, max_width, cached_size, .. } => {
                let size = cached_size.unwrap_or(egui::vec2(max_width.unwrap_or(100.0), 24.0));
                let bounds = egui::Rect::from_min_size(*pos, size);
                bounds.expand(tolerance).contains(point)
            }
            ShapeData::Image { rect, .. } => {
                rect.expand(tolerance).contains(point)
            }
            ShapeData::StickyNote { rect, .. } => {
                rect.expand(tolerance).contains(point)
            }
        }
    }

    pub fn load_textures(&mut self, ctx: &egui::Context, id: usize) {
        if let ShapeData::Image { bytes, texture, .. } = self {
            if texture.is_none() {
                if let Ok(img) = image::load_from_memory(bytes) {
                    let rgba = img.to_rgba8();
                    let color_img = egui::ColorImage::from_rgba_unmultiplied(
                        [rgba.width() as usize, rgba.height() as usize],
                        &rgba.into_raw(),
                    );
                    *texture = Some(ctx.load_texture(
                        format!("image_texture_{}", id),
                        color_img,
                        egui::TextureOptions {
                            magnification: egui::TextureFilter::Linear,
                            minification: egui::TextureFilter::Linear,
                            mipmap_mode: Some(egui::TextureFilter::Linear),
                            wrap_mode: egui::TextureWrapMode::ClampToEdge,
                        },
                    ));
                }
            }
        }
    }

    pub fn render(&self, painter: &egui::Painter, zoom: f32, pan_offset: egui::Vec2) {
        let transform = |pos: egui::Pos2| -> egui::Pos2 {
            egui::pos2(pos.x * zoom + pan_offset.x, pos.y * zoom + pan_offset.y)
        };

        match self {
            ShapeData::Pen { points, color, stroke_width } => {
                if points.len() > 1 {
                    let transformed_points: Vec<egui::Pos2> =
                        points.iter().map(|&p| transform(p)).collect();
                    let stroke = egui::Stroke::new(stroke_width * zoom, *color);
                    painter.add(egui::Shape::line(transformed_points, stroke));
                }
            }
            ShapeData::Rectangle { rect, color, stroke_width, filled } => {
                let start = transform(rect.min);
                let end = transform(rect.max);
                let transformed_rect = egui::Rect::from_two_pos(start, end);
                let fill = if *filled { *color } else { egui::Color32::TRANSPARENT };
                let stroke = egui::Stroke::new(stroke_width * zoom, *color);
                painter.rect(transformed_rect, 0.0, fill, stroke, egui::StrokeKind::Outside);
            }
            ShapeData::Circle { center, radius, color, stroke_width, filled } => {
                let center_transformed = transform(*center);
                let radius_transformed = radius * zoom;
                let fill = if *filled { *color } else { egui::Color32::TRANSPARENT };
                let stroke = egui::Stroke::new(stroke_width * zoom, *color);
                painter.circle(center_transformed, radius_transformed, fill, stroke);
            }
            ShapeData::Text { pos, text, color, size, max_width, .. } => {
                let screen_pos = transform(*pos);
                let font_id = egui::FontId::proportional(*size * zoom);
                if let Some(mw) = max_width {
                    let wrap_width = mw * zoom;
                    let galley = painter.layout(text.clone(), font_id, *color, wrap_width);
                    painter.galley(screen_pos, galley, *color);
                } else {
                    painter.text(screen_pos, egui::Align2::LEFT_TOP, text, font_id, *color);
                }
            }
            ShapeData::Image { rect, texture, .. } => {
                if let Some(tex) = texture {
                    let start = transform(rect.min);
                    let end = transform(rect.max);
                    let transformed_rect = egui::Rect::from_two_pos(start, end);
                    painter.image(
                        tex.id(),
                        transformed_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            }
            ShapeData::StickyNote { rect, text, bg_color, text_color, text_size } => {
                let start = transform(rect.min);
                let end = transform(rect.max);
                let transformed_rect = egui::Rect::from_two_pos(start, end);
                // Draw filled rounded rect
                painter.rect_filled(transformed_rect, 6.0 * zoom, *bg_color);
                // Draw text inside with padding
                let padding = 8.0 * zoom;
                let text_rect = transformed_rect.shrink(padding);
                if text_rect.width() > 0.0 && text_rect.height() > 0.0 {
                    let font_id = egui::FontId::proportional(*text_size * zoom);
                    let galley = painter.layout(text.clone(), font_id, *text_color, text_rect.width());
                    painter.galley(text_rect.min.into(), galley, *text_color);
                }
            }
        }
    }
}

fn dist_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ap = p - a;
    let ab = b - a;
    let ab_len_sq = ab.length_sq();
    if ab_len_sq < 1e-6 {
        return p.distance(a);
    }
    let t = (ap.dot(ab) / ab_len_sq).clamp(0.0, 1.0);
    let projection = a + ab * t;
    p.distance(projection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_contains_point() {
        let rect = egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(50.0, 50.0));
        let shape = Shape::new_rect(1, rect, egui::Color32::RED, 2.0, false);
        
        // Inside
        assert!(shape.data.contains_point(egui::pos2(30.0, 30.0), 2.0));
        // On edge
        assert!(shape.data.contains_point(egui::pos2(10.0, 30.0), 2.0));
        // Outside
        assert!(!shape.data.contains_point(egui::pos2(5.0, 30.0), 2.0));
    }

    #[test]
    fn test_circle_contains_point() {
        let shape = Shape::new_circle(1, egui::pos2(100.0, 100.0), 20.0, egui::Color32::BLUE, 2.0, false);
        
        // Center
        assert!(shape.data.contains_point(egui::pos2(100.0, 100.0), 2.0));
        // On boundary
        assert!(shape.data.contains_point(egui::pos2(120.0, 100.0), 2.0));
        // Outside
        assert!(!shape.data.contains_point(egui::pos2(130.0, 100.0), 2.0));
    }

    #[test]
    fn test_nudge_translation() {
        let mut shape = Shape::new_rect(1, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(10.0, 10.0)), egui::Color32::WHITE, 1.0, false);
        shape.data.translate(egui::vec2(5.0, -2.0));
        let bounds = shape.data.get_bounds();
        assert_eq!(bounds.min, egui::pos2(5.0, -2.0));
        assert_eq!(bounds.max, egui::pos2(15.0, 8.0));
    }

    #[test]
    fn test_image_resize_aspect_ratio() {
        let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 50.0)); // Aspect ratio = 2.0
        let mut shape = Shape::new_image(1, rect, vec![], [100.0, 50.0], None);
        
        // Resize dragging bottom-right handle (handle_index = 3)
        // Move mouse to x=200. New width = 200. Aspect ratio 2.0 means new height = 100.
        shape.data.resize(3, egui::vec2(100.0, 100.0), egui::pos2(200.0, 200.0));
        
        let bounds = shape.data.get_bounds();
        assert_eq!(bounds.width(), 200.0);
        assert_eq!(bounds.height(), 100.0);
    }
}

