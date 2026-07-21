/// Detect whether a string looks like Markdown (has syntax we'd want to strip).
pub fn looks_like_markdown(text: &str) -> bool {
    text.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with('#')            // headings
            || t.starts_with("- ")    // unordered list
            || t.starts_with("* ")
            || t.starts_with("+ ")
            || t.starts_with("> ")    // blockquote
            || t.starts_with("```")   // fenced code
            || t.starts_with("|")     // table
    }) || text.contains("**")          // bold
        || text.contains("__")         // bold/underline
        || text.contains('`')          // inline code
        || (text.contains("](") && text.contains('[')) // links/images
}

/// Convert Markdown into plain text by removing the syntax that makes it Markdown.
/// Best-effort, line based; keeps the readable content, drops the markup.
pub fn strip_markdown(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;

    for raw in text.lines() {
        let trimmed = raw.trim_start();

        // Fenced code blocks: drop the ``` fences, keep the code lines verbatim.
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            out.push(raw.to_string());
            continue;
        }

        let indent = &raw[..raw.len() - trimmed.len()];
        let mut line = trimmed.to_string();

        // Headings: strip leading #'s and any trailing closing #'s.
        if line.starts_with('#') {
            line = line.trim_start_matches('#').trim_start().to_string();
            line = line.trim_end_matches('#').trim_end().to_string();
        }

        // Blockquotes: strip leading > markers.
        while line.starts_with('>') {
            line = line[1..].trim_start().to_string();
        }

        // List markers: "- ", "* ", "+ ", or "1. ".
        if let Some(rest) = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("+ "))
        {
            line = format!("• {}", rest);
        } else if let Some(pos) = line.find(". ") {
            if line[..pos].chars().all(|c| c.is_ascii_digit()) && pos > 0 {
                line = line[pos + 2..].to_string();
            }
        }

        // Horizontal rules -> blank line.
        if line == "---" || line == "***" || line == "___" {
            line.clear();
        }

        line = strip_inline_markdown(&line);
        out.push(format!("{}{}", indent, line));
    }

    out.join("\n")
}

/// Remove inline Markdown markup: emphasis, code spans, and links/images.
pub fn strip_inline_markdown(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            // Image / link: ![alt](url) or [text](url) -> keep alt/text.
            '!' if chars.get(i + 1) == Some(&'[') => {
                i += 1; // skip '!', fall through handles '['
                continue;
            }
            '[' => {
                if let Some(close) = chars[i..].iter().position(|&x| x == ']') {
                    let end = i + close;
                    // Must be followed by "(...)" to count as a link.
                    if chars.get(end + 1) == Some(&'(') {
                        if let Some(paren) = chars[end + 1..].iter().position(|&x| x == ')') {
                            out.extend(&chars[i + 1..end]);
                            i = end + 1 + paren + 1;
                            continue;
                        }
                    }
                }
                out.push(c);
                i += 1;
            }
            // Emphasis / bold markers: skip runs of * or _.
            '*' | '_' => {
                while i < chars.len() && (chars[i] == '*' || chars[i] == '_') {
                    i += 1;
                }
            }
            // Inline code: skip backticks, keep contents.
            '`' => {
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_markdown() {
        assert!(looks_like_markdown("# Heading 1"));
        assert!(looks_like_markdown("- Item 1"));
        assert!(looks_like_markdown("**Bold text**"));
        assert!(looks_like_markdown("[Link](https://example.com)"));
        assert!(!looks_like_markdown("Just normal plain text"));
    }

    #[test]
    fn test_strip_markdown_headings_and_lists() {
        let input = "# Main Title\n## Subtitle ##\n- Bullet 1\n> Quote line";
        let expected = "Main Title\nSubtitle\n• Bullet 1\nQuote line";
        assert_eq!(strip_markdown(input), expected);
    }

    #[test]
    fn test_strip_inline_markdown() {
        assert_eq!(strip_inline_markdown("Hello **world**"), "Hello world");
        assert_eq!(strip_inline_markdown("Click [here](https://link.com)"), "Click here");
        assert_eq!(strip_inline_markdown("Run `cargo test` now"), "Run cargo test now");
        assert_eq!(strip_inline_markdown("Image: ![alt text](img.png)"), "Image: alt text");
    }

    #[test]
    fn test_strip_markdown_fenced_code() {
        let input = "```rust\nfn main() {}\n```";
        let expected = "fn main() {}";
        assert_eq!(strip_markdown(input), expected);
    }
}
