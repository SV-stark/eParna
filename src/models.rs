use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub fn default_reading_status() -> String {
    "reading".into()
}

pub fn default_highlight_color() -> String {
    "yellow".into()
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct RecentBook {
    pub path: String,
    pub title: String,
    pub section: usize,
    pub opened: String,
    #[serde(default)]
    pub favorite: bool,
    /// Reading status: "reading", "want", "finished"
    #[serde(default = "default_reading_status")]
    pub status: String,
    /// SHA-256 content hash from `Book::fingerprint()`, used to dedupe the library.
    #[serde(default)]
    pub content_hash: String,
    /// Readium CFI of the last read position, so resuming survives a re-parse.
    #[serde(default)]
    pub cfi: String,
    /// Denormalised from the book on open so the library grid can render
    /// without re-parsing every file.
    #[serde(default)]
    pub author: String,
    /// Lowercased file extension, shown as a badge.
    #[serde(default)]
    pub format: String,
    /// Total spine sections, for the per-book progress indicator.
    #[serde(default)]
    pub total: usize,
    /// Path to a cached cover image (for library thumbnails).
    #[serde(default)]
    pub cover: String,
    /// Accumulated reading time in seconds for this book.
    #[serde(default)]
    pub reading_time_secs: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub section: usize,
    /// For a note this is the note text; for a highlight it is empty.
    pub note: String,
    /// The passage the highlight covers (or that a note refers to).
    #[serde(default)]
    pub quote: String,
    /// "note" or "highlight". Older state files have no kind, so treat an
    /// empty value with a quote as a highlight and otherwise as a note.
    #[serde(default)]
    pub kind: String,
    pub created: String,
    /// Highlight color: "yellow", "green", "blue", "pink", "purple"
    #[serde(default = "default_highlight_color")]
    pub color: String,
}

impl Default for Annotation {
    fn default() -> Self {
        Self {
            section: 0,
            note: String::new(),
            quote: String::new(),
            kind: "highlight".into(),
            created: String::new(),
            color: "yellow".into(),
        }
    }
}

impl Annotation {
    pub fn is_highlight(&self) -> bool {
        match self.kind.as_str() {
            "highlight" => true,
            "note" => false,
            // Legacy entries: a non-empty quote means it was a highlight.
            _ => !self.quote.is_empty(),
        }
    }

    pub fn highlight_style(&self) -> &'static str {
        match self.color.as_str() {
            "green" => {
                "background-color: rgba(76, 175, 80, 0.40); border-radius: 2px; padding: 0 2px;"
            }
            "blue" => {
                "background-color: rgba(33, 150, 243, 0.35); border-radius: 2px; padding: 0 2px;"
            }
            "pink" => {
                "background-color: rgba(233, 30, 99, 0.35); border-radius: 2px; padding: 0 2px;"
            }
            "purple" => {
                "background-color: rgba(156, 39, 176, 0.35); border-radius: 2px; padding: 0 2px;"
            }
            _ => "background-color: rgba(255, 235, 59, 0.45); border-radius: 2px; padding: 0 2px;",
        }
    }

    pub fn color_badge(&self) -> &'static str {
        match self.color.as_str() {
            "green" => "[G]",
            "blue" => "[B]",
            "pink" => "[P]",
            "purple" => "[V]",
            _ => "[Y]",
        }
    }

    /// Short human label for the sidebar.
    pub fn summary(&self) -> String {
        if self.is_highlight() {
            let q: String = self.quote.chars().take(60).collect();
            let badge = self.color_badge();
            if self.note.is_empty() {
                format!("{badge} “{}”", q)
            } else {
                format!(
                    "{badge} “{}” — {}",
                    q,
                    self.note.chars().take(40).collect::<String>()
                )
            }
        } else {
            self.note.chars().take(60).collect()
        }
    }
}

fn default_view_mode() -> String {
    "grid".into()
}

