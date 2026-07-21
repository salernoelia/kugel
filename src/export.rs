use crate::shapes::{Shape, ShapeData};
use eframe::egui;
use std::path::Path;

pub fn export_canvas_to_image(
    shapes: &[Shape],
    bg_color: egui::Color32,
    scale_factor: f32,
    export_path: &Path,
    use_jpeg: bool,
    jpeg_quality: i32,
) -> Result<(), String> {
    if shapes.is_empty() {
        return Err("Cannot export an empty canvas".to_string());
    }

    // 1. Calculate bounding box of all shapes
    let mut bounds = egui::Rect::NOTHING;
    for shape in shapes {
        let sb = shape.data.get_bounds();
        if sb.is_positive() {
            bounds = bounds.union(sb);
        }
    }

    if !bounds.is_positive() {
        return Err("No exportable elements found on canvas".to_string());
    }

    // Add padding around the elements (50 canvas units)
    let padding = 50.0;
    let min_x = bounds.min.x - padding;
    let min_y = bounds.min.y - padding;
    let width = bounds.width() + padding * 2.0;
    let height = bounds.height() + padding * 2.0;

    // Multiply by resolution scale factor
    let export_width = (width * scale_factor).round() as i32;
    let export_height = (height * scale_factor).round() as i32;

    if export_width <= 0 || export_height <= 0 {
        return Err("Invalid export dimensions".to_string());
    }

    // 2. Create offscreen Skia surface
    let mut surface = skia_safe::surfaces::raster_n32_premul((export_width, export_height))
        .ok_or_else(|| "Failed to create Skia surface".to_string())?;
    
    let canvas = surface.canvas();

    // Clear background
    let sk_bg_color = to_skia_color(bg_color);
    canvas.clear(sk_bg_color);

    // Apply scaling and translation
    canvas.save();
    canvas.scale((scale_factor, scale_factor));
    canvas.translate((-min_x, -min_y));

    // 3. Draw shapes
    for shape in shapes {
        draw_shape_to_skia(canvas, &shape.data)?;
    }

    canvas.restore();

    // 4. Encode and save image
    let image = surface.image_snapshot();
    let data = if use_jpeg {
        image
            .encode(
                None,
                skia_safe::EncodedImageFormat::JPEG,
                Some(jpeg_quality.clamp(1, 100) as u32),
            )
            .ok_or_else(|| "Failed to encode image to JPEG".to_string())?
    } else {
        image
            .encode(None, skia_safe::EncodedImageFormat::PNG, None)
            .ok_or_else(|| "Failed to encode image to PNG".to_string())?
    };

    std::fs::write(export_path, data.as_bytes())
        .map_err(|e| format!("Failed to write export file: {}", e))?;

    Ok(())
}

fn to_skia_color(c: egui::Color32) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a(), c.r(), c.g(), c.b())
}

// Bundled font so export is deterministic and independent of system fonts.
// Font::default() has no typeface, so draw_str would render nothing.
const OPEN_SANS: &[u8] = include_bytes!("../assets/fonts/OpenSans-Regular.ttf");

fn make_font(size: f32) -> Option<skia_safe::Font> {
    let font_mgr = skia_safe::FontMgr::new();
    let typeface = font_mgr.new_from_data(OPEN_SANS, None)?;
    Some(skia_safe::Font::from_typeface(typeface, size))
}

fn wrap_text_to_lines(text: &str, font: &skia_safe::Font, max_width: Option<f32>) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        let raw_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if let Some(mw) = max_width {
            if mw > 0.0 {
                let mut current_line = String::new();
                let words: Vec<&str> = raw_line.split(' ').collect();
                for word in words {
                    if current_line.is_empty() {
                        let (w, _) = font.measure_str(word, None);
                        if w > mw {
                            for ch in word.chars() {
                                let mut test_str = current_line.clone();
                                test_str.push(ch);
                                let (test_w, _) = font.measure_str(&test_str, None);
                                if test_w > mw && !current_line.is_empty() {
                                    lines.push(current_line);
                                    current_line = ch.to_string();
                                } else {
                                    current_line.push(ch);
                                }
                            }
                        } else {
                            current_line.push_str(word);
                        }
                    } else {
                        let test_str = format!("{} {}", current_line, word);
                        let (test_w, _) = font.measure_str(&test_str, None);
                        if test_w > mw {
                            lines.push(current_line);
                            current_line = word.to_string();
                        } else {
                            current_line = test_str;
                        }
                    }
                }
                lines.push(current_line);
            } else {
                lines.push(raw_line.to_string());
            }
        } else {
            lines.push(raw_line.to_string());
        }
    }
    lines
}

