use eframe::egui;

/// Try to extract a URL from a dropped file (checking bytes, file path, .webloc, .url files).
pub fn extract_url_from_dropped_file(file: &egui::DroppedFile) -> Option<String> {
    if let Some(bytes) = &file.bytes {
        if let Ok(text) = std::str::from_utf8(bytes) {
            if let Some(url) = extract_url(text) {
                return Some(url);
            }
        }
    }
    if let Some(path) = &file.path {
        if path.extension().map_or(false, |ext| ext == "webloc") {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Some(start) = text.find("<string>") {
                    if let Some(end) = text[start..].find("</string>") {
                        let candidate = &text[start + 8..start + end];
                        if let Some(url) = extract_url(candidate) {
                            return Some(url);
                        }
                    }
                }
            }
        }
        if path.extension().map_or(false, |ext| ext == "url") {
            if let Ok(text) = std::fs::read_to_string(path) {
                for line in text.lines() {
                    let clean = line.trim();
                    if clean.to_lowercase().starts_with("url=") {
                        if let Some(url) = extract_url(&clean[4..]) {
                            return Some(url);
                        }
                    }
                }
            }
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Some(url) = extract_url(&text) {
                return Some(url);
            }
        }
    }
    None
}

/// Extract the first URL found in text, if any.
pub fn extract_url(text: &str) -> Option<String> {
    for word in text.trim().split_whitespace() {
        let clean = word.trim_matches(|c: char| {
            c == '('
                || c == ')'
                || c == '<'
                || c == '>'
                || c == '"'
                || c == '\''
                || c == ','
                || c == ';'
                || c == '!'
                || c == '?'
        });
        if clean.starts_with("http://") || clean.starts_with("https://") {
            return Some(clean.to_string());
        } else if clean.starts_with("www.") {
            return Some(format!("https://{clean}"));
        } else if clean.contains('.') && !clean.contains('@') && !clean.ends_with('.') {
            let parts: Vec<&str> = clean.split('/').next().unwrap_or("").split('.').collect();
            if parts.len() >= 2 {
                let tld = parts.last().unwrap_or(&"");
                if matches!(
                    *tld,
                    "com"
                        | "org"
                        | "net"
                        | "io"
                        | "dev"
                        | "app"
                        | "ai"
                        | "co"
                        | "uk"
                        | "de"
                        | "fr"
                        | "it"
                        | "es"
                        | "ca"
                        | "me"
                        | "info"
                        | "tech"
                        | "xyz"
                ) {
                    return Some(format!("https://{clean}"));
                }
            }
        }
    }
    None
}

/// Fallback domain name from a URL for immediate preview.
pub fn domain_fallback(url: &str) -> String {
    let clean = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    let host = clean.split('/').next().unwrap_or(clean);
    if host.is_empty() {
        "website".to_string()
    } else {
        host.to_string()
    }
}

/// Truncate a title to a maximum number of characters cleanly.
pub fn truncate_title(title: &str, max_chars: usize) -> String {
    let clean = title.trim();
    if clean.chars().count() > max_chars {
        let truncated: String = clean.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated.trim())
    } else {
        clean.to_string()
    }
}

/// Fetch website HTML title in background thread.
pub fn fetch_website_title(url: &str) -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .ok()?;

    let response = client.get(url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().ok()?;

    let lower_body = body.to_lowercase();
    let start_idx = lower_body.find("<title")?;
    let rest = &body[start_idx..];
    let tag_end = rest.find('>')? + 1;
    let content_rest = &rest[tag_end..];
    let end_idx = content_rest.to_lowercase().find("</title")?;

    let raw_title = &content_rest[..end_idx];
    let cleaned = raw_title
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace('\r', " ")
        .replace('\n', " ");

    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let title = words.join(" ");
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_url() {
        assert_eq!(
            extract_url("Check out https://github.com/salernoelia/kugel!"),
            Some("https://github.com/salernoelia/kugel".to_string())
        );
        assert_eq!(
            extract_url("www.example.com/test"),
            Some("https://www.example.com/test".to_string())
        );
        assert_eq!(
            extract_url("Visit google.com for search"),
            Some("https://google.com".to_string())
        );
        assert_eq!(extract_url("just plain text without link"), None);
    }

    #[test]
    fn test_domain_fallback() {
        assert_eq!(
            domain_fallback("https://github.com/salernoelia/kugel"),
            "github.com"
        );
        assert_eq!(
            domain_fallback("https://www.news.ycombinator.com/item?id=123"),
            "news.ycombinator.com"
        );
        assert_eq!(domain_fallback("https://"), "website");
    }

    #[test]
    fn test_truncate_title() {
        assert_eq!(truncate_title("Short Title", 20), "Short Title");
        assert_eq!(
            truncate_title("Very Long Website Title That Exceeds Limit", 20),
            "Very Long Website..."
        );
        assert_eq!(truncate_title("   Spaces   ", 10), "Spaces");
    }

    #[test]
    fn test_extract_url_from_dropped_file() {
        let dropped = egui::DroppedFile {
            bytes: Some(std::sync::Arc::new(*b"https://rust-lang.org")),
            ..Default::default()
        };
        assert_eq!(
            extract_url_from_dropped_file(&dropped),
            Some("https://rust-lang.org".to_string())
        );
    }
}
