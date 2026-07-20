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
        ShapeData::Text { pos, text, color, size, .. } => {
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(to_skia_color(*color));

            if let Some(font) = make_font(*size) {
                // baseline offset approx size * 0.8 to match top-left positioning of egui
                canvas.draw_str(text, (pos.x, pos.y + size * 0.8), &font, &paint);
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
        ShapeData::StickyNote { rect, text, bg_color, text_color, text_size, .. } => {
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
            if let Some(font) = make_font(*text_size) {
                canvas.draw_str(text, (rect.min.x + padding, rect.min.y + padding + text_size * 0.8), &font, &text_paint);
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
