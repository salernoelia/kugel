use crate::shapes::Shape;

#[derive(Default)]
pub struct Canvas {
    pub shapes: Vec<Shape>,
    current_shape: Option<Shape>,
    pub history: Vec<Vec<Shape>>,
    pub undo_history: Vec<Vec<Shape>>,
}

impl Canvas {
    pub fn start_shape(
        &mut self,
        tool: crate::shapes::Tool,
        pos: egui::Pos2,
        color: egui::Color32,
        width: f32,
    ) {
        self.current_shape = Some(Shape::new(tool, pos, color, width));
    }

    pub fn update_current_shape(&mut self, pos: egui::Pos2) {
        if let Some(shape) = &mut self.current_shape {
            shape.update(pos);
        }
    }

    pub fn finish_shape(&mut self) {
        if let Some(shape) = self.current_shape.take() {
            self.history.push(self.shapes.clone()); // Save current state
            self.shapes.push(shape);
        }
    }

    pub fn clear(&mut self) {
        self.history.push(self.shapes.clone()); // Save current state
        self.shapes.clear();
        self.current_shape = None;
    }

    pub fn render(&self, painter: &egui::Painter, zoom: f32, pan_offset: egui::Vec2) {
        for shape in &self.shapes {
            shape.render(painter, zoom, pan_offset);
        }

        if let Some(shape) = &self.current_shape {
            shape.render(painter, zoom, pan_offset);
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
