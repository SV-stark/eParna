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
