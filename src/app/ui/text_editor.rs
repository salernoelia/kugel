use crate::app::App;
use crate::shapes::{ShapeData, Tool};
use eframe::egui;

pub fn render_inline_text_editor(app: &mut App, ctx: &egui::Context) {
    if let Some(idx) = app.editing_text_index {
        if idx < app.canvas.shapes.len() {
            let (canvas_pos, text_size, text_color) = match &app.canvas.shapes[idx].data {
                ShapeData::Text {
                    pos, size, color, ..
                } => (*pos, *size, *color),
                ShapeData::StickyNote {
                    rect,
                    text_size,
                    text_color,
                    bg_color,
                    ..
                } => {
                    let dark_mode = ctx.style().visuals.dark_mode;
                    let tc = if dark_mode && *bg_color == egui::Color32::from_rgb(255, 243, 176) {
                        egui::Color32::from_rgb(245, 235, 205)
                    } else {
                        *text_color
                    };
                    (rect.min + egui::vec2(8.0, 8.0), *text_size, tc)
                }
                _ => (egui::Pos2::ZERO, 24.0, egui::Color32::WHITE),
            };
            let screen_pos = app.canvas_to_screen(canvas_pos);

            egui::Area::new(egui::Id::new("inline_text_edit"))
                .fixed_pos(screen_pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let font_id = egui::FontId::proportional(text_size * app.zoom);

                    let wrap_px: Option<f32> = match &app.canvas.shapes[idx].data {
                        ShapeData::StickyNote { rect, .. } => {
                            Some((rect.width() - 16.0) * app.zoom)
                        }
                        ShapeData::Text {
                            max_width: Some(mw),
                            ..
                        } => Some(mw * app.zoom),
                        _ => None,
                    };

                    let layout_font = font_id.clone();
                    let mut layouter =
                        move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _w: f32| {
                            let job = egui::text::LayoutJob::simple(
                                buf.as_str().to_owned(),
                                layout_font.clone(),
                                text_color,
                                wrap_px.unwrap_or(f32::INFINITY),
                            );
                            ui.fonts(|f| f.layout_job(job))
                        };

                    let mut text_edit =
                        egui::TextEdit::multiline(&mut app.editing_text_buffer)
                            .font(font_id)
                            .text_color(text_color)
                            .frame(false)
                            .margin(egui::Margin::same(0))
                            .layouter(&mut layouter);

                    if let Some(w) = wrap_px {
                        text_edit = text_edit.desired_width(w);
                    }

                    let response = ui.add(text_edit);
                    if app.request_text_focus {
                        response.request_focus();
                        app.request_text_focus = false;
                    }

                    match &mut app.canvas.shapes[idx].data {
                        ShapeData::Text { text, .. } => {
                            *text = app.editing_text_buffer.clone();
                        }
                        ShapeData::StickyNote { text, .. } => {
                            *text = app.editing_text_buffer.clone();
                        }
                        _ => {}
                    }
                    app.check_and_spawn_title_preview_for_shape(idx, ctx);

                    let pressed_esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let pressed_cmd_enter = ui.input(|i| {
                        (i.modifiers.command || i.modifiers.ctrl)
                            && i.key_pressed(egui::Key::Enter)
                    });

                    if response.lost_focus() || pressed_esc || pressed_cmd_enter {
                        let is_empty = app.editing_text_buffer.trim().is_empty();
                        match &mut app.canvas.shapes[idx].data {
                            ShapeData::Text { text, .. } => {
                                if is_empty {
                                    app.canvas.shapes.remove(idx);
                                    app.clear_selection();
                                } else {
                                    *text = app.editing_text_buffer.clone();
                                }
                            }
                            ShapeData::StickyNote { text, .. } => {
                                *text = app.editing_text_buffer.clone();
                            }
                            _ => {}
                        }
                        app.is_dirty = true;
                        app.check_and_spawn_title_preview_for_shape(idx, ctx);
                        app.editing_text_index = None;
                        app.tool = Tool::Select;
                    }
                });
        } else {
            app.editing_text_index = None;
        }
    }
}