pub fn default_wpm() -> u32 {
    220
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedState {
    pub recent: Vec<RecentBook>,
    pub annotations: HashMap<String, Vec<Annotation>>,
    /// Bookmarked spine sections per book path.
    #[serde(default)]
    pub bookmarks: HashMap<String, Vec<usize>>,
    pub font_size: f32,
    pub line_height: f32,
    pub margin: f32,
    pub max_width: f32,
    pub justify: bool,
    pub font_family: String,
    pub two_page: bool,
    pub theme: String,
    pub dark: bool,
    pub custom_bg: String,
    pub custom_fg: String,
    /// Estimated reading speed in Words Per Minute.
    #[serde(default = "default_wpm")]
    pub wpm: u32,
    /// Lifetime reading time across all books in seconds.
    #[serde(default)]
    pub total_reading_secs: u64,
    /// Library folders watched/scanned for ebooks.
    #[serde(default)]
    pub library_folders: Vec<String>,
    /// Library view presentation: "grid" or "list".
    #[serde(default = "default_view_mode")]
    pub library_view_mode: String,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            recent: Vec::new(),
            annotations: HashMap::new(),
            bookmarks: HashMap::new(),
            font_size: 16.0,
            line_height: 1.6,
            margin: 24.0,
            max_width: 720.0,
            justify: false,
            font_family: "sans-serif".into(),
            two_page: false,
            theme: "Light".into(),
            dark: false,
            custom_bg: "#ffffff".into(),
            custom_fg: "#202020".into(),
            wpm: 220,
            total_reading_secs: 0,
            library_folders: Vec::new(),
            library_view_mode: "grid".into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarMode {
    #[default]
    Contents,
    Bookmarks,
    Annotations,
    Search,
    Footnotes,
    Dictionary,
}

/// The collapsible bar shown under the primary toolbar.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    None,
    Typography,
    Actions,
}

/// A simple text or image modal rendered by hand.
pub struct Modal {
    pub title: String,
    pub body: String,
    /// Optional image (data: URI) to render in the modal body, e.g. for an
    /// image lightbox. When set, `body` is ignored.
    pub image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocItem {
    pub label: String,
    pub depth: usize,
    pub section_index: usize,
    pub fragment: Option<String>,
}

impl TocItem {
    pub fn select_label(&self) -> String {
        format!("{}{}", "  ".repeat(self.depth.min(6)), self.label)
    }
}

pub fn href_to_index(book: &ebook_rs::Book, href: &str) -> Option<usize> {
    let path = href.split('#').next().unwrap_or(href).trim();
    if path.is_empty() {
        return None;
    }
    book.sections().iter().position(|s| {
        s.full_path == path
            || s.href == path
            || s.full_path.ends_with(path)
            || (!s.href.is_empty() && path.ends_with(&s.href))
    })
}

pub fn build_toc_items(book: &ebook_rs::Book) -> Vec<TocItem> {
    let mut items = Vec::new();
    for flat in book.flatten_toc() {
        let Some(section_index) = href_to_index(book, &flat.full_path)
            .or_else(|| href_to_index(book, &flat.href))
        else {
            continue;
        };
        let label = flat.label.trim().to_string();
        if label.is_empty() {
            continue;
        }
        let fragment = flat
            .href
            .split_once('#')
            .or_else(|| flat.full_path.split_once('#'))
            .map(|(_, f)| f.trim().to_string())
            .filter(|f| !f.is_empty());
        items.push(TocItem {
            label,
            depth: flat.depth,
            section_index,
            fragment,
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotation_legacy_highlight_detection() {
        let legacy_hl = Annotation {
            section: 1,
            note: String::new(),
            quote: "To be or not to be".into(),
            kind: String::new(),
            created: "0".into(),
            color: "yellow".into(),
        };
        assert!(legacy_hl.is_highlight());
        assert!(legacy_hl.summary().contains("[Y]"));

        let legacy_note = Annotation {
            section: 1,
            note: "Remember this".into(),
            quote: String::new(),
            kind: String::new(),
            created: "0".into(),
            color: "blue".into(),
        };
        assert!(!legacy_note.is_highlight());
        assert_eq!(legacy_note.summary(), "Remember this");
    }

    #[test]
    fn test_persisted_state_defaults_on_partial_json() {
        let json =
            r#"{"theme":"Nord","recent":[{"path":"a.epub","title":"A","section":2,"opened":"1"}]}"#;
        let state: PersistedState = serde_json::from_str(json).expect("partial JSON should parse");
        assert_eq!(state.theme, "Nord");
        assert_eq!(state.wpm, 220);
        assert_eq!(state.library_view_mode, "grid");
        assert_eq!(state.recent.len(), 1);
        assert_eq!(state.recent[0].status, "reading");
    }

    #[test]
    fn test_href_to_index_handles_fragments_and_empty_paths() {
        let epub = ebook_rs::sample_builder::generate_sample_epub().expect("sample epub");
        let book = ebook_rs::Book::from_bytes(&epub).expect("parse sample epub");
        assert_eq!(href_to_index(&book, ""), None);
        assert_eq!(href_to_index(&book, "#only-fragment"), None);
        assert_eq!(href_to_index(&book, "ch1.xhtml"), Some(0));
        assert_eq!(href_to_index(&book, "ch1.xhtml#sub1"), Some(0));
        assert_eq!(href_to_index(&book, "OEBPS/ch2.xhtml#sec"), Some(1));
    }

    #[test]
    fn test_build_toc_items_preserves_subitems_and_depths() {
        let epub = ebook_rs::sample_builder::generate_sample_epub().expect("sample epub");
        let book = ebook_rs::Book::from_bytes(&epub).expect("parse sample epub");
        let items = build_toc_items(&book);
        assert!(!items.is_empty());
        assert_eq!(items[0].depth, 0);
        assert_eq!(items[0].section_index, 0);
    }
}
