use crate::models::Annotation;
use base64::Engine as _;
use ebook_rs::Book;

/// Safely find the closing `>` of an HTML/XML tag, ignoring any `>` characters
/// that appear inside attribute quotes (`"..."` or `'...'`).
pub fn find_tag_end(tag_and_rest: &str) -> Option<usize> {
    let mut in_quote: Option<char> = None;
    for (i, c) in tag_and_rest.char_indices() {
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            }
        } else {
            match c {
                '"' | '\'' => in_quote = Some(c),
                '>' => return Some(i + 1),
                _ => {}
            }
        }
    }
    None
}

/// Pin unsized illustrations to the reading column width.
pub fn constrain_images(html: &str, column_px: f32) -> String {
    if !html.contains("<img") {
        return html.to_string();
    }
    let column = column_px.max(120.0).round() as i64;
    let mut out = String::with_capacity(html.len() + 64);
    let mut rest = html;
    while let Some(pos) = rest.find("<img") {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let end = find_tag_end(tail).unwrap_or(tail.len());
        let tag = &tail[..end];
        if tag.contains("width=") || tag.contains("style=") {
            out.push_str(tag);
        } else {
            let insert_at = tag.len() - if tag.ends_with("/>") { 2 } else { 1 };
            out.push_str(&tag[..insert_at]);
            out.push_str(&format!(" style=\"width:{column}px\""));
            out.push_str(&tag[insert_at..]);
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Generate CSS overrides for Dark, Sepia, Nord, Solarized, and E-Ink themes
/// to prevent publisher stylesheets with hardcoded black text or white backgrounds
/// from blinding the reader.
pub fn theme_style_override(theme: &str) -> &'static str {
    match theme {
        "Dark" => {
            "<style>body, div, p, span, li, h1, h2, h3, h4, h5, h6, table, tr, th, td, blockquote, pre, code { color: #e0e0e0 !important; background-color: transparent !important; } a { color: #8ab4f8 !important; } table { border-collapse: collapse; } th, td { border: 1px solid #444; padding: 4px 8px; } blockquote { border-left: 3px solid #666; margin: 1em 0; padding-left: 1em; }</style>"
        }
        "Sepia" => {
            "<style>body, div, p, span, li, h1, h2, h3, h4, h5, h6, table, tr, th, td, blockquote, pre, code { color: #5b4636 !important; background-color: transparent !important; } a { color: #8c5b36 !important; } table { border-collapse: collapse; } th, td { border: 1px solid #c8b9a6; padding: 4px 8px; } blockquote { border-left: 3px solid #8c5b36; margin: 1em 0; padding-left: 1em; }</style>"
        }
        "Nord" => {
            "<style>body, div, p, span, li, h1, h2, h3, h4, h5, h6, table, tr, th, td, blockquote, pre, code { color: #eceff4 !important; background-color: transparent !important; } a { color: #88c0d0 !important; } table { border-collapse: collapse; } th, td { border: 1px solid #4c566a; padding: 4px 8px; } blockquote { border-left: 3px solid #88c0d0; margin: 1em 0; padding-left: 1em; }</style>"
        }
        "Solarized" => {
            "<style>body, div, p, span, li, h1, h2, h3, h4, h5, h6, table, tr, th, td, blockquote, pre, code { color: #657b83 !important; background-color: transparent !important; } a { color: #268bd2 !important; } table { border-collapse: collapse; } th, td { border: 1px solid #93a1a1; padding: 4px 8px; } blockquote { border-left: 3px solid #268bd2; margin: 1em 0; padding-left: 1em; }</style>"
        }
        "Solarized Dark" => {
            "<style>body, div, p, span, li, h1, h2, h3, h4, h5, h6, table, tr, th, td, blockquote, pre, code { color: #839496 !important; background-color: transparent !important; } a { color: #2aa198 !important; } table { border-collapse: collapse; } th, td { border: 1px solid #586e75; padding: 4px 8px; } blockquote { border-left: 3px solid #2aa198; margin: 1em 0; padding-left: 1em; }</style>"
        }
        "E-Ink" => {
            "<style>body, div, p, span, li, h1, h2, h3, h4, h5, h6, table, tr, th, td, blockquote, pre, code { color: #000000 !important; background-color: transparent !important; font-weight: 500 !important; } a { color: #000000 !important; text-decoration: underline !important; } table { border-collapse: collapse; } th, td { border: 1px solid #000; padding: 4px 8px; } blockquote { border-left: 3px solid #000; margin: 1em 0; padding-left: 1em; }</style>"
        }
        _ => "",
    }
}

/// Safely highlights occurrences of `query` within text nodes only (never inside HTML tags or attributes).
pub fn highlight_in_html(html: &str, query: &str, style: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() || !html.to_lowercase().contains(&trimmed.to_lowercase()) {
        return html.to_string();
    }
    let query_lower = trimmed.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let chars: Vec<char> = html.chars().collect();
    let mut out = String::with_capacity(html.len() + 128);
    let mut in_tag = false;
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '<' {
            in_tag = true;
            out.push(chars[i]);
            i += 1;
            continue;
        }
        if chars[i] == '>' {
            in_tag = false;
            out.push(chars[i]);
            i += 1;
            continue;
        }
        if in_tag {
            out.push(chars[i]);
            i += 1;
            continue;
        }

        // Check if query matches starting at i
        let mut matches = true;
        if i + query_chars.len() <= chars.len() {
            for k in 0..query_chars.len() {
                if chars[i + k].to_lowercase().next() != Some(query_chars[k]) {
                    matches = false;
                    break;
                }
            }
        } else {
            matches = false;
        }

        if matches {
            let matched_text: String = chars[i..i + query_chars.len()].iter().collect();
            out.push_str(&format!("<mark style=\"{style}\">{matched_text}</mark>"));
            i += query_chars.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }

    out
}

/// Turn a section's `processed_html` into something the GPUI renderer can show,
/// with dark-mode styling overrides, visual in-text highlights, active search highlights,
/// and optional text justification.
pub fn prepare_section_html(
    html: &str,
    book: &Book,
    section_path: &str,
    annotations: &[Annotation],
    active_search: Option<&str>,
    theme: &str,
    justify: bool,
) -> String {
    let body = strip_element(&strip_element(html, "head"), "title");
    let mut body = inline_svg_images(&body, book, section_path);
    body = make_images_clickable(&body);

    // Apply persistent visual highlights with their respective color styles
    for a in annotations {
        if a.is_highlight() && !a.quote.trim().is_empty() {
            body = highlight_in_html(&body, &a.quote, a.highlight_style());
        }
    }

    // Apply active search query highlight
    if let Some(q) = active_search
        && !q.trim().is_empty()
    {
        body = highlight_in_html(
            &body,
            q,
            "background-color: #ff9800; color: #000; font-weight: bold; border-radius: 2px; padding: 0 2px;",
        );
    }

    // Prepend theme CSS reset and text justification rules
    let mut prefix = theme_style_override(theme).to_string();
    if justify {
        prefix.push_str("<style>body, div, p, span, li { text-align: justify !important; text-justify: inter-word !important; hyphens: auto !important; }</style>");
    }
    if !prefix.is_empty() {
        format!("{prefix}{body}")
    } else {
        body
    }
}

/// Wrap every `<img>` in an `<a href="img:...">` so clicking an image opens
/// the lightbox modal.
pub fn make_images_clickable(html: &str) -> String {
    if !html.contains("<img") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len() + 64);
    let mut rest = html;
    while let Some(pos) = rest.find("<img") {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let end = find_tag_end(tail).unwrap_or(tail.len());
        let tag = &tail[..end];
        let src = extract_img_src(tag);
        if let Some(src) = src {
            out.push_str(&format!("<a href='img:{}'>", src));
            out.push_str(tag);
            out.push_str("</a>");
        } else {
            out.push_str(tag);
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

pub fn extract_img_src(tag: &str) -> Option<String> {
    for q in ['"', '\''] {
        let pat = format!("src={}", q);
        if let Some(start) = tag.find(&pat) {
            let from = start + pat.len();
            if let Some(len) = tag[from..].find(q) {
                return Some(tag[from..from + len].to_string());
            }
        }
    }
    None
}

/// Best-effort print-page label for the current section, from the book's
/// page list (EPUB 2 NCX / EPUB 3 nav).
pub fn current_page_label(book: &Book, current: usize) -> Option<String> {
    let sec = book.get_section(current).ok()?;
    let href = sec.href.clone();
    let full = sec.full_path.clone();
    book.page_list()
        .iter()
        .rev()
        .find(|p| {
            let t = p.href.split('#').next().unwrap_or(&p.href);
            t == href || t == full || full.ends_with(t) || href.ends_with(t)
        })
        .map(|p| format!("p. {}", p.page))
}

/// Remove every `<tag …>…</tag>` block, keeping everything around it.
pub fn strip_element(html: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    if !html.contains(&open) {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find(&open) {
        let tail = &rest[start..];
        let after = tail[open.len()..].chars().next();
        let is_tag = matches!(after, Some(c) if c == '>' || c == '/' || c.is_whitespace());
        if !is_tag {
            out.push_str(&rest[..start + open.len()]);
            rest = &tail[open.len()..];
            continue;
        }
        out.push_str(&rest[..start]);
        match tail.find(&close) {
            Some(end) => rest = &tail[end + close.len()..],
            None => match find_tag_end(tail) {
                Some(gt) => rest = &tail[gt..],
                None => rest = "",
            },
        }
    }
    out.push_str(rest);
    out
}

/// Rewrite SVG `<image>` elements into plain `<img>` tags with inlined `data:` URI.
pub fn inline_svg_images(html: &str, book: &Book, section_path: &str) -> String {
    if !html.contains("<image") {
        return html.to_string();
    }
    let dir = section_path
        .rfind('/')
        .map(|i| &section_path[..i])
        .unwrap_or("");
    let mut out = String::with_capacity(html.len() + 256);
    let mut rest = html;
    while let Some(pos) = rest.find("<image") {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let end = find_tag_end(tail).unwrap_or(tail.len());
        let tag = &tail[..end];
        match image_href(tag) {
            Some(href) if href.starts_with("data:") => {
                out.push_str(&format!("<img src=\"{href}\" style=\"width:100%\" />"));
            }
            Some(href) => match image_resource(book, dir, href) {
                Some((bytes, mime)) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    out.push_str(&format!(
                        "<img src=\"data:{mime};base64,{b64}\" style=\"width:100%\" />"
                    ));
                }
                None => out.push_str(tag),
            },
            None => out.push_str(tag),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Pull the source out of an `<image>` tag, whichever attribute name is used.
pub fn image_href(tag: &str) -> Option<&str> {
    for attr in ["xlink:href=\"", "xlink:href='", "href=\"", "href='"] {
        if let Some(start) = tag.find(attr) {
            let quote = attr.chars().last()?;
            let from = start + attr.len();
            if let Some(len) = tag[from..].find(quote) {
                return Some(&tag[from..from + len]);
            }
        }
    }
    None
}

/// Read an `<image>` reference out of the archive.
pub fn image_resource(book: &Book, dir: &str, href: &str) -> Option<(Vec<u8>, &'static str)> {
    if href.starts_with("data:") || href.starts_with("http") {
        return None;
    }
    for candidate in [resolve_relative(dir, href), href.to_string()] {
        if let Ok(hit) = book.get_resource_bytes(&candidate) {
            return Some(hit);
        }
    }
    None
}

/// Join a relative reference onto a directory, collapsing `.` and `..`.
pub fn resolve_relative(dir: &str, href: &str) -> String {
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        dir.split('/').filter(|s| !s.is_empty()).collect()
    };
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Escape plain text for rendering inside the HTML `TextView`, keeping line breaks.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_in_html() {
        let html = "<p>The quick brown fox jumps over the lazy dog.</p>";
        let res = highlight_in_html(html, "fox", "background: yellow;");
        assert_eq!(
            res,
            "<p>The quick brown <mark style=\"background: yellow;\">fox</mark> jumps over the lazy dog.</p>"
        );
    }

    #[test]
    fn test_highlight_does_not_corrupt_tags() {
        let html = r#"<img src="fox_image.jpg" alt="fox"><p>fox</p>"#;
        let res = highlight_in_html(html, "fox", "background: yellow;");
        assert!(res.starts_with(r#"<img src="fox_image.jpg" alt="fox">"#));
        assert!(res.contains(r#"<mark style="background: yellow;">fox</mark>"#));
    }

    #[test]
    fn test_theme_style_override() {
        assert!(theme_style_override("Dark").contains("#e0e0e0 !important"));
        assert!(theme_style_override("Sepia").contains("#5b4636 !important"));
        assert!(theme_style_override("Nord").contains("#eceff4 !important"));
        assert!(theme_style_override("Solarized").contains("#657b83 !important"));
        assert_eq!(theme_style_override("Light"), "");
    }
}
