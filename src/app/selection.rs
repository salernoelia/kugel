use crate::app::App;
use crate::shapes::ShapeData;
use eframe::egui;

impl App {
    /// Clear selection and select a single shape.
    pub fn select_single(&mut self, idx: usize) {
        self.selected_shape_indices.clear();
        self.selected_shape_indices.insert(idx);
        self.primary_selected = Some(idx);
    }

    /// Select all shapes.
    pub fn select_all(&mut self) {
        self.selected_shape_indices.clear();
        for idx in 0..self.canvas.shapes.len() {
            self.selected_shape_indices.insert(idx);
        }
        if !self.canvas.shapes.is_empty() {
            self.primary_selected = Some(self.canvas.shapes.len() - 1);
        } else {
            self.primary_selected = None;
        }
    }

    /// Duplicate all selected shapes in place and select the copies.
    pub fn duplicate_selection(&mut self, ctx: &egui::Context) {
        if self.selected_shape_indices.is_empty() {
            return;
        }
        self.canvas.history.push(self.canvas.shapes.clone());
        self.canvas.undo_history.clear();
        self.is_dirty = true;

        let mut indices: Vec<usize> = self.selected_shape_indices.iter().copied().collect();
        indices.sort_unstable();
        self.clear_selection();

        for idx in indices {
            if idx < self.canvas.shapes.len() {
                let mut dup = self.canvas.shapes[idx].clone();
                dup.id = self.canvas.next_id;
                self.canvas.next_id += 1;
                dup.data.load_textures(ctx, dup.id);
                self.canvas.shapes.push(dup);
                let new_idx = self.canvas.shapes.len() - 1;
                self.selected_shape_indices.insert(new_idx);
                self.primary_selected = Some(new_idx);
            }
        }
    }

    /// Clear all selection.
    pub fn clear_selection(&mut self) {
        self.selected_shape_indices.clear();
        self.primary_selected = None;
    }

    /// Check if any shape is selected.
    pub fn has_selection(&self) -> bool {
        !self.selected_shape_indices.is_empty()
    }

    /// Alignment snapping: compare the moving bounds' edges/centers against every
    /// non-selected shape and return the nearest correction (canvas units) plus the
    /// guide segments to draw. `threshold` is in canvas units.
    pub fn compute_alignment_snap(
        &self,
        moving: egui::Rect,
        threshold: f32,
    ) -> (egui::Vec2, Vec<(egui::Pos2, egui::Pos2)>) {
        let m_xs = [moving.min.x, moving.center().x, moving.max.x];
        let m_ys = [moving.min.y, moving.center().y, moving.max.y];

        let mut best_dx = threshold;
        let mut best_dy = threshold;
        let mut corr_x = 0.0;
        let mut corr_y = 0.0;
        let mut snap_x: Option<(f32, egui::Rect)> = None;
        let mut snap_y: Option<(f32, egui::Rect)> = None;

        for (i, shape) in self.canvas.shapes.iter().enumerate() {
            if self.selected_shape_indices.contains(&i) {
                continue;
            }
            let t = shape.data.get_bounds();
            if !t.is_positive() {
                continue;
            }
            for &tx in &[t.min.x, t.center().x, t.max.x] {
                for &mx in &m_xs {
                    let d = (tx - mx).abs();
                    if d < best_dx {
                        best_dx = d;
                        corr_x = tx - mx;
                        snap_x = Some((tx, t));
                    }
                }
            }
            for &ty in &[t.min.y, t.center().y, t.max.y] {
                for &my in &m_ys {
                    let d = (ty - my).abs();
                    if d < best_dy {
                        best_dy = d;
                        corr_y = ty - my;
                        snap_y = Some((ty, t));
                    }
                }
            }
        }

        let correction = egui::vec2(corr_x, corr_y);
        let corrected = moving.translate(correction);
        let mut guides = Vec::new();
        if let Some((tx, t)) = snap_x {
            let y0 = corrected.min.y.min(t.min.y);
            let y1 = corrected.max.y.max(t.max.y);
            guides.push((egui::pos2(tx, y0), egui::pos2(tx, y1)));
        }
        if let Some((ty, t)) = snap_y {
            let x0 = corrected.min.x.min(t.min.x);
            let x1 = corrected.max.x.max(t.max.x);
            guides.push((egui::pos2(x0, ty), egui::pos2(x1, ty)));
        }
        (correction, guides)
    }

