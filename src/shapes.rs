#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool {
    Pen,
    Rectangle,
    Circle,
    Select,
}

#[derive(Clone)]
pub struct Shape {
    tool: Tool,
    start: egui::Pos2,
    end: egui::Pos2,
    points: Vec<egui::Pos2>,
    color: egui::Color32,
    stroke_width: f32,
}

impl Shape {
    pub fn new(tool: Tool, pos: egui::Pos2, color: egui::Color32, width: f32) -> Self {
        Self {
            tool,
            start: pos,
            end: pos,
            points: vec![pos],
            color,
            stroke_width: width,
        }
    }

    pub fn update(&mut self, pos: egui::Pos2) {
        self.end = pos;
        if self.tool == Tool::Pen {
            self.points.push(pos);
        }
    }

    pub fn icon(&self) -> &str {
        match self.tool {
            Tool::Pen => "✏",
            Tool::Rectangle => "▭",
            Tool::Circle => "○",
            Tool::Select => "➤",
        }
    }

    pub fn render(&self, painter: &egui::Painter, zoom: f32, pan_offset: egui::Vec2) {
        let transform = |pos: egui::Pos2| -> egui::Pos2 {
            egui::pos2(pos.x * zoom + pan_offset.x, pos.y * zoom + pan_offset.y)
        };

        let stroke = egui::Stroke::new(self.stroke_width * zoom, self.color);

        match self.tool {
            Tool::Pen => {
                if self.points.len() > 1 {
                    let transformed_points: Vec<egui::Pos2> =
                        self.points.iter().map(|&p| transform(p)).collect();
                    painter.add(egui::Shape::line(transformed_points, stroke));
                }
            }
            Tool::Rectangle => {
                let start = transform(self.start);
                let end = transform(self.end);
                let rect = egui::Rect::from_two_pos(start, end);
                painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Outside);
            }
            Tool::Circle => {
                let center = transform(self.start);
                let radius = self.start.distance(self.end) * zoom;
                painter.circle_stroke(center, radius, stroke);
            }
            Tool::Select => {}
        }
    }
}
