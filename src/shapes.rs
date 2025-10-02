use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool {
    Pen,
    Rectangle,
    Circle,
    Select,
    Bezier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BezierCurve {
    pub p0: egui::Pos2,
    pub p1: egui::Pos2,
    pub p2: egui::Pos2,
    pub p3: egui::Pos2,
}

impl BezierCurve {
    pub fn sample(&self, t: f32) -> egui::Pos2 {
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        let x = mt3 * self.p0.x
            + 3.0 * mt2 * t * self.p1.x
            + 3.0 * mt * t2 * self.p2.x
            + t3 * self.p3.x;
        let y = mt3 * self.p0.y
            + 3.0 * mt2 * t * self.p1.y
            + 3.0 * mt * t2 * self.p2.y
            + t3 * self.p3.y;

        egui::pos2(x, y)
    }
}

pub struct Shape {
    tool: Tool,
    start: egui::Pos2,
    end: egui::Pos2,
    points: Vec<egui::Pos2>,
    bezier: Option<BezierCurve>, // <- Fix line 15
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
            bezier: None,
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

    pub fn set_bezier(&mut self, bezier: BezierCurve) {
        self.bezier = Some(bezier);
    }

    pub fn icon(&self) -> &str {
        match self.tool {
            Tool::Pen => "✏",
            Tool::Rectangle => "▭",
            Tool::Circle => "○",
            Tool::Select => "➤",
            Tool::Bezier => "C",
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
            Tool::Bezier => {
                if let Some(bezier) = &self.bezier {
                    let segments = 50;
                    let mut bezier_points = Vec::with_capacity(segments + 1);

                    for i in 0..=segments {
                        let t = i as f32 / segments as f32;
                        let point = bezier.sample(t);
                        bezier_points.push(transform(point));
                    }

                    if bezier_points.len() > 1 {
                        painter.add(egui::Shape::line(bezier_points, stroke));
                    }
                }
            }
        }
    }
}
