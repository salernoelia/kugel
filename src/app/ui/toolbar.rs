use crate::app::App;
use crate::icons::Icons;
use crate::shapes::Tool;
use eframe::egui;

pub fn render_bottom_toolbar(
    app: &mut App,
    ctx: &egui::Context,
    icons: &Icons,
    panel_bg: egui::Color32,
    panel_stroke: egui::Stroke,
    is_dark: bool,
) {
    let compact_toolbar = ctx.screen_rect().width() < 1080.0;
    egui::Area::new(egui::Id::new("bottom_toolbar"))
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -20.0])
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(panel_bg)
                .stroke(panel_stroke)
                .corner_radius(egui::CornerRadius::same(14))
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let tools = [
                            (Tool::Select, &icons.select, "Select (V)"),
                            (Tool::Pen, &icons.pen, "Pen (P)"),
                            (Tool::Line, &icons.line, "Line (L)"),
                            (Tool::Rectangle, &icons.rectangle, "Rectangle (R)"),
                            (Tool::Circle, &icons.circle, "Circle (O)"),
                            (Tool::Text, &icons.text, "Text (T)"),
                            (Tool::StickyNote, &icons.note, "Sticky Note (N)"),
                            (Tool::Section, &icons.section, "Section (F)"),
                        ];
                        if compact_toolbar {
                            ui.spacing_mut().button_padding = egui::vec2(6.0, 6.0);
                            ui.spacing_mut().item_spacing.x = 4.0;
                        }
                        for &(t, icon_tex, tooltip) in &tools {
                            let selected = app.tool == t;
                            if icons
                                .selectable_icon_button(ui, selected, icon_tex, tooltip)
                                .clicked()
                            {
                                app.tool = t;
                                app.clear_selection();
                                app.editing_text_index = None;
                            }
                        }

                        if compact_toolbar {
                            return;
                        }

                        ui.separator();

                        if icons
                            .icon_button(ui, &icons.import, "Import Image (I)")
                            .clicked()
                        {
                            app.import_image_dialog(ctx);
                        }

                        ui.separator();

                        let paintbrush_tex = if is_dark {
                            &icons.paintbrush.dark
                        } else {
                            &icons.paintbrush.light
                        };
                        let size_image = egui::Image::new(paintbrush_tex)
                            .fit_to_exact_size(egui::vec2(18.0, 18.0));
                        ui.add(size_image).on_hover_text("Stroke Size");
                        ui.add(
                            egui::Slider::new(&mut app.stroke_width, 1.0..=20.0)
                                .show_value(false),
                        );

                        let color_resp = egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut app.selected_color,
                            egui::color_picker::Alpha::Opaque,
                        )
                        .on_hover_text("Stroke Color");

                        if color_resp.changed() && !app.selected_shape_indices.is_empty() {
                            if !app.recoloring_selection {
                                app.canvas.history.push(app.canvas.shapes.clone());
                                app.canvas.undo_history.clear();
                                app.recoloring_selection = true;
                            }
                            for &idx in &app.selected_shape_indices {
                                if let Some(shape) = app.canvas.shapes.get_mut(idx) {
                                    shape.data.set_color(app.selected_color);
                                }
                            }
                            app.is_dirty = true;
                        }
                        if app.recoloring_selection && ui.input(|i| i.pointer.any_released()) {
                            app.recoloring_selection = false;
                        }

                        ui.checkbox(&mut app.filled_shapes, "Fill");

                        ui.separator();

                        if icons.icon_button(ui, &icons.undo, "Undo (Cmd+Z)").clicked() {
                            app.canvas.undo();
                            app.clear_selection();
                            app.editing_text_index = None;
                            app.is_dirty = true;
                        }
                        if icons.icon_button(ui, &icons.redo, "Redo (Cmd+Y)").clicked() {
                            app.canvas.redo();
                            app.clear_selection();
                            app.editing_text_index = None;
                            app.is_dirty = true;
                        }
                        if icons
                            .icon_button(ui, &icons.clear, "Clear Canvas")
                            .clicked()
                        {
                            app.canvas.clear();
                            app.clear_selection();
                            app.editing_text_index = None;
                            app.is_dirty = true;
                        }

                        ui.separator();

                        if icons
                            .icon_button(ui, &icons.save, "Save Board (Cmd+S)")
                            .clicked()
                        {
                            app.save();
                        }
                        if icons
                            .icon_button(ui, &icons.open, "Open Board (Cmd+O)")
                            .clicked()
                        {
                            app.open_file_dialog(ctx);
                        }
                        if icons
                            .icon_button(ui, &icons.export, "Export Board (Cmd+E)")
                            .clicked()
                        {
                            app.show_export_dialog = true;
                        }
                    });
                });
        });
}
