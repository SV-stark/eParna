use crate::models::{Annotation, RecentBook};
use std::path::Path;

/// Sanitize a book title for use as a default export filename stem.
pub fn sanitize_filename(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = s.trim();
    if trimmed.is_empty() {
        "ebook".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Export `book` to the format implied by `path`'s extension using `ebook-rs` 0.20.0
/// multi-format codecs, WebPub manifest (`.webpub.json`), or RAG chunks JSON for `.json`.
pub fn export_book_bytes(
    book: &ebook_rs::Book,
    path: &Path,
) -> Result<Vec<u8>, ebook_rs::EbookError> {
    let lower_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if lower_name.ends_with(".kepub.epub") {
        return book.export_format_bytes("kepub");
    }
    if lower_name.ends_with(".webpub.json") {
        return book
            .to_webpub_json()
            .map(|s| s.into_bytes())
            .map_err(|e| ebook_rs::EbookError::Io(format!("Failed to export WebPub JSON: {e}")));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "json" {
        let chunks = book.to_rag_chunks(&ebook_rs::RagChunkConfig::default());
        return serde_json::to_vec_pretty(&chunks)
            .map_err(|e| ebook_rs::EbookError::Io(format!("Failed to serialize RAG JSON: {e}")));
    }
    book.export_format_bytes(&ext)
}

/// Check whether an export format identifier or compound extension is supported.
pub fn is_supported_output_format(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("kepub.epub")
        || ext.eq_ignore_ascii_case("json")
        || ext.eq_ignore_ascii_case("webpub.json")
        || ebook_rs::Book::supported_output_formats()
            .iter()
            .any(|f| f.eq_ignore_ascii_case(ext))
}

/// Export highlights, notes, and bookmarks as W3C Web Annotation JSON-LD.
pub fn format_annotations_w3c_json(
    book: &ebook_rs::Book,
    annotations: &[Annotation],
    bookmarks: &[usize],
) -> Result<String, String> {
    let mut mgr = ebook_rs::AnnotationManager::new();
    let cfi_for = |section: usize| -> String {
        book.to_readium_locator(section, 0)
            .ok()
            .and_then(|l| l.locations.cfi)
            .unwrap_or_else(|| format!("epubcfi(/6/{}!/4/2/1:0)", (section + 1) * 2))
    };
    for a in annotations {
        let hex = match a.color.as_str() {
            "green" => "#a5d6a7",
            "blue" => "#90caf9",
            "pink" => "#f48fb1",
            "purple" => "#ce93d8",
            _ => "#fff59d",
        };
        let cfi = cfi_for(a.section);
        let selected = (!a.quote.trim().is_empty()).then_some(a.quote.as_str());
        let note = (!a.note.trim().is_empty()).then_some(a.note.as_str());
        mgr.create_highlight(&cfi, hex, selected, note);
    }
    for &sec in bookmarks {
        let cfi = cfi_for(sec);
        mgr.create_bookmark(&cfi, Some(&format!("Section {}", sec + 1)));
    }
    let source_iri = book
        .metadata()
        .identifier
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            let hash = book.fingerprint().content_hash;
            (!hash.is_empty()).then(|| format!("urn:book:{hash}"))
        });
    if let Some(src) = source_iri {
        mgr.to_w3c_json_with_source(&src)
    } else {
        mgr.to_w3c_json()
    }
}

/// Run the lossless EPUB 3 optimizer on a clone of `book` and return the
/// optimized EPUB bytes alongside the [`ebook_rs::OptimizationReport`].
pub fn optimize_book_epub3(
    book: &ebook_rs::Book,
) -> Result<(Vec<u8>, ebook_rs::OptimizationReport), ebook_rs::EbookError> {
    let mut cloned = book.clone();
    let report =
        ebook_rs::EpubOptimizer::optimize(&mut cloned, &ebook_rs::EpubOptimizerOptions::default());
    let bytes = cloned.export_epub3_bytes()?;
    Ok((bytes, report))
}

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

    if book.locations().total_locations > 0 {
        out.push_str(&format!(
            "Readium locations: {}\n",
            book.locations().total_locations
        ));
    }
    let total_mins = book.book_reading_time_minutes(None);
    if total_mins > 0.0 {
        let rounded = total_mins.round() as u32;
        if rounded >= 60 {
            out.push_str(&format!(
                "Estimated reading time: ~{}h {}m\n",
                rounded / 60,
                rounded % 60
            ));
        } else {
            out.push_str(&format!(
                "Estimated reading time: ~{} min\n",
                rounded.max(1)
            ));
        }
    }

    if let Ok(sec) = book.get_section(current_section) {
        if let (Some(w), Some(h)) = (sec.viewport_width, sec.viewport_height) {
            out.push_str(&format!("Page size: {:.0} × {:.0} pt\n", w, h));
        }
        let analytics = ebook_rs::ReadingAnalytics::analyze_text(&sec.plain_text);
        if analytics.word_count > 0 {
            out.push_str(&format!(
                "Section analytics: {} words · ~{:.1} min · difficulty {:.1}/10\n",
                analytics.word_count, analytics.reading_time_minutes, analytics.difficulty_score
            ));
            if !analytics.top_keywords.is_empty() {
                let kws: Vec<&str> = analytics
                    .top_keywords
                    .iter()
                    .take(5)
                    .map(|(k, _)| k.as_str())
                    .collect();
                out.push_str(&format!("Top keywords: {}\n", kws.join(", ")));
            }
        }
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

    let a11y = &m.accessibility;
    if a11y.is_accessible
        || !a11y.access_modes.is_empty()
        || !a11y.accessibility_features.is_empty()
    {
        let mut parts = Vec::new();
        if a11y.is_screen_reader_friendly() {
            parts.push("screen-reader friendly".to_string());
        }
        if !a11y.access_modes.is_empty() {
            parts.push(format!("modes: {}", a11y.access_modes.join(", ")));
        }
        if !a11y.accessibility_features.is_empty() {
            parts.push(format!(
                "features: {}",
                a11y.accessibility_features.join(", ")
            ));
        }
        if !parts.is_empty() {
            out.push_str(&format!("Accessibility: {}\n", parts.join(" · ")));
        }
    }

    let fp = book.fingerprint();
    if !fp.content_hash.is_empty() {
        let short_hash: String = fp.content_hash.chars().take(16).collect();
        out.push_str(&format!("Content hash: {short_hash}…\n"));
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

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Hello: World/1?"), "Hello_ World_1_");
        assert_eq!(sanitize_filename("   "), "ebook");
    }

    #[test]
    fn test_export_book_bytes_multi_format_and_rag_json() {
        let epub = ebook_rs::sample_builder::generate_sample_epub().expect("sample epub");
        let book = ebook_rs::Book::from_bytes(&epub).expect("parse sample epub");

        for ext in [
            "epub",
            "kepub",
            "kepub.epub",
            "kfx",
            "mobi",
            "azw3",
            "pdb",
            "lit",
            "pdf",
            "fb2",
            "html",
            "htmlz",
            "txt",
            "txtz",
            "md",
            "oeb",
            "zip",
            "tcr",
            "docx",
            "odt",
            "rtf",
            "pmlz",
            "rb",
            "typ",
            "typst",
            "adoc",
            "org",
            "rst",
            "tex",
            "pptx",
            "json",
        ] {
            let path = Path::new("out").with_file_name(format!("book.{ext}"));
            let bytes = export_book_bytes(&book, &path)
                .unwrap_or_else(|e| panic!("export .{ext} failed: {e}"));
            assert!(!bytes.is_empty(), "exported .{ext} should not be empty");
        }

        // Verify round-trip parsing for 0.19.x input codecs (including 0.19.1 .lit)
        for ext in [
            "html", "htmlz", "txtz", "oeb", "pdb", "lit", "pmlz", "rb", "tcr", "pptx",
        ] {
            let path = Path::new("out").with_file_name(format!("book.{ext}"));
            let bytes = export_book_bytes(&book, &path).expect("export for round-trip");
            let reparsed = ebook_rs::Book::from_bytes(&bytes)
                .unwrap_or_else(|e| panic!("round-trip parse .{ext} failed: {e}"));
            assert!(
                !reparsed.sections().is_empty(),
                "round-trip .{ext} should contain at least one section"
            );
        }

        let bad = export_book_bytes(&book, Path::new("book.unknownfmt"));
        assert!(bad.is_err());
    }

    #[test]
    fn test_optimize_book_epub3_and_book_info() {
        let epub = ebook_rs::sample_builder::generate_sample_epub().expect("sample epub");
        let mut book = ebook_rs::Book::from_bytes(&epub).expect("parse sample epub");
        book.generate_locations(1000);

        let (opt_bytes, report) = optimize_book_epub3(&book).expect("optimize epub3");
        assert!(!opt_bytes.is_empty());
        assert!(report.original_size_bytes > 0);

        let info = format_book_info(&book, 0, false, None);
        assert!(info.contains("Readium locations:"));
        assert!(info.contains("Estimated reading time:"));
        assert!(info.contains("Section analytics:"));
        assert!(info.contains("Content hash:"));
    }

    #[test]
    fn test_w3c_annotations_webpub_and_supported_formats() {
        let epub = ebook_rs::sample_builder::generate_sample_epub().expect("sample epub");
        let book = ebook_rs::Book::from_bytes(&epub).expect("parse sample epub");

        for fmt in ebook_rs::Book::supported_output_formats() {
            assert!(
                is_supported_output_format(fmt),
                "expected supported_output_formats entry '{fmt}' to be recognized"
            );
        }
        assert!(is_supported_output_format("webpub.json"));
        assert!(!is_supported_output_format("unknownfmt"));

        let webpub =
            export_book_bytes(&book, Path::new("book.webpub.json")).expect("export .webpub.json");
        let webpub_str = std::str::from_utf8(&webpub).expect("utf-8 webpub");
        assert!(webpub_str.contains("readingOrder") || webpub_str.contains("metadata"));

        let items = vec![Annotation {
            section: 0,
            note: "Important".into(),
            quote: "Sample quote".into(),
            kind: "highlight".into(),
            created: "1700000000".into(),
            color: "blue".into(),
        }];
        let json = format_annotations_w3c_json(&book, &items, &[0]).expect("w3c json");
        assert!(json.contains("http://www.w3.org/ns/anno.jsonld"));
        assert!(json.contains("SpecificResource"));
        assert!(json.contains("\"source\""));
        assert!(json.contains("highlighting"));
        assert!(json.contains("bookmarking"));
    }
}