    pub fn screen_to_canvas(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (screen_pos.x - self.pan_offset.x) / self.zoom,
            (screen_pos.y - self.pan_offset.y) / self.zoom,
        )
    }

    /// Canvas-space point where pasted content should land: current mouse
    /// position, falling back to the viewport center when no pointer is known.
    pub fn paste_target_canvas(&self, ctx: &egui::Context) -> egui::Pos2 {
        let screen = ctx
            .input(|i| i.pointer.latest_pos())
            .unwrap_or_else(|| ctx.screen_rect().center());
        self.screen_to_canvas(screen)
    }

    pub fn canvas_to_screen(&self, canvas_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            canvas_pos.x * self.zoom + self.pan_offset.x,
            canvas_pos.y * self.zoom + self.pan_offset.y,
        )
    }

    pub fn hit_test(&self, canvas_pos: egui::Pos2) -> Option<usize> {
        let tolerance = 5.0;
        for (idx, shape) in self.canvas.shapes.iter().enumerate().rev() {
            if shape.data.contains_point(canvas_pos, tolerance) {
                return Some(idx);
            }
        }
        None
    }

    pub fn get_handle_under_mouse(&self, shape_idx: usize, mouse_pos: egui::Pos2) -> Option<usize> {
        let shape = &self.canvas.shapes[shape_idx];
        let bounds = shape.data.get_bounds();
        let screen_bounds = egui::Rect::from_min_max(
            self.canvas_to_screen(bounds.min),
            self.canvas_to_screen(bounds.max),
        );
        let is_text_or_sticky = matches!(
            shape.data,
            ShapeData::Text { .. } | ShapeData::StickyNote { .. }
        );

        if is_text_or_sticky {
            let handles = [
                (1, screen_bounds.right_top()),
                (3, screen_bounds.right_bottom()),
            ];
            for &(h_idx, pos) in &handles {
                if mouse_pos.distance(pos) <= 8.0 {
                    return Some(h_idx);
                }
            }
        } else {
            let handle_positions = [
                screen_bounds.left_top(),     // 0
                screen_bounds.right_top(),    // 1
                screen_bounds.left_bottom(),  // 2
                screen_bounds.right_bottom(), // 3
            ];
            for (h_idx, &pos) in handle_positions.iter().enumerate() {
                if mouse_pos.distance(pos) <= 8.0 {
                    return Some(h_idx);
                }
            }
        }
        None
    }

    /// Union bounds (canvas) of all selected shapes.
    pub fn selection_bounds(&self) -> Option<egui::Rect> {
        let mut acc: Option<egui::Rect> = None;
        for &idx in &self.selected_shape_indices {
            if idx < self.canvas.shapes.len() {
                let b = self.canvas.shapes[idx].data.get_bounds();
                if b.is_positive() {
                    acc = Some(acc.map_or(b, |a| a.union(b)));
                }
            }
        }
        acc
    }

    /// Corner handle of the group selection box under the mouse (screen pos).
    pub fn group_handle_under_mouse(&self, mouse_pos: egui::Pos2) -> Option<usize> {
        let bounds = self.selection_bounds()?;
        let screen_bounds = egui::Rect::from_min_max(
            self.canvas_to_screen(bounds.min),
            self.canvas_to_screen(bounds.max),
        );
        let handle_positions = [
            screen_bounds.left_top(),
            screen_bounds.right_top(),
            screen_bounds.left_bottom(),
            screen_bounds.right_bottom(),
        ];
        for (h_idx, &pos) in handle_positions.iter().enumerate() {
            if mouse_pos.distance(pos) <= 8.0 {
                return Some(h_idx);
            }
        }
        None
    }

    /// Declutter tool: Arrange selected shapes into a clean horizontal row
    /// with the same top position (minimum min.y among selection) and a small regular gap.
    pub fn declutter_selection(&mut self) {
        if self.selected_shape_indices.len() < 2 {
            return;
        }

        self.canvas.push_history();

        let mut sorted_indices: Vec<usize> = self
            .selected_shape_indices
            .iter()
            .copied()
            .filter(|&idx| idx < self.canvas.shapes.len())
            .collect();

        if sorted_indices.len() < 2 {
            return;
        }

        // Sort shapes left-to-right by their bounding box min.x
        sorted_indices.sort_by(|&a, &b| {
            let rect_a = self.canvas.shapes[a].data.get_bounds();
            let rect_b = self.canvas.shapes[b].data.get_bounds();
            rect_a.min.x.partial_cmp(&rect_b.min.x).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Find the top-most position (minimum min.y) among the selected shapes
        let top_y = sorted_indices
            .iter()
            .map(|&idx| self.canvas.shapes[idx].data.get_bounds().min.y)
            .fold(f32::INFINITY, f32::min);

        let gap = 16.0;
        let mut next_x = self.canvas.shapes[sorted_indices[0]].data.get_bounds().min.x;

        for &idx in &sorted_indices {
            let bounds = self.canvas.shapes[idx].data.get_bounds();
            let delta_x = next_x - bounds.min.x;
            let delta_y = top_y - bounds.min.y;

            self.canvas.shapes[idx].data.translate(egui::vec2(delta_x, delta_y));

            let new_bounds = self.canvas.shapes[idx].data.get_bounds();
            next_x = new_bounds.max.x + gap;
        }

        self.is_dirty = true;
        self.notification = Some((
            format!("Decluttered {} shapes into row", sorted_indices.len()),
            std::time::Instant::now(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shift_toggle_selection() {
        let mut app = App::default();
        app.canvas.add_text(egui::pos2(0.0, 0.0), "Item 1".into(), egui::Color32::WHITE);
        app.canvas.add_text(egui::pos2(100.0, 100.0), "Item 2".into(), egui::Color32::WHITE);

        app.select_single(0);
        assert!(app.selected_shape_indices.contains(&0));
        assert!(!app.selected_shape_indices.contains(&1));

        app.selected_shape_indices.insert(1);
        assert!(app.selected_shape_indices.contains(&0));
        assert!(app.selected_shape_indices.contains(&1));

        app.selected_shape_indices.remove(&0);
        assert!(!app.selected_shape_indices.contains(&0));
        assert!(app.selected_shape_indices.contains(&1));
    }

    #[test]
    fn test_declutter_selection() {
        let mut app = App::default();
        // Create 3 shapes scattered in space
        let rect1 = egui::Rect::from_min_max(egui::pos2(200.0, 300.0), egui::pos2(300.0, 400.0)); // w=100
        let rect2 = egui::Rect::from_min_max(egui::pos2(50.0, 500.0), egui::pos2(150.0, 600.0));  // w=100
        let rect3 = egui::Rect::from_min_max(egui::pos2(400.0, 100.0), egui::pos2(500.0, 200.0)); // w=100

        let idx0 = app.canvas.shapes.len();
        app.canvas.shapes.push(crate::shapes::Shape::new_rect(1, rect1, egui::Color32::RED, 1.0, false));
        let idx1 = app.canvas.shapes.len();
        app.canvas.shapes.push(crate::shapes::Shape::new_rect(2, rect2, egui::Color32::BLUE, 1.0, false));
        let idx2 = app.canvas.shapes.len();
        app.canvas.shapes.push(crate::shapes::Shape::new_rect(3, rect3, egui::Color32::GREEN, 1.0, false));

        app.selected_shape_indices.insert(idx0);
        app.selected_shape_indices.insert(idx1);
        app.selected_shape_indices.insert(idx2);

        app.declutter_selection();

        // Top y should be min(300, 500, 100) = 100
        let b1 = app.canvas.shapes[idx1].data.get_bounds(); // originally min.x = 50 (leftmost)
        let b0 = app.canvas.shapes[idx0].data.get_bounds(); // originally min.x = 200
        let b2 = app.canvas.shapes[idx2].data.get_bounds(); // originally min.x = 400

        assert_eq!(b1.min.y, 100.0);
        assert_eq!(b0.min.y, 100.0);
        assert_eq!(b2.min.y, 100.0);

        assert_eq!(b1.min.x, 50.0);
        assert_eq!(b0.min.x, 50.0 + 100.0 + 16.0); // 166.0
        assert_eq!(b2.min.x, 166.0 + 100.0 + 16.0); // 282.0

        // Test undo restores original scattered positions
        app.canvas.undo();
        assert_eq!(app.canvas.shapes[idx0].data.get_bounds().min, egui::pos2(200.0, 300.0));
        assert_eq!(app.canvas.shapes[idx1].data.get_bounds().min, egui::pos2(50.0, 500.0));
        assert_eq!(app.canvas.shapes[idx2].data.get_bounds().min, egui::pos2(400.0, 100.0));
    }
}
