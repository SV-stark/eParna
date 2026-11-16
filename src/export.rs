use crate::models::{Annotation, RecentBook};

pub fn format_annotations_markdown(
    title: &str,
    author: &str,
    annotations: &[Annotation],
) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("title: \"{}\"\n", title.replace('"', "\\\"")));
    if !author.is_empty() {
        out.push_str(&format!("author: \"{}\"\n", author.replace('"', "\\\"")));
    }
    out.push_str("tags:\n  - reading-notes\n  - eParna\n");
    out.push_str("---\n\n");
    out.push_str(&format!("# Reading Notes: {title}\n\n"));
    if annotations.is_empty() {
        out.push_str("_No annotations yet._\n");
        return out;
    }
    for (i, a) in annotations.iter().enumerate() {
        out.push_str(&format!("### Item #{}\n\n", i + 1));
        let badge = match a.color.as_str() {
            "green" => "Green",
            "blue" => "Blue",
            "pink" => "Pink",
            "purple" => "Purple",
            _ => "Yellow",
        };
        out.push_str(&format!(
            "*Section {} · {} · Color: {} · Date: {}*\n\n",
            a.section + 1,
            if a.is_highlight() {
                "Highlight"
            } else {
                "Note"
            },
            badge,
            a.created
        ));
        if !a.quote.trim().is_empty() {
            for line in a.quote.trim().lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        if !a.note.trim().is_empty() {
            out.push_str(&format!("**Note:** {}\n\n", a.note));
        }
    }
    out
}

pub fn format_citations(book: &ebook_rs::Book) -> String {
    let mut out = String::new();
    out.push_str("=== BibTeX ===\n");
    out.push_str(&book.to_bibtex());
    out.push_str("\n\n=== APA (7th ed.) ===\n");
    out.push_str(&book.to_apa());
    out.push_str("\n\n=== MLA (9th ed.) ===\n");
    out.push_str(&book.to_mla());
    out.push_str("\n\n=== Chicago (17th ed.) ===\n");
    out.push_str(&book.to_chicago());
    out
}

pub fn format_validation(book: &ebook_rs::Book) -> String {
    let report = book.validate();
    let mut out = format!(
        "{} · {} error(s), {} warning(s), {} note(s)\n",
        if report.is_valid { "Valid" } else { "Invalid" },
        report.errors_count,
        report.warnings_count,
        report.info_count
    );
    if report.errors.is_empty() {
        out.push_str("\nNo structural problems found.");
        return out;
    }
    for e in report.errors.iter().take(50) {
        out.push_str(&format!(
            "\n[{:?}] {} — {}\n",
            e.severity, e.code, e.message
        ));
    }
    out
}

pub fn format_book_info(
    book: &ebook_rs::Book,
    current_section: usize,
    rtl_mode: bool,
    recent_entry: Option<&RecentBook>,
) -> String {
    let m = book.metadata();
    let mut out = format!("{}\n", m.title);
    if !m.creators.is_empty() {
        out.push_str(&format!("{}\n", m.creators.join(", ")));
    }
    if let Some(p) = m.publisher() {
        out.push_str(&format!("{p}\n"));
    }
    out.push_str(&format!("{} sections\n", book.sections().len()));

    if let Ok(sec) = book.get_section(current_section)
        && let (Some(w), Some(h)) = (sec.viewport_width, sec.viewport_height)
    {
        out.push_str(&format!("Page size: {:.0} × {:.0} pt\n", w, h));
    }

    if let Some(r) = recent_entry {
        let mins = r.reading_time_secs / 60;
        out.push_str(&format!("Time read: {mins} min\n"));
    }

    if !book.media_overlays().is_empty() {
        out.push_str(&format!(
            "Audio read-along (SMIL): {} overlay package(s) available\n",
            book.media_overlays().len()
        ));
    }

    out.push_str(&format!(
        "Reading direction: {}\n",
        if rtl_mode {
            "Right-to-Left (RTL)"
        } else {
            "Left-to-Right (LTR)"
        }
    ));

    out.push('\n');
    let detected_lang = book.detect_language();
    for (label, value) in [
        ("Language", Some(m.language().to_string())),
        ("Detected Language", detected_lang),
        ("Published", m.pub_date.clone()),
        ("Identifier", m.identifier.clone()),
        ("Rights", m.rights.clone()),
        (
            "Subjects",
            (!m.subjects.is_empty()).then(|| m.subjects.join(", ")),
        ),
    ] {
        if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
            out.push_str(&format!("{label}: {v}\n"));
        }
    }
    if let Some(d) = &m.description {
        let d = d.trim();
        if !d.is_empty() {
            out.push_str("\n\n");
            out.push_str(d);
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_annotations_markdown_empty() {
        let md = format_annotations_markdown("My \"Book\"", "Author", &[]);
        assert!(md.contains("title: \"My \\\"Book\\\"\""));
        assert!(md.contains("author: \"Author\""));
        assert!(md.contains("_No annotations yet._"));
    }

    #[test]
    fn test_format_annotations_markdown_with_items() {
        let items = vec![Annotation {
            section: 2,
            note: "Key insight".into(),
            quote: "Line 1\nLine 2".into(),
            kind: "highlight".into(),
            created: "1700000000".into(),
            color: "green".into(),
        }];
        let md = format_annotations_markdown("Test", "", &items);
        assert!(md.contains("*Section 3 · Highlight · Color: Green · Date: 1700000000*"));
        assert!(md.contains("> Line 1\n> Line 2"));
        assert!(md.contains("**Note:** Key insight"));
    }
}
