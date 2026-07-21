use crate::app::App;
use crate::shapes::ShapeData;
use crate::url_utils::{domain_fallback, extract_url, fetch_website_title, truncate_title};
use crate::updater::UiEvent;
use eframe::egui;

impl App {
    /// If a text shape's content is nothing but a single URL, return it
    /// (normalized to an https:// prefix for bare `www.` links).
    pub fn text_shape_url(&self, idx: usize) -> Option<String> {
        let ShapeData::Text { text, .. } = &self.canvas.shapes.get(idx)?.data else {
            return None;
        };
        let t = text.trim();
        if t.is_empty() || t.split_whitespace().count() != 1 {
            return None;
        }
        if t.starts_with("http://") || t.starts_with("https://") {
            Some(t.to_string())
        } else if t.starts_with("www.") {
            Some(format!("https://{t}"))
        } else {
            None
        }
    }

    pub fn generate_missing_link_previews(&mut self, ctx: &egui::Context) {
        let count = self.canvas.shapes.len();
        for idx in 0..count {
            self.check_and_spawn_title_preview_for_shape(idx, ctx);
        }
    }

    pub fn check_and_spawn_title_preview_for_shape(&mut self, shape_idx: usize, ctx: &egui::Context) {
        if shape_idx >= self.canvas.shapes.len() {
            return;
        }
        let shape = &mut self.canvas.shapes[shape_idx];
        let text_content = match &shape.data {
            ShapeData::Text { text, .. } | ShapeData::StickyNote { text, .. } => text.clone(),
            _ => return,
        };

        let is_url = extract_url(&text_content);
        if let Some(url) = is_url {
            let current_url = shape.data.link_url().map(String::from);
            if current_url != Some(url.clone()) {
                shape.data.set_link_url(Some(url.clone()));
                let fallback = format!("🌐 {}", truncate_title(&domain_fallback(&url), 55));
                shape.data.set_link_title(Some(fallback));
                self.is_dirty = true;

                let shape_id = shape.id;
                let ui_tx = self.ui_event_tx.clone();
                let ctx_clone = ctx.clone();
                let url_clone = url;

                std::thread::spawn(move || {
                    let raw_title = fetch_website_title(&url_clone)
                        .unwrap_or_else(|| domain_fallback(&url_clone));
                    let title = truncate_title(&raw_title, 55);
                    let _ = ui_tx.send(UiEvent::LinkTitleFetched {
                        shape_id,
                        url: url_clone,
                        title: format!("🌐 {title}"),
                    });
                    ctx_clone.request_repaint();
                });
            }
        } else if shape.data.link_title().is_some() || shape.data.link_url().is_some() {
            shape.data.set_link_title(None);
            shape.data.set_link_url(None);
            self.is_dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_preview_cleared_when_not_link() {
        let mut app = App::default();
        let ctx = egui::Context::default();
        let idx = app.canvas.add_text(egui::pos2(0.0, 0.0), "https://github.com".into(), egui::Color32::WHITE);
        app.check_and_spawn_title_preview_for_shape(idx, &ctx);
        assert!(app.canvas.shapes[idx].data.link_title().is_some());

        if let ShapeData::Text { text, .. } = &mut app.canvas.shapes[idx].data {
            *text = "Just plain text without a link".to_string();
        }
        app.check_and_spawn_title_preview_for_shape(idx, &ctx);
        assert!(app.canvas.shapes[idx].data.link_title().is_none());
        assert!(app.canvas.shapes[idx].data.link_url().is_none());
    }

    #[test]
    fn test_link_preview_updated_when_url_changes() {
        let mut app = App::default();
        let ctx = egui::Context::default();
        let idx = app.canvas.add_text(egui::pos2(0.0, 0.0), "https://github.com".into(), egui::Color32::WHITE);
        app.check_and_spawn_title_preview_for_shape(idx, &ctx);
        assert_eq!(app.canvas.shapes[idx].data.link_url(), Some("https://github.com"));

        if let ShapeData::Text { text, .. } = &mut app.canvas.shapes[idx].data {
            *text = "https://wikipedia.org".to_string();
        }
        app.check_and_spawn_title_preview_for_shape(idx, &ctx);
        assert_eq!(app.canvas.shapes[idx].data.link_url(), Some("https://wikipedia.org"));
        assert!(app.canvas.shapes[idx].data.link_title().unwrap().contains("wikipedia.org"));
    }
}
