mod canvas;
mod shapes;

use canvas::Canvas;
use eframe::egui;
use shapes::Tool;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Drawmi",
        options,
        Box::new(|_cc| Ok(Box::new(VectorEditorApp::default()))),
    )
}

struct VectorEditorApp {
    canvas: Canvas,
    tool: Tool,
    selected_color: egui::Color32,
    stroke_width: f32,
    zoom: f32,
    pan_offset: egui::Vec2,
    use_grid: bool,
    background_color: egui::Color32,
}

impl Default for VectorEditorApp {
    fn default() -> Self {
        Self {
            canvas: Canvas::default(),
            tool: Tool::Pen,
            selected_color: egui::Color32::BLACK,
            stroke_width: 2.0,
            zoom: 1.0,
            pan_offset: egui::Vec2::ZERO,
            use_grid: false,
            background_color: egui::Color32::from_gray(240),
        }
    }
}

impl VectorEditorApp {
    // Transform screen coordinates to canvas coordinates
    fn screen_to_canvas(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (screen_pos.x - self.pan_offset.x) / self.zoom,
            (screen_pos.y - self.pan_offset.y) / self.zoom,
        )
    }

    // Transform canvas coordinates to screen coordinates
    // fn canvas_to_screen(&self, canvas_pos: egui::Pos2) -> egui::Pos2 {
    //     egui::pos2(
    //         canvas_pos.x * self.zoom + self.pan_offset.x,
    //         canvas_pos.y * self.zoom + self.pan_offset.y,
    //     )
    // }
}

impl eframe::App for VectorEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("tools_panel")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Tools");
                ui.separator();

                if ui
                    .selectable_label(self.tool == Tool::Pen, "✏ Pen")
                    .clicked()
                {
                    self.tool = Tool::Pen;
                }
                if ui
                    .selectable_label(self.tool == Tool::Rectangle, "▭ Rectangle")
                    .clicked()
                {
                    self.tool = Tool::Rectangle;
                }
                if ui
                    .selectable_label(self.tool == Tool::Circle, "○ Circle")
                    .clicked()
                {
                    self.tool = Tool::Circle;
                }
                if ui
                    .selectable_label(self.tool == Tool::Select, "➤ Select")
                    .clicked()
                {
                    self.tool = Tool::Select;
                }
                if ui
                    .selectable_label(self.tool == Tool::Bezier, "➤ Bezier")
                    .clicked()
                {
                    self.tool = Tool::Bezier;
                }

                ui.separator();
                ui.heading("Properties");

                ui.label("Stroke Color:");
                egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut self.selected_color,
                    egui::color_picker::Alpha::Opaque,
                );

                ui.label("Background color Color:");
                egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut self.background_color,
                    egui::color_picker::Alpha::Opaque,
                );

                ui.add(egui::Slider::new(&mut self.stroke_width, 1.0..=20.0).text("Width"));

                ui.separator();
                ui.label(format!("Zoom: {:.0}%", self.zoom * 100.0));
                if ui.button("Reset View").clicked() {
                    self.zoom = 1.0;
                    self.pan_offset = egui::Vec2::ZERO;
                }
                ui.checkbox(&mut self.use_grid, "Show Grid");

                ui.separator();
                if ui.button("Clear Canvas").clicked() {
                    self.canvas.clear();
                }
            });

        egui::SidePanel::right("layers_panel")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Layers");
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (idx, shape) in self.canvas.shapes.iter().enumerate() {
                        let label = format!("{} {}", shape.icon(), idx);
                        if ui.selectable_label(false, label).clicked() {
                            // TODO: Select shape
                        }
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, mut painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

            painter.rect_filled(response.rect, 0.0, self.background_color);

            if self.use_grid == true {
                let grid_spacing = 50.0 * self.zoom;
                let grid_color = egui::Color32::from_gray(220);

                if grid_spacing > 5.0 {
                    let min_x = ((response.rect.min.x - self.pan_offset.x) / grid_spacing).floor()
                        * grid_spacing
                        + self.pan_offset.x;
                    let min_y = ((response.rect.min.y - self.pan_offset.y) / grid_spacing).floor()
                        * grid_spacing
                        + self.pan_offset.y;

                    let mut x = min_x;
                    while x < response.rect.max.x {
                        painter.vline(
                            x,
                            response.rect.y_range(),
                            egui::Stroke::new(1.0, grid_color),
                        );
                        x += grid_spacing;
                    }

                    let mut y = min_y;
                    while y < response.rect.max.y {
                        painter.hline(
                            response.rect.x_range(),
                            y,
                            egui::Stroke::new(1.0, grid_color),
                        );
                        y += grid_spacing;
                    }
                }
            }

            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            let zoom_delta = ui.input(|i| i.zoom_delta());

            if zoom_delta != 1.0 || scroll_delta.y != 0.0 {
                let pointer_pos = response.hover_pos().unwrap_or(response.rect.center());

                let zoom_factor = if zoom_delta != 1.0 {
                    zoom_delta
                } else {
                    if ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
                        1.0 + scroll_delta.y * 0.001
                    } else {
                        1.0
                    }
                };

                if zoom_factor != 1.0 {
                    let old_zoom = self.zoom;
                    self.zoom = (self.zoom * zoom_factor).clamp(0.1, 10.0);

                    let zoom_change = self.zoom / old_zoom;
                    self.pan_offset = pointer_pos.to_vec2()
                        + (self.pan_offset - pointer_pos.to_vec2()) * zoom_change;
                }
            }

            let is_panning = ui.input(|i| {
                i.pointer.middle_down()
                    || (i.key_down(egui::Key::Space) && i.pointer.primary_down())
            });

            if is_panning && response.dragged() {
                self.pan_offset += response.drag_delta();
            } else if !is_panning {
                // Handle Bezier tool specially with clicks
                if self.tool == Tool::Bezier {
                    if let Some(pos) = response.hover_pos() {
                        let canvas_pos = self.screen_to_canvas(pos);
                        self.canvas.update_bezier_hover(canvas_pos);
                    }

                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let canvas_pos = self.screen_to_canvas(pos);
                            self.canvas.start_shape(
                                self.tool,
                                canvas_pos,
                                self.selected_color,
                                self.stroke_width,
                            );
                        }
                    }
                } else {
                    // Handle other tools with drag
                    if response.drag_started() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let canvas_pos = self.screen_to_canvas(pos);
                            self.canvas.start_shape(
                                self.tool,
                                canvas_pos,
                                self.selected_color,
                                self.stroke_width,
                            );
                        }
                    }

                    if response.dragged() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let canvas_pos = self.screen_to_canvas(pos);
                            self.canvas.update_current_shape(canvas_pos);
                        }
                    }

                    if response.drag_stopped() {
                        self.canvas.finish_shape();
                    }
                }
            }

            painter.set_clip_rect(response.rect);

            self.canvas.render(&painter, self.zoom, self.pan_offset);
        });
    }
}