fn draw_shape_to_skia(canvas: &skia_safe::Canvas, data: &ShapeData) -> Result<(), String> {
    match data {
        ShapeData::Pen { points, color, stroke_width } => {
            if points.len() > 1 {
                let mut path = skia_safe::Path::new();
                path.move_to((points[0].x, points[0].y));
                for p in points.iter().skip(1) {
                    path.line_to((p.x, p.y));
                }

                let mut paint = skia_safe::Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_color(to_skia_color(*color));
                paint.set_stroke_width(*stroke_width);
                paint.set_stroke_cap(skia_safe::PaintCap::Round);
                paint.set_stroke_join(skia_safe::PaintJoin::Round);

                canvas.draw_path(&path, &paint);
            }
        }
        ShapeData::Line { start, end, color, stroke_width } => {
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(skia_safe::paint::Style::Stroke);
            paint.set_color(to_skia_color(*color));
            paint.set_stroke_width(*stroke_width);
            paint.set_stroke_cap(skia_safe::PaintCap::Round);
            canvas.draw_line((start.x, start.y), (end.x, end.y), &paint);
        }
        ShapeData::Rectangle { rect, color, stroke_width, filled } => {
            let sk_rect = skia_safe::Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y);
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(to_skia_color(*color));

            if *filled {
                paint.set_style(skia_safe::paint::Style::Fill);
                canvas.draw_rect(sk_rect, &paint);
            } else {
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(*stroke_width);
                canvas.draw_rect(sk_rect, &paint);
            }
        }
        ShapeData::Circle { center, radius, color, stroke_width, filled } => {
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(to_skia_color(*color));

            if *filled {
                paint.set_style(skia_safe::paint::Style::Fill);
                canvas.draw_circle((center.x, center.y), *radius, &paint);
            } else {
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(*stroke_width);
                canvas.draw_circle((center.x, center.y), *radius, &paint);
            }
        }
        ShapeData::Text { pos, text, color, size, max_width, link_title, .. } => {
            if let Some(lt) = link_title {
                let mut title_paint = skia_safe::Paint::default();
                title_paint.set_anti_alias(true);
                title_paint.set_color(skia_safe::Color::from_rgb(147, 197, 253));
                if let Some(font) = make_font(13.0) {
                    canvas.draw_str(lt, (pos.x, pos.y - 4.0), &font, &title_paint);
                }
            }
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(to_skia_color(*color));

            if let Some(font) = make_font(*size) {
                let line_height = *size * 1.25;
                let lines = wrap_text_to_lines(text, &font, *max_width);
                for (i, line) in lines.iter().enumerate() {
                    let y = pos.y + size * 0.8 + (i as f32 * line_height);
                    canvas.draw_str(line, (pos.x, y), &font, &paint);
                }
            }
        }
        ShapeData::Image { rect, bytes, .. } => {
            if let Some(skia_img) = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(bytes)) {
                let sk_rect = skia_safe::Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y);
                let paint = skia_safe::Paint::default();
                canvas.draw_image_rect(
                    &skia_img,
                    None,
                    &sk_rect,
                    &paint,
                );
            }
        }
        ShapeData::StickyNote { rect, text, bg_color, text_color, text_size, link_title, .. } => {
            if let Some(lt) = link_title {
                let mut title_paint = skia_safe::Paint::default();
                title_paint.set_anti_alias(true);
                title_paint.set_color(skia_safe::Color::from_rgb(147, 197, 253));
                if let Some(font) = make_font(13.0) {
                    canvas.draw_str(lt, (rect.min.x, rect.min.y - 4.0), &font, &title_paint);
                }
            }
            let sk_rect = skia_safe::Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y);
            
            let mut bg_paint = skia_safe::Paint::default();
            bg_paint.set_anti_alias(true);
            bg_paint.set_color(to_skia_color(*bg_color));
            bg_paint.set_style(skia_safe::paint::Style::Fill);
            
            let rrect = skia_safe::RRect::new_rect_xy(sk_rect, 6.0, 6.0);
            canvas.draw_rrect(rrect, &bg_paint);

            let mut text_paint = skia_safe::Paint::default();
            text_paint.set_anti_alias(true);
            text_paint.set_color(to_skia_color(*text_color));
            let padding = 8.0;
            let text_width = (rect.width() - padding * 2.0).max(10.0);
            if let Some(font) = make_font(*text_size) {
                let line_height = *text_size * 1.25;
                let lines = wrap_text_to_lines(text, &font, Some(text_width));
                for (i, line) in lines.iter().enumerate() {
                    let y = rect.min.y + padding + text_size * 0.8 + (i as f32 * line_height);
                    canvas.draw_str(line, (rect.min.x + padding, y), &font, &text_paint);
                }
            }
        }
        ShapeData::SectionBox { rect, color } => {
            let sk_rect = skia_safe::Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y);
            let rrect = skia_safe::RRect::new_rect_xy(sk_rect, 4.0, 4.0);
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(to_skia_color(*color));
            paint.set_style(skia_safe::paint::Style::Stroke);
            paint.set_stroke_width(1.5);
            canvas.draw_rrect(rrect, &paint);
        }
    }
    Ok(())
}
