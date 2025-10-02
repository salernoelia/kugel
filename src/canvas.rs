use crate::shapes::{BezierCurve, Shape, Tool};
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq)]
enum BezierState {
    WaitingForStart,
    WaitingForEnd,
    WaitingForControl1,
    WaitingForControl2,
    Editing { selected_handle: usize },
}

pub struct Canvas {
    pub shapes: Vec<Shape>,
    current_shape: Option<Shape>,
    bezier_state: BezierState,
    bezier_points: Vec<egui::Pos2>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            shapes: Vec::new(),
            current_shape: None,
            bezier_state: BezierState::WaitingForStart,
            bezier_points: Vec::new(),
        }
    }
}

impl Canvas {
    pub fn start_shape(&mut self, tool: Tool, pos: egui::Pos2, color: egui::Color32, width: f32) {
        if tool == Tool::Bezier {
            match self.bezier_state {
                BezierState::WaitingForStart => {
                    self.bezier_points = vec![pos];
                    self.bezier_state = BezierState::WaitingForEnd;
                }
                BezierState::WaitingForEnd => {
                    self.bezier_points.push(pos);
                    let delta = pos - self.bezier_points[0];
                    self.bezier_points
                        .push(self.bezier_points[0] + delta * 0.33);
                    self.bezier_state = BezierState::WaitingForControl1;
                }
                BezierState::WaitingForControl1 => {
                    self.bezier_points[2] = pos;
                    self.bezier_state = BezierState::WaitingForControl2;
                }
                BezierState::WaitingForControl2 => {
                    self.bezier_points.push(pos);
                    let bezier = BezierCurve {
                        p0: self.bezier_points[0],
                        p1: self.bezier_points[2],
                        p2: self.bezier_points[3],
                        p3: self.bezier_points[1],
                    };
                    let mut shape = Shape::new(Tool::Bezier, self.bezier_points[0], color, width);
                    shape.set_bezier(bezier);
                    self.shapes.push(shape);
                    self.bezier_state = BezierState::WaitingForStart;
                    self.bezier_points.clear();
                }
                BezierState::Editing { .. } => {}
            }
        } else {
            self.current_shape = Some(Shape::new(tool, pos, color, width));
        }
    }

    pub fn update_current_shape(&mut self, pos: egui::Pos2) {
        if let Some(shape) = &mut self.current_shape {
            shape.update(pos);
        }
    }

    pub fn update_bezier_hover(&mut self, pos: egui::Pos2) {
        if let BezierState::WaitingForControl1 = self.bezier_state {
            if self.bezier_points.len() >= 3 {
                self.bezier_points[2] = pos;
            }
        } else if let BezierState::WaitingForControl2 = self.bezier_state {
            if self.bezier_points.len() >= 3 {
                self.bezier_points.push(pos);
                self.bezier_points.truncate(4);
            }
        }
    }

    pub fn finish_shape(&mut self) {
        if let Some(shape) = self.current_shape.take() {
            self.shapes.push(shape);
        }
    }

    pub fn clear(&mut self) {
        self.shapes.clear();
        self.current_shape = None;
        self.bezier_state = BezierState::WaitingForStart;
        self.bezier_points.clear();
    }

    pub fn render(&self, painter: &egui::Painter, zoom: f32, pan_offset: egui::Vec2) {
        for shape in &self.shapes {
            shape.render(painter, zoom, pan_offset);
        }

        if let Some(shape) = &self.current_shape {
            shape.render(painter, zoom, pan_offset);
        }

        // Render bezier construction helpers
        if !self.bezier_points.is_empty() {
            let transform = |pos: egui::Pos2| -> egui::Pos2 {
                egui::pos2(pos.x * zoom + pan_offset.x, pos.y * zoom + pan_offset.y)
            };

            let handle_radius = 5.0;
            let line_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(150));

            match self.bezier_state {
                BezierState::WaitingForEnd if self.bezier_points.len() >= 1 => {
                    let p0 = transform(self.bezier_points[0]);
                    painter.circle_filled(p0, handle_radius, egui::Color32::RED);
                }
                BezierState::WaitingForControl1 if self.bezier_points.len() >= 3 => {
                    let p0 = transform(self.bezier_points[0]);
                    let p3 = transform(self.bezier_points[1]);
                    let p1 = transform(self.bezier_points[2]);

                    painter.circle_filled(p0, handle_radius, egui::Color32::RED);
                    painter.circle_filled(p3, handle_radius, egui::Color32::RED);
                    painter.circle_filled(p1, handle_radius, egui::Color32::BLUE);
                    painter.line_segment([p0, p1], line_stroke);
                }
                BezierState::WaitingForControl2 if self.bezier_points.len() >= 4 => {
                    let p0 = transform(self.bezier_points[0]);
                    let p3 = transform(self.bezier_points[1]);
                    let p1 = transform(self.bezier_points[2]);
                    let p2 = transform(self.bezier_points[3]);

                    // Draw the curve preview
                    let bezier = BezierCurve {
                        p0: self.bezier_points[0],
                        p1: self.bezier_points[2],
                        p2: self.bezier_points[3],
                        p3: self.bezier_points[1],
                    };

                    let segments = 50;
                    let mut bezier_points = Vec::with_capacity(segments + 1);
                    for i in 0..=segments {
                        let t = i as f32 / segments as f32;
                        let point = bezier.sample(t);
                        bezier_points.push(transform(point));
                    }
                    painter.add(egui::Shape::line(
                        bezier_points,
                        egui::Stroke::new(2.0, egui::Color32::DARK_GRAY),
                    ));

                    painter.circle_filled(p0, handle_radius, egui::Color32::RED);
                    painter.circle_filled(p3, handle_radius, egui::Color32::RED);
                    painter.circle_filled(p1, handle_radius, egui::Color32::BLUE);
                    painter.circle_filled(p2, handle_radius, egui::Color32::BLUE);
                    painter.line_segment([p0, p1], line_stroke);
                    painter.line_segment([p3, p2], line_stroke);
                }
                _ => {}
            }
        }
    }

    pub fn is_bezier_active(&self) -> bool {
        self.bezier_state != BezierState::WaitingForStart
    }
}
