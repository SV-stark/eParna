use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct RecentBook {
    pub path: String,
    pub title: String,
    pub section: usize,
    pub opened: String,
    #[serde(default)]
    pub favorite: bool,
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

#[derive(Clone, Serialize, Deserialize, Default)]
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

    /// Short human label for the sidebar.
    pub fn summary(&self) -> String {
        if self.is_highlight() {
            let q: String = self.quote.chars().take(60).collect();
            if self.note.is_empty() {
                format!("“{}”", q)
            } else {
                format!(
                    "“{}” — {}",
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedState {
    pub recent: Vec<RecentBook>,
    pub annotations: HashMap<String, Vec<Annotation>>,
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
