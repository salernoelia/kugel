use crate::shapes::{Shape, ShapeData, Tool};
use eframe::egui;

#[derive(Default)]
pub struct Canvas {
    pub shapes: Vec<Shape>,
    pub current_shape: Option<Shape>,
    pub history: Vec<Vec<Shape>>,
    pub undo_history: Vec<Vec<Shape>>,
    pub next_id: usize,
    pub creation_start_pos: Option<egui::Pos2>,
}

impl Canvas {
    pub fn start_shape(
        &mut self,
        tool: Tool,
        pos: egui::Pos2,
        color: egui::Color32,
        width: f32,
        filled: bool,
    ) -> Option<usize> {
        self.undo_history.clear();
        self.creation_start_pos = Some(pos);
        match tool {
            Tool::Pen => {
                self.current_shape = Some(Shape::new_pen(self.next_id, vec![pos], color, width));
                None
            }
            Tool::Rectangle => {
                self.current_shape = Some(Shape::new_rect(
                    self.next_id,
                    egui::Rect::from_two_pos(pos, pos),
                    color,
                    width,
                    filled,
                ));
                None
            }
            Tool::Circle => {
                self.current_shape = Some(Shape::new_circle(
                    self.next_id,
                    pos,
                    0.0,
                    color,
                    width,
                    filled,
                ));
                None
            }
            Tool::Text => {
                // Text is created instantly and placed in edit mode
                let text_shape = Shape::new_text(
                    self.next_id,
                    pos,
                    "".to_string(),
                    color,
                    24.0,
                );
                self.history.push(self.shapes.clone());
                self.shapes.push(text_shape);
                let edit_index = self.shapes.len() - 1;
                self.next_id += 1;
                Some(edit_index)
            }
            Tool::StickyNote => {
                // StickyNote is created instantly and placed in edit mode
                let rect = egui::Rect::from_min_size(pos, egui::vec2(140.0, 140.0));
                let sticky_shape = Shape::new_sticky_note(
                    self.next_id,
                    rect,
                    "".to_string(),
                    egui::Color32::from_rgb(255, 243, 176), // Light yellow
                    egui::Color32::from_rgb(60, 50, 20),    // Dark brown text
                    16.0,
                );
                self.history.push(self.shapes.clone());
                self.shapes.push(sticky_shape);
                let edit_index = self.shapes.len() - 1;
                self.next_id += 1;
                Some(edit_index)
            }
            Tool::Select => None,
        }
    }

    pub fn update_current_shape(&mut self, pos: egui::Pos2) {
        if let Some(shape) = &mut self.current_shape {
            match &mut shape.data {
                ShapeData::Pen { points, .. } => {
                    if let Some(&last) = points.last() {
                        // Prevent storing too many redundant points
                        if last.distance(pos) > 1.0 {
                            points.push(pos);
                        }
                    } else {
                        points.push(pos);
                    }
                }
                ShapeData::Rectangle { rect, .. } => {
                    if let Some(start) = self.creation_start_pos {
                        *rect = egui::Rect::from_two_pos(start, pos);
                    }
                }
                ShapeData::Circle { center, radius, .. } => {
                    *radius = center.distance(pos);
                }
                _ => {}
            }
        }
    }

    pub fn finish_shape(&mut self) {
        self.creation_start_pos = None;
        if let Some(shape) = self.current_shape.take() {
            // Verify shape has substance (e.g. pen has points)
            let keep = match &shape.data {
                ShapeData::Pen { points, .. } => points.len() > 1,
                ShapeData::Rectangle { rect, .. } => rect.width() > 1.0 || rect.height() > 1.0,
                ShapeData::Circle { radius, .. } => *radius > 1.0,
                _ => true,
            };

            if keep {
                self.history.push(self.shapes.clone());
                self.shapes.push(shape);
                self.next_id += 1;
            }
        }
    }

    pub fn add_image(&mut self, pos: egui::Pos2, bytes: Vec<u8>, size: [f32; 2], ctx: &egui::Context) -> usize {
        self.history.push(self.shapes.clone());
        self.undo_history.clear();

        let w = size[0];
        let h = size[1];
        let aspect = w / h;
        let disp_w = w.min(400.0);
        let disp_h = disp_w / aspect;

        let rect = egui::Rect::from_center_size(pos, egui::vec2(disp_w, disp_h));
        let mut shape = Shape::new_image(self.next_id, rect, bytes, size, None);
        shape.data.load_textures(ctx, self.next_id);

        let added_idx = self.shapes.len();
        self.shapes.push(shape);
        self.next_id += 1;
        added_idx
    }

    pub fn add_text(&mut self, pos: egui::Pos2, text: String, color: egui::Color32) -> usize {
        self.history.push(self.shapes.clone());
        self.undo_history.clear();

        let shape = Shape::new_text(self.next_id, pos, text, color, 24.0);
        let added_idx = self.shapes.len();
        self.shapes.push(shape);
        self.next_id += 1;
        added_idx
    }

    pub fn clear(&mut self) {
        if !self.shapes.is_empty() {
            self.history.push(self.shapes.clone());
            self.undo_history.clear();
            self.shapes.clear();
        }
    }

    pub fn render(&self, painter: &egui::Painter, zoom: f32, pan_offset: egui::Vec2, editing_index: Option<usize>) {
        for (idx, shape) in self.shapes.iter().enumerate() {
            let is_editing = Some(idx) == editing_index;
            shape.data.render(painter, zoom, pan_offset, is_editing);
        }

        if let Some(shape) = &self.current_shape {
            shape.data.render(painter, zoom, pan_offset, false);
        }
    }

    pub fn load_textures(&mut self, ctx: &egui::Context) {
        for shape in &mut self.shapes {
            shape.data.load_textures(ctx, shape.id);
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.undo_history.push(self.shapes.clone());
            self.shapes = prev;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.undo_history.pop() {
            self.history.push(self.shapes.clone());
            self.shapes = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::Tool;

    #[test]
    fn test_canvas_undo_redo() {
        let mut canvas = Canvas::default();
        
        // Add text shape
        canvas.add_text(egui::pos2(0.0, 0.0), "Hello".to_string(), egui::Color32::WHITE);
        assert_eq!(canvas.shapes.len(), 1);
        
        // Add rectangle
        canvas.start_shape(Tool::Rectangle, egui::pos2(10.0, 10.0), egui::Color32::RED, 2.0, false);
        canvas.update_current_shape(egui::pos2(20.0, 20.0));
        canvas.finish_shape();
        assert_eq!(canvas.shapes.len(), 2);
        
        // Undo -> should go back to 1 shape
        canvas.undo();
        assert_eq!(canvas.shapes.len(), 1);
        
        // Redo -> should go back to 2 shapes
        canvas.redo();
        assert_eq!(canvas.shapes.len(), 2);
        
        // Undo, then clear -> shapes should be empty
        canvas.undo();
        canvas.clear();
        assert_eq!(canvas.shapes.len(), 0);
        
        // Undo clear -> back to 1 shape
        canvas.undo();
        assert_eq!(canvas.shapes.len(), 1);
    }
}

