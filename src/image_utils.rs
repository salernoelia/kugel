use std::path::Path;

/// Fit image raw dimensions into a compact display box on canvas
pub fn fit_display_size(raw_size: [f32; 2], max_w: f32, max_h: f32) -> [f32; 2] {
    let scale = (max_w / raw_size[0]).min(max_h / raw_size[1]).min(1.0);
    [raw_size[0] * scale, raw_size[1] * scale]
}

pub fn compress_and_scale(img: image::DynamicImage) -> Result<(Vec<u8>, [f32; 2]), String> {
    // Fast SIMD chunk check for non-opaque alpha pixels
    fn has_transparency(img: &image::DynamicImage) -> bool {
        if let Some(rgba) = img.as_rgba8() {
            let raw = rgba.as_raw();
            raw.chunks_exact(4).any(|px| px[3] != 255)
        } else {
            false
        }
    }

    let width = img.width();
    let height = img.height();
    let short_side = width.min(height);

    // Scale DOWN only — never enlarge. Cap short side to 1200px for high quality detail
    const MAX_SHORT_SIDE: u32 = 1200;
    let scaled_img = if short_side > MAX_SHORT_SIDE {
        let scale = MAX_SHORT_SIDE as f32 / short_side as f32;
        let new_w = (width as f32 * scale) as u32;
        let new_h = (height as f32 * scale) as u32;
        img.resize(new_w, new_h, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    let out_w = scaled_img.width();
    let out_h = scaled_img.height();
    let mut compressed_bytes = Vec::new();

    if scaled_img.color().has_alpha() && has_transparency(&scaled_img) {
        let encoder = image::codecs::png::PngEncoder::new(&mut compressed_bytes);
        scaled_img
            .write_with_encoder(encoder)
            .map_err(|e| e.to_string())?;
    } else {
        const JPEG_QUALITY: u8 = 90;
        let rgb = scaled_img.to_rgb8();
        let mut encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut compressed_bytes, JPEG_QUALITY);
        encoder
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
    }

    Ok((compressed_bytes, [out_w as f32, out_h as f32]))
}

/// Process a single file (PDF or Image) into a list of scaled image byte pairs.
pub fn process_file_to_images(path: &Path) -> Vec<(Vec<u8>, [f32; 2])> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());

    if ext.as_deref() == Some("pdf") {
        if let Ok(pdf_bytes) = std::fs::read(path) {
            if let Ok(page_images) = render_pdf_to_images(&pdf_bytes) {
                // Decode and scale all PDF pages in parallel across CPU worker threads
                let processed_pages: Vec<Option<(Vec<u8>, [f32; 2])>> = std::thread::scope(|s| {
                    let handles: Vec<_> = page_images
                        .into_iter()
                        .map(|page_bytes| {
                            s.spawn(move || {
                                let img = image::load_from_memory(&page_bytes).ok()?;
                                compress_and_scale(img).ok()
                            })
                        })
                        .collect();
                    handles.into_iter().map(|h| h.join().unwrap_or(None)).collect()
                });
                return processed_pages.into_iter().flatten().collect();
            }
        }
        Vec::new()
    } else if let Ok(bytes) = std::fs::read(path) {
        if let Ok(img) = image::load_from_memory(&bytes) {
            if let Ok(res) = compress_and_scale(img) {
                return vec![res];
            }
        }
        Vec::new()
    } else {
        Vec::new()
    }
}

/// Render pages of a PDF into individual PNG image bytes.
pub fn render_pdf_to_images(pdf_bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let pdf_path = temp_dir.path().join("doc.pdf");
    std::fs::write(&pdf_path, pdf_bytes).map_err(|e| e.to_string())?;

    let output_prefix = temp_dir.path().join("page");

    // 1. Try pdftoppm (high quality native 1200px page rendering)
    let pdftoppm_res = std::process::Command::new("pdftoppm")
        .arg("-png")
        .arg("-scale-to-x")
        .arg("1200")
        .arg("-scale-to-y")
        .arg("-1")
        .arg(&pdf_path)
        .arg(&output_prefix)
        .output();

    let mut png_paths: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(res) = pdftoppm_res {
        if res.status.success() {
            if let Ok(entries) = std::fs::read_dir(temp_dir.path()) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("png") {
                        png_paths.push(p);
                    }
                }
            }
        }
    }

    // 2. Fallback: qlmanage on macOS if pdftoppm failed or is absent
    if png_paths.is_empty() {
        let _ = std::process::Command::new("qlmanage")
            .arg("-t")
            .arg("-s")
            .arg("1200")
            .arg("-o")
            .arg(temp_dir.path())
            .arg(&pdf_path)
            .output();

        if let Ok(entries) = std::fs::read_dir(temp_dir.path()) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("png") {
                    png_paths.push(p);
                }
            }
        }
    }

    if png_paths.is_empty() {
        return Err("Could not render PDF pages".into());
    }

    // Sort page paths numerically (page-1.png, page-2.png, ... page-10.png)
    png_paths.sort_by(|a, b| {
        let num_a = extract_trailing_number(a.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
        let num_b = extract_trailing_number(b.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
        num_a.cmp(&num_b)
    });

    let mut images = Vec::new();
    for path in png_paths {
        if let Ok(bytes) = std::fs::read(&path) {
            images.push(bytes);
        }
    }

    if images.is_empty() {
        Err("No page images extracted".into())
    } else {
        Ok(images)
    }
}

pub fn extract_trailing_number(s: &str) -> usize {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_trailing_number() {
        assert_eq!(extract_trailing_number("page-1"), 1);
        assert_eq!(extract_trailing_number("page-2"), 2);
        assert_eq!(extract_trailing_number("page-10"), 10);
        assert_eq!(extract_trailing_number("no_digits"), 0);
    }

    #[test]
    fn test_fit_display_size() {
        assert_eq!(fit_display_size([1200.0, 1600.0], 200.0, 260.0), [195.0, 260.0]);
        assert_eq!(fit_display_size([100.0, 100.0], 200.0, 260.0), [100.0, 100.0]);
    }
}
