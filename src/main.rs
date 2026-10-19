use base64::Engine as _;
use ebook_rs::SearchResult;
use gpui_kit::base::{TextSelection, h_flex, v_flex};
use gpui_kit::component::{
    Root,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    progress::Progress,
    select::{Select, SelectEvent, SelectState},
    text::TextView,
};
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize, Default)]
struct RecentBook {
    path: String,
    title: String,
    section: usize,
    opened: String,
    #[serde(default)]
    favorite: bool,
    /// SHA-256 content hash from `Book::fingerprint()`, used to dedupe the library.
    #[serde(default)]
    content_hash: String,
    /// Readium CFI of the last read position, so resuming survives a re-parse.
    #[serde(default)]
    cfi: String,
    /// Denormalised from the book on open so the library grid can render
    /// without re-parsing every file.
    #[serde(default)]
    author: String,
    /// Lowercased file extension, shown as a badge.
    #[serde(default)]
    format: String,
    /// Total spine sections, for the per-book progress indicator.
    #[serde(default)]
    total: usize,
    /// Path to a cached cover image (for library thumbnails).
    #[serde(default)]
    cover: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct Annotation {
    section: usize,
    /// For a note this is the note text; for a highlight it is empty.
    note: String,
    /// The passage the highlight covers (or that a note refers to).
    #[serde(default)]
    quote: String,
    /// "note" or "highlight". Older state files have no kind, so treat an
    /// empty value with a quote as a highlight and otherwise as a note.
    #[serde(default)]
    kind: String,
    created: String,
}

impl Annotation {
    fn is_highlight(&self) -> bool {
        match self.kind.as_str() {
            "highlight" => true,
            "note" => false,
            // Legacy entries: a non-empty quote means it was a highlight.
            _ => !self.quote.is_empty(),
        }
    }

    /// Short human label for the sidebar.
    fn summary(&self) -> String {
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct PersistedState {
    recent: Vec<RecentBook>,
    annotations: HashMap<String, Vec<Annotation>>,
    font_size: f32,
    line_height: f32,
    margin: f32,
    max_width: f32,
    justify: bool,
    font_family: String,
    two_page: bool,
    theme: String,
    dark: bool,
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
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarMode {
    Contents,
    Bookmarks,
    Annotations,
    Search,
    Footnotes,
    Dictionary,
}

/// Resolve a TOC href (which may carry a `#fragment`) to a spine section index.
fn href_to_index(book: &ebook_rs::Book, href: &str) -> Option<usize> {
    let href = href.split('#').next().unwrap_or(href);
    book.sections()
        .iter()
        .position(|s| s.href == href || s.full_path.ends_with(href) || href.ends_with(&s.href))
}

/// Push a short notification without repeating the builder boilerplate.
fn notify_simple(window: &mut Window, cx: &mut Context<ReaderView>, title: &str, message: String) {
    use gpui_kit::component::WindowExt;
    window.push_notification(
        gpui_kit::component::notification::Notification::new()
            .title(title)
            .message(message),
        cx,
    );
}

struct ReaderView {
    state: PersistedState,
    config_path: PathBuf,
    show_library: bool,

    book: Option<ebook_rs::Book>,
    book_path: Option<PathBuf>,
    current: usize,
    html: String,

    toc_items: Vec<(String, usize)>,
    toc_state: Option<Entity<SelectState<Vec<String>>>>,

    sidebar_mode: SidebarMode,
    search_input: Entity<InputState>,
    goto_input: Entity<InputState>,
    search_results: Vec<SearchResult>,

    bookmarks: Vec<usize>,
    bookmark_state: Option<Entity<SelectState<Vec<String>>>>,

    annotations: Vec<Annotation>,
    annotation_input: Entity<InputState>,

    font_size: f32,
    line_height: f32,
    margin: f32,
    max_width: f32,
    justify: bool,
    font_family: String,
    two_page: bool,
    theme: String,
    dark: bool,

    library_search_input: Entity<InputState>,
    library_sort: usize,
    library_favorites_only: bool,
    opds_url_input: Entity<InputState>,
    opds_entries: Vec<(String, String)>,
    opds_status: String,

    dictionary_input: Entity<InputState>,
    dictionary_result: String,

    search_regex: bool,
    toc_search_input: Entity<InputState>,
    toc_matches: Vec<(String, String, usize)>,
    /// Cached cover images keyed by path, stored as `data:` URIs for thumbnails.
    covers: HashMap<String, String>,

    /// Length of the current text selection, for the status line.
    selection_status: String,

    /// Which secondary toolbar panel is open, if any. Mutually exclusive.
    panel: Panel,

    /// Modal overlay (book info, validation report, ...).
    modal: Option<Modal>,

    fullscreen: bool,
}

/// The collapsible bar shown under the primary toolbar.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Panel {
    #[default]
    None,
    Typography,
    Actions,
}

/// A simple text modal rendered by hand, so long reports (book info,
/// validation) get a scrollable surface instead of a squeezed toast.
struct Modal {
    title: String,
    body: String,
}

impl ReaderView {
    fn config_path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let dir = PathBuf::from(appdata).join("eParna");
            let _ = std::fs::create_dir_all(&dir);
            dir.join("state.json")
        } else {
            PathBuf::from("eParna_state.json")
        }
    }

    /// Location of the state file used before the app was renamed from ePustak,
    /// so existing libraries/settings can be migrated on first launch.
    fn legacy_config_path() -> Option<PathBuf> {
        std::env::var("APPDATA")
            .ok()
            .map(|appdata| PathBuf::from(appdata).join("ePustak").join("state.json"))
    }

    fn load_state() -> PersistedState {
        let p = Self::config_path();
        let raw = std::fs::read_to_string(&p)
            .ok()
            .or_else(|| Self::legacy_config_path().and_then(|lp| std::fs::read_to_string(lp).ok()));
        match raw {
            Some(s) => serde_json::from_str(&s).unwrap_or_default(),
            None => PersistedState::default(),
        }
    }

    fn save_state(&self) {
        let s = serde_json::to_string_pretty(&self.state).unwrap_or_default();
        let _ = std::fs::write(&self.config_path, s);
    }

    fn sync_state(&mut self) {
        self.state.font_size = self.font_size;
        self.state.line_height = self.line_height;
        self.state.margin = self.margin;
        self.state.max_width = self.max_width;
        self.state.justify = self.justify;
        self.state.font_family = self.font_family.clone();
        self.state.two_page = self.two_page;
        self.state.theme = self.theme.clone();
        self.state.dark = self.dark;
        self.save_state();
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.set_window_title("eParna");
        let st = Self::load_state();
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search in book..."));
        let goto_input = cx.new(|cx| InputState::new(window, cx).placeholder("Section #"));
        let opds_url_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("OPDS catalog URL"));
        let library_search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter library..."));
        let dictionary_input = cx.new(|cx| InputState::new(window, cx).placeholder("Word..."));
        let toc_search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Find in TOC..."));
        let annotation_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Add a note..."));
        let mut view = Self {
            font_size: st.font_size,
            line_height: st.line_height,
            margin: st.margin,
            max_width: st.max_width,
            justify: st.justify,
            font_family: st.font_family.clone(),
            two_page: st.two_page,
            theme: st.theme.clone(),
            dark: st.dark,
            state: st,
            config_path: Self::config_path(),
            show_library: true,
            book: None,
            book_path: None,
            current: 0,
            html: String::new(),
            toc_items: Vec::new(),
            toc_state: None,
            sidebar_mode: SidebarMode::Contents,
            search_input,
            goto_input,
            search_results: Vec::new(),
            bookmarks: Vec::new(),
            bookmark_state: None,
            annotations: Vec::new(),
            annotation_input,
            fullscreen: false,
            library_search_input,
            library_sort: 0,
            library_favorites_only: false,
            opds_url_input,
            opds_entries: Vec::new(),
            opds_status: String::new(),
            dictionary_input,
            dictionary_result: String::new(),
            search_regex: false,
            toc_search_input,
            toc_matches: Vec::new(),
            covers: HashMap::new(),
            selection_status: String::new(),
            panel: Panel::None,
            modal: None,
        };
        if view.show_library {
            view.load_covers();
        }
        if let Some(arg) = std::env::args().nth(1) {
            view.open_file(PathBuf::from(arg), window, cx);
        }
        view
    }

    fn covers_dir() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("eParna").join("covers")
        } else {
            PathBuf::from("eParna_covers")
        }
    }

    /// Small FNV-1a hash, used to name cached cover files after the book path.
    fn simple_hash(s: &str) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in s.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    fn mime_ext(mime: &str) -> &'static str {
        if mime.contains("jpeg") || mime.contains("jpg") {
            "jpg"
        } else if mime.contains("png") {
            "png"
        } else if mime.contains("gif") {
            "gif"
        } else if mime.contains("webp") {
            "webp"
        } else if mime.contains("bmp") {
            "bmp"
        } else {
            "img"
        }
    }

    /// Persist the book's cover image to the cache dir and return its path (empty if none).
    fn cache_cover(&self, book: &ebook_rs::Book, path: &str) -> String {
        let Some((bytes, mime)) = book.cover_image() else {
            return String::new();
        };
        if bytes.len() > 256 * 1024 {
            return String::new();
        }
        let dir = Self::covers_dir();
        if std::fs::create_dir_all(&dir).is_err() {
            return String::new();
        }
        let file = dir.join(format!(
            "{}.{}",
            Self::simple_hash(path),
            Self::mime_ext(mime)
        ));
        std::fs::write(&file, &bytes)
            .ok()
            .map(|_| file.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Load cached cover files into memory as data URIs for library thumbnails.
    fn load_covers(&mut self) {
        for r in &self.state.recent {
            if r.cover.is_empty() || self.covers.contains_key(&r.cover) {
                continue;
            }
            let p = PathBuf::from(&r.cover);
            let Ok(bytes) = std::fs::read(&p) else {
                continue;
            };
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("png");
            let mime = match ext {
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "bmp" => "image/bmp",
                _ => "image/png",
            };
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            self.covers
                .insert(r.cover.clone(), format!("data:{};base64,{}", mime, b64));
        }
    }

    fn theme_colors(&self) -> (Hsla, Hsla) {
        match self.theme.as_str() {
            "Dark" => (rgb(0x1e1e1e).into(), rgb(0xe0e0e0).into()),
            "Sepia" => (rgb(0xf4ecd8).into(), rgb(0x5b4636).into()),
            _ => (rgb(0xffffff).into(), rgb(0x202020).into()),
        }
    }

    fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match ebook_rs::Book::from_file(&path) {
            Ok(book) => {
                let title = book.metadata().title.clone();
                window.set_window_title(&format!("{} — eParna", title));
                // Denormalised for the library grid so it never re-parses a book
                // just to draw a row.
                let author = book.metadata().creator().to_string();
                let format = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let total = book.sections().len();
                let key = path.to_string_lossy().to_string();
                // Capture the saved position before the recents entry is rewritten.
                let saved = self
                    .state
                    .recent
                    .iter()
                    .find(|r| r.path == key)
                    .map(|r| (r.section, r.cfi.clone()));
                // Cache the cover for library thumbnails and fingerprint for dedupe.
                let cover = self.cache_cover(&book, &key);
                let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let content_hash = if file_size <= 32 * 1024 * 1024 {
                    book.fingerprint().content_hash
                } else {
                    String::new()
                };
                // Build TOC
                let mut items: Vec<(String, usize)> = Vec::new();
                for flat in book.flatten_toc() {
                    if let Some(i) = href_to_index(&book, &flat.href)
                        && !items.iter().any(|(_, j)| *j == i)
                    {
                        items.push((flat.label.clone(), i));
                    }
                }
                let labels: Vec<String> = items.iter().map(|(l, _)| l.clone()).collect();
                let toc_state = cx.new(|cx| SelectState::new(labels, None, window, cx));
                let toc_sub =
                    cx.subscribe(&toc_state, |view, _state, event: &SelectEvent<_>, cx| {
                        if let SelectEvent::Confirm(Some(label)) = event
                            && let Some((_, idx)) = view.toc_items.iter().find(|(l, _)| l == label)
                        {
                            view.current = *idx;
                            view.load_current(cx);
                            cx.notify();
                        }
                    });
                toc_sub.detach();

                // Load annotations & bookmarks for this book
                self.annotations = self
                    .state
                    .annotations
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                self.bookmarks.clear();
                self.refresh_bookmark_state(window, cx);

                // Recent books update (preserve favourite flag)
                let now = format!(
                    "{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                );
                let was_fav = self
                    .state
                    .recent
                    .iter()
                    .find(|r| r.path == key)
                    .map(|r| r.favorite)
                    .unwrap_or(false);
                // Dedupe by content: the same book opened from another path updates the
                // existing entry instead of adding a duplicate row.
                let mut merged = false;
                if !content_hash.is_empty()
                    && let Some(pos) = self.state.recent.iter().position(|r| {
                        !r.content_hash.is_empty()
                            && r.content_hash == content_hash
                            && r.path != key
                    })
                {
                    let mut entry = self.state.recent.remove(pos);
                    entry.path = key.clone();
                    entry.title = title.clone();
                    entry.section = 0;
                    entry.opened = now.clone();
                    entry.cover = cover.clone();
                    entry.content_hash = content_hash.clone();
                    entry.author = author.clone();
                    entry.format = format.clone();
                    entry.total = total;
                    self.state.recent.insert(0, entry);
                    merged = true;
                }
                if !merged {
                    self.state.recent.retain(|r| r.path != key);
                    self.state.recent.insert(
                        0,
                        RecentBook {
                            path: key.clone(),
                            title: title.clone(),
                            section: 0,
                            opened: now,
                            favorite: was_fav,
                            content_hash,
                            cfi: String::new(),
                            author: author.clone(),
                            format: format.clone(),
                            total,
                            cover,
                        },
                    );
                }
                if self.state.recent.len() > 50 {
                    self.state.recent.truncate(50);
                }
                self.save_state();

                self.toc_items = items;
                self.toc_state = Some(toc_state);
                self.book = Some(book);
                self.book_path = Some(path);
                self.search_results.clear();
                self.show_library = false;
                // Resume: prefer the CFI (survives a re-parse / re-converted
                // copy), fall back to the raw section index.
                let len = self.book.as_ref().map(|b| b.sections().len()).unwrap_or(0);
                let idx = saved
                    .as_ref()
                    .and_then(|(_, c)| self.section_from_cfi(c))
                    .or_else(|| saved.as_ref().map(|(i, _)| *i))
                    .unwrap_or(0);
                self.current = idx.min(len.saturating_sub(1));
                self.load_current(cx);
            }
            Err(e) => {
                let _ = e;
            }
        }
        cx.notify();
    }

    fn load_current(&mut self, _cx: &mut Context<Self>) {
        if let Some(book) = &self.book {
            match book.get_section(self.current) {
                Ok(sec) => {
                    self.html = sec.processed_html.clone();
                }
                Err(e) => {
                    self.html = format!("<p>Error: {}</p>", e);
                }
            }
        }
        // persist progress — store a Readium CFI alongside the section index so
        // resume survives a re-parse or a re-converted copy of the book.
        if let Some(path) = &self.book_path {
            let key = path.to_string_lossy().to_string();
            let cfi = self
                .book
                .as_ref()
                .and_then(|b| b.to_readium_locator(self.current, 0).ok())
                .and_then(|l| l.locations.cfi);
            if let Some(r) = self.state.recent.iter_mut().find(|r| r.path == key) {
                r.section = self.current;
                if let Some(cfi) = cfi {
                    r.cfi = cfi;
                }
            }
            self.save_state();
        }
    }

    /// Resolve a stored CFI back to a section index for this book.
    fn section_from_cfi(&self, cfi: &str) -> Option<usize> {
        let book = self.book.as_ref()?;
        if cfi.trim().is_empty() {
            return None;
        }
        // Ignore stale CFIs: if the book changed shape, fall back to the index.
        book.get_section_by_cfi(cfi)
            .ok()
            .map(|sec| sec.index)
            .filter(|i| *i < book.sections().len())
    }

    fn next(&mut self, cx: &mut Context<Self>) {
        if let Some(book) = &self.book
            && self.current + 1 < book.sections().len()
        {
            self.current += 1;
            self.load_current(cx);
            cx.notify();
        }
    }
    fn prev(&mut self, cx: &mut Context<Self>) {
        if self.current > 0 {
            self.current -= 1;
            self.load_current(cx);
            cx.notify();
        }
    }
    fn jump(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(book) = &self.book
            && idx < book.sections().len()
        {
            self.current = idx;
            self.load_current(cx);
            cx.notify();
        }
    }

    fn do_search(&mut self, cx: &mut Context<Self>) {
        let q = self.search_input.read(cx).value().to_string();
        if q.trim().is_empty() {
            return;
        }
        if let Some(book) = &self.book {
            self.search_results = if self.search_regex {
                book.search_regex_limited(&q, Some(500)).unwrap_or_default()
            } else {
                book.search_limited(&q, Some(500))
            };
            if let Some(r) = self.search_results.first() {
                self.current = r.spine_index;
                self.load_current(cx);
            }
            cx.notify();
        }
    }

    fn toggle_bookmark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.book.is_none() {
            return;
        }
        if let Some(pos) = self.bookmarks.iter().position(|&i| i == self.current) {
            self.bookmarks.remove(pos);
        } else {
            self.bookmarks.push(self.current);
            self.bookmarks.sort();
        }
        self.refresh_bookmark_state(window, cx);
        cx.notify();
    }

    fn refresh_bookmark_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let labels: Vec<String> = self
            .bookmarks
            .iter()
            .map(|&i| format!("Section {}", i + 1))
            .collect();
        if labels.is_empty() {
            self.bookmark_state = None;
        } else {
            let st = cx.new(|cx| SelectState::new(labels, None, window, cx));
            let sub = cx.subscribe(&st, |view, _state, event: &SelectEvent<_>, cx| {
                if let SelectEvent::Confirm(Some(label)) = event
                    && let Some(num) = label
                        .strip_prefix("Section ")
                        .and_then(|s| s.parse::<usize>().ok())
                {
                    view.jump(num - 1, cx);
                }
            });
            sub.detach();
            self.bookmark_state = Some(st);
        }
    }

    fn now_secs() -> String {
        format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        )
    }

    /// Append an annotation and persist the whole list for this book.
    fn push_annotation(&mut self, ann: Annotation, cx: &mut Context<Self>) {
        self.annotations.push(ann);
        if let Some(path) = &self.book_path {
            let key = path.to_string_lossy().to_string();
            self.state.annotations.insert(key, self.annotations.clone());
            self.save_state();
        }
        cx.notify();
    }

    /// The text currently selected in the reading view, trimmed.
    fn selected_text(window: &mut Window, cx: &mut App) -> String {
        TextSelection::selected_text(window, cx).trim().to_string()
    }

    /// Copy the current selection to the clipboard (`Ctrl+C`).
    fn copy_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = Self::selected_text(window, cx);
        if text.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.selection_status = format!("Copied {} characters", text.chars().count());
        cx.notify();
    }

    /// Turn the current selection into a highlight anchored to this section.
    fn highlight_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.book.is_none() {
            return;
        }
        let quote = Self::selected_text(window, cx);
        if quote.is_empty() {
            self.selection_status = "Select some text first".into();
            cx.notify();
            return;
        }
        let chars = quote.chars().count();
        self.push_annotation(
            Annotation {
                section: self.current,
                note: String::new(),
                quote,
                kind: "highlight".into(),
                created: Self::now_secs(),
            },
            cx,
        );
        self.selection_status = format!("Highlighted {} characters", chars);
    }

    /// Add a note, quoting the current selection if there is one.
    fn add_annotation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.book.is_none() {
            return;
        }
        let note = self.annotation_input.read(cx).value().to_string();
        if note.trim().is_empty() {
            return;
        }
        let quote = Self::selected_text(window, cx);
        self.push_annotation(
            Annotation {
                section: self.current,
                note,
                quote,
                kind: "note".into(),
                created: Self::now_secs(),
            },
            cx,
        );
    }

    fn change_font(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.font_size = (self.font_size + delta).clamp(10.0, 40.0);
        self.sync_state();
        cx.notify();
    }

    fn go_library(&mut self, cx: &mut Context<Self>) {
        self.show_library = true;
        self.load_covers();
        // save progress
        self.sync_state();
        cx.notify();
    }

    fn render_library(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
        bg: Hsla,
        fg: Hsla,
    ) -> AnyElement {
        let on_open = cx.listener(|view, _ev, window, cx| {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Ebook",
                    &[
                        "epub", "mobi", "azw3", "fb2", "pdf", "txt", "md", "docx", "odt", "rtf",
                        "cbz", "lit", "kfx", "kepub",
                    ],
                )
                .pick_file()
            {
                view.open_file(path, window, cx);
            }
        });
        let sort_label = match self.library_sort {
            0 => "Sort: Recent",
            1 => "Sort: Title",
            _ => "Sort: Path",
        };
        let on_sort = cx.listener(|view, _ev, _w, cx| {
            view.library_sort = (view.library_sort + 1) % 3;
            cx.notify();
        });
        let fav_label = if self.library_favorites_only {
            "Favorites: on"
        } else {
            "Favorites: off"
        };
        let on_fav_toggle = cx.listener(|view, _ev, _w, cx| {
            view.library_favorites_only = !view.library_favorites_only;
            cx.notify();
        });
        let mut v = v_flex()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .p_4()
            .gap_4()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_size(px(24.0)).child("eParna Library"))
                    .child(Button::new("openlib").label("Open file").on_click(on_open))
                    .child(Button::new("sortlib").label(sort_label).on_click(on_sort))
                    .child(
                        Button::new("favlib")
                            .label(fav_label)
                            .on_click(on_fav_toggle),
                    ),
            );
        // filter
        v = v.child(Input::new(&self.library_search_input));
        let q = self.library_search_input.read(cx).value().to_lowercase();
        // recent books (sorted)
        let mut recent = self.state.recent.clone();
        match self.library_sort {
            1 => recent.sort_by_key(|a| a.title.to_lowercase()),
            2 => recent.sort_by_key(|a| a.path.to_lowercase()),
            _ => {}
        }
        // ---- continue reading ------------------------------------------------
        // Books actually in progress get a wide tile up top, so resuming takes
        // one click instead of a hunt through the grid.
        let mut in_progress: Vec<&RecentBook> = recent
            .iter()
            .filter(|r| r.total > 0 && r.section > 0 && r.section + 1 < r.total)
            .collect();
        in_progress.sort_by_key(|r| std::cmp::Reverse(r.opened.clone()));
        if !in_progress.is_empty() {
            v = v.child(section_heading("Continue reading"));
            let mut row = h_flex().gap_3().flex_wrap();
            for (i, r) in in_progress.iter().take(4).enumerate() {
                let id = format!("cont{i}");
                // `open_file` resolves the stored CFI itself, so don't clobber
                // the position afterwards.
                let on_open = {
                    let p = r.path.clone();
                    cx.listener(move |view, _ev, window, cx| {
                        view.open_file(PathBuf::from(p.clone()), window, cx);
                    })
                };
                let on_fav = {
                    let p = r.path.clone();
                    cx.listener(move |view, _ev, _w, cx| view.toggle_favorite(p.clone(), cx))
                };
                row = row.child(wide_tile(
                    id.clone(),
                    cover_for(&self.covers, r),
                    book_title(r),
                    book_subtitle(r),
                    book_pct(r),
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(
                            Button::new(format!("{id}-open"))
                                .label("Resume")
                                .on_click(on_open),
                        )
                        .child(
                            Button::new(format!("{id}-fav"))
                                .ghost()
                                .label(star_label(r.favorite))
                                .on_click(on_fav),
                        )
                        .into_any_element(),
                ));
            }
            v = v.child(row);
        }

        // ---- grid ------------------------------------------------------------
        v = v.child(section_heading(&format!("All books ({})", recent.len())));
        let mut grid = h_flex().gap_3().flex_wrap();
        let mut shown = 0usize;
        for (i, r) in recent.iter().enumerate() {
            if self.library_favorites_only && !r.favorite {
                continue;
            }
            if !q.is_empty()
                && !r.title.to_lowercase().contains(&q)
                && !r.author.to_lowercase().contains(&q)
                && !r.path.to_lowercase().contains(&q)
            {
                continue;
            }
            shown += 1;
            let id = format!("bk{i}");
            let pct = book_pct(r);
            let on_open = {
                let p = r.path.clone();
                cx.listener(move |view, _ev, window, cx| {
                    view.open_file(PathBuf::from(p.clone()), window, cx);
                })
            };
            let on_remove = {
                let p = r.path.clone();
                cx.listener(move |view, _ev, _w, cx| view.remove_recent(p.clone(), cx))
            };
            let on_fav = {
                let p = r.path.clone();
                cx.listener(move |view, _ev, _w, cx| view.toggle_favorite(p.clone(), cx))
            };
            grid = grid.child(grid_tile(
                id.clone(),
                cover_for(&self.covers, r),
                book_title(r),
                book_subtitle(r),
                pct,
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new(format!("{id}-open"))
                            .label(if pct > 0.0 && pct < 100.0 {
                                "Continue"
                            } else {
                                "Open"
                            })
                            .on_click(on_open),
                    )
                    .child(
                        Button::new(format!("{id}-fav"))
                            .ghost()
                            .label(star_label(r.favorite))
                            .on_click(on_fav),
                    )
                    .child(
                        Button::new(format!("{id}-rm"))
                            .ghost()
                            .label("\u{2715}")
                            .on_click(on_remove),
                    )
                    .into_any_element(),
            ));
        }
        v = v.child(grid);
        if shown == 0 {
            v = v.child(
                div()
                    .text_size(px(13.0))
                    .child(if self.state.recent.is_empty() {
                        "No books yet \u{2014} open a file to get started."
                    } else {
                        "Nothing matches the current filter."
                    }),
            );
        }
        // OPDS
        v = v.child(div().text_size(px(16.0)).child("OPDS catalog"));
        v = v.child(Input::new(&self.opds_url_input));
        let on_fetch = cx.listener(|view, _ev, _w, cx| {
            let url = view.opds_url_input.read(cx).value().to_string();
            if url.trim().is_empty() {
                return;
            }
            match ureq::get(&url).call() {
                Ok(resp) => {
                    let body = resp.into_body().read_to_string().unwrap_or_default();
                    let feed = if body.trim_start().starts_with('{') {
                        ebook_rs::OpdsFeed::parse_json(&body)
                    } else {
                        ebook_rs::OpdsFeed::parse_atom_xml(&body)
                    };
                    match feed {
                        Ok(f) => {
                            view.opds_entries = f
                                .entries
                                .iter()
                                .map(|e| {
                                    (
                                        e.title.clone(),
                                        e.download_link(None)
                                            .map(|l| l.href.clone())
                                            .unwrap_or_default(),
                                    )
                                })
                                .collect();
                            view.opds_status = format!("{} entries", view.opds_entries.len());
                        }
                        Err(e) => {
                            view.opds_status = format!("Parse error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    view.opds_status = format!("Fetch error: {}", e);
                }
            }
            cx.notify();
        });
        v = v.child(Button::new("fetchopds").label("Fetch").on_click(on_fetch));
        v = v.child(div().text_size(px(12.0)).child(self.opds_status.clone()));
        for (title, href) in self.opds_entries.clone() {
            let href2 = href.clone();
            let on_open_entry = cx.listener(move |view, _ev, window, cx| {
                if href2.is_empty() {
                    return;
                }
                // download
                if let Ok(resp) = ureq::get(&href2).call() {
                    let mut bytes = Vec::new();
                    let _ = resp.into_body().as_reader().read_to_end(&mut bytes);
                    let tmp = std::env::temp_dir().join("opds_book.epub");
                    if std::fs::write(&tmp, &bytes).is_ok() {
                        view.open_file(tmp, window, cx);
                    }
                }
            });
            v = v.child(
                Button::new(format!("opds{}", title))
                    .ghost()
                    .label(title)
                    .on_click(on_open_entry),
            );
        }

        v.into_any_element()
    }

    fn remove_recent(&mut self, path: String, cx: &mut Context<Self>) {
        self.state.recent.retain(|r| r.path != path);
        self.save_state();
        cx.notify();
    }

    fn toggle_favorite(&mut self, path: String, cx: &mut Context<Self>) {
        if let Some(r) = self.state.recent.iter_mut().find(|r| r.path == path) {
            r.favorite = !r.favorite;
        }
        self.save_state();
        cx.notify();
    }

    fn toggle_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.toggle_fullscreen();
        self.fullscreen = window.is_fullscreen();
        cx.notify();
    }

    // ---- secondary-panel helpers -----------------------------------------

    fn show_modal(&mut self, title: &str, body: String, cx: &mut Context<Self>) {
        self.modal = Some(Modal {
            title: title.into(),
            body,
        });
        self.panel = Panel::None;
        cx.notify();
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        cx.notify();
    }

    /// Book metadata, formatted for a readable modal rather than a toast.
    fn book_info_body(&self) -> Option<String> {
        let book = self.book.as_ref()?;
        let m = book.metadata();
        let mut out = format!("{}\n", m.title);
        if !m.creators.is_empty() {
            out.push_str(&format!("{}\n", m.creators.join(", ")));
        }
        if let Some(p) = m.publisher() {
            out.push_str(&format!("{}\n", p));
        }
        out.push_str(&format!("{} sections\n", book.sections().len()));
        out.push('\n');
        for (label, value) in [
            ("Language", Some(m.language().to_string())),
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
        Some(out.trim_end().to_string())
    }

    /// Structural validation report, formatted for the modal.
    fn validation_body(&self) -> Option<String> {
        let report = self.book.as_ref()?.validate();
        let mut out = format!(
            "{} · {} error(s), {} warning(s), {} note(s)\n",
            if report.is_valid { "Valid" } else { "Invalid" },
            report.errors_count,
            report.warnings_count,
            report.info_count
        );
        if report.errors.is_empty() {
            out.push_str("\nNo structural problems found.");
            return Some(out);
        }
        for e in report.errors.iter().take(50) {
            out.push_str(&format!(
                "\n[{:?}] {} — {}\n",
                e.severity, e.code, e.message
            ));
        }
        Some(out)
    }

    fn adjust_font_size(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.font_size = (self.font_size + delta).clamp(10.0, 40.0);
        self.sync_state();
        cx.notify();
    }
    fn adjust_line_height(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.line_height = (self.line_height + delta).clamp(1.0, 2.5);
        self.sync_state();
        cx.notify();
    }
    fn adjust_margin(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.margin = (self.margin + delta).clamp(0.0, 96.0);
        self.sync_state();
        cx.notify();
    }
    fn adjust_width(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.max_width = (self.max_width + delta).clamp(400.0, 1400.0);
        self.sync_state();
        cx.notify();
    }
    fn cycle_font(&mut self, cx: &mut Context<Self>) {
        const FONTS: [&str; 4] = ["serif", "sans-serif", "monospace", "system-ui"];
        let i = FONTS
            .iter()
            .position(|f| *f == self.font_family)
            .unwrap_or(0);
        self.font_family = FONTS[(i + 1) % FONTS.len()].to_string();
        self.sync_state();
        cx.notify();
    }
    fn cycle_theme(&mut self, cx: &mut Context<Self>) {
        self.theme = if self.theme == "Light" {
            "Dark".into()
        } else if self.theme == "Dark" {
            "Sepia".into()
        } else {
            "Light".into()
        };
        self.dark = self.theme == "Dark";
        self.sync_state();
        cx.notify();
    }
    fn toggle_two_page(&mut self, cx: &mut Context<Self>) {
        self.two_page = !self.two_page;
        self.sync_state();
        cx.notify();
    }
    /// Back to the defaults, so a reader who has fiddled can escape.
    fn reset_display(&mut self, cx: &mut Context<Self>) {
        let d = PersistedState::default();
        self.font_size = d.font_size;
        self.line_height = d.line_height;
        self.margin = d.margin;
        self.max_width = d.max_width;
        self.font_family = d.font_family;
        self.theme = d.theme;
        self.dark = d.dark;
        self.two_page = d.two_page;
        // `justify` has no effect: gpui has no TextAlign::Justify, so it is
        // deliberately left alone rather than pretending to work.
        self.sync_state();
        cx.notify();
    }

    /// Convert the open book to EPUB 3 and write it where the user chooses.
    fn save_as_epub(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = &self.book else {
            return;
        };
        match book.export_epub3_bytes() {
            Ok(bytes) => {
                let name = format!(
                    "{}.epub",
                    book.metadata()
                        .title
                        .chars()
                        .map(|c| {
                            if c.is_alphanumeric() || c == ' ' || c == '-' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect::<String>()
                );
                let chosen = rfd::FileDialog::new()
                    .set_file_name(&name)
                    .add_filter("EPUB", &["epub"])
                    .save_file();
                if let Some(path) = chosen {
                    let n = bytes.len();
                    let ok = std::fs::write(&path, &bytes).is_ok();
                    notify_simple(
                        window,
                        cx,
                        if ok { "Saved EPUB" } else { "Write failed" },
                        format!("{} ({n} bytes)", path.display()),
                    );
                }
            }
            Err(e) => notify_simple(window, cx, "Export failed", e),
        }
    }

    /// Write this book's annotations to `annotations.md`.
    fn export_annotations(&mut self) {
        let title = self
            .book
            .as_ref()
            .map(|b| b.metadata().title.clone())
            .unwrap_or_else(|| "Untitled".into());
        let mut out = format!("# Notes — {title}\n\n");
        if self.annotations.is_empty() {
            out.push_str("_No annotations yet._\n");
        }
        for (i, a) in self.annotations.iter().enumerate() {
            out.push_str(&format!("## {}\n\n", i + 1));
            out.push_str(&format!(
                "*Section {} · {} · {}*\n\n",
                a.section + 1,
                if a.is_highlight() {
                    "Highlight"
                } else {
                    "Note"
                },
                a.created
            ));
            if !a.quote.trim().is_empty() {
                // Prefix every line so multi-line quotes stay quoted.
                for line in a.quote.trim().lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push('\n');
            }
            if !a.note.trim().is_empty() {
                out.push_str(&a.note);
                out.push_str("\n\n");
            }
        }
        let _ = std::fs::write("annotations.md", out);
    }
}

/// Pin unsized illustrations to the reading column.
///
/// The HTML renderer honours `width`/`height` attributes and definite lengths,
/// but has no `max-width`, and a percentage width resolves against the text
/// flow rather than the page. EPUBs routinely ship a cover larger than any
/// window, which would otherwise blow the whole layout out sideways and push
/// the toolbar off-screen. So every image that does not declare its own size
/// is given the column width in pixels.
fn constrain_images(html: &str, column_px: f32) -> String {
    if !html.contains("<img") {
        return html.to_string();
    }
    let column = column_px.max(120.0).round() as i64;
    let mut out = String::with_capacity(html.len() + 64);
    let mut rest = html;
    while let Some(pos) = rest.find("<img") {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let end = tail.find('>').map(|i| i + 1).unwrap_or(tail.len());
        let tag = &tail[..end];
        // Leave anything the book already sized (or styled) alone.
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

/// Escape plain text for rendering inside the HTML `TextView`, keeping the
/// line breaks the reader expects.
fn escape_html(s: &str) -> String {
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

/// Read progress for a library entry, as a 0-100 percentage. Mirrors the
/// reader's own progress so the library and the status line never disagree.
fn book_pct(r: &RecentBook) -> f32 {
    if r.total == 0 {
        0.0
    } else {
        ((r.section + 1) as f32 / r.total as f32 * 100.0).round()
    }
}

fn book_title(r: &RecentBook) -> String {
    if r.title.is_empty() {
        r.path.clone()
    } else {
        r.title.clone()
    }
}

fn book_subtitle(r: &RecentBook) -> String {
    match (r.author.is_empty(), r.format.is_empty()) {
        (false, false) => format!("{} \u{b7} {}", r.author, r.format),
        (false, true) => r.author.clone(),
        (true, false) => r.format.clone(),
        (true, true) => String::new(),
    }
}

fn star_label(favorite: bool) -> &'static str {
    if favorite { "\u{2605}" } else { "\u{2606}" }
}

/// The cached cover for an entry, if one was extracted when it was opened.
fn cover_for(covers: &HashMap<String, String>, r: &RecentBook) -> Option<String> {
    if r.cover.is_empty() {
        None
    } else {
        covers.get(&r.cover).cloned()
    }
}

fn section_heading(text: &str) -> AnyElement {
    div()
        .text_size(px(16.0))
        .child(text.to_string())
        .into_any_element()
}

/// Shorten to `max` characters, ending with an ellipsis so the reader can see
/// the title was trimmed rather than silently misread.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// A book cover, or a neutral placeholder so tiles keep a consistent height.
fn cover_art(id: String, cover: Option<String>, w: f32, h: f32) -> AnyElement {
    match cover {
        Some(uri) => div()
            .w(px(w))
            .h(px(h))
            .overflow_hidden()
            .child(TextView::html(
                id,
                format!("<img src=\"{uri}\" width=\"{w}\" height=\"{h}\"/>"),
            ))
            .into_any_element(),
        None => v_flex()
            .w(px(w))
            .h(px(h))
            .items_center()
            .justify_center()
            .border_1()
            .child(div().text_size(px(24.0)).child("\u{1F4D6}"))
            .into_any_element(),
    }
}

/// Vertical library tile: cover above metadata. Used for the main grid.
fn grid_tile(
    id: String,
    cover: Option<String>,
    title: String,
    subtitle: String,
    pct: f32,
    actions: AnyElement,
) -> AnyElement {
    v_flex()
        .w(px(156.0))
        .gap_1()
        .p_2()
        .border_1()
        .child(cover_art(format!("{id}-c"), cover, 136.0, 190.0))
        .child(div().text_size(px(13.0)).child(truncate(&title, 34)))
        .child(div().text_size(px(11.0)).child(truncate(&subtitle, 40)))
        .child(Progress::new(format!("{id}-p")).value(pct))
        .child(actions)
        .into_any_element()
}

/// Wide library tile: cover beside metadata. Used for "Continue reading".
fn wide_tile(
    id: String,
    cover: Option<String>,
    title: String,
    subtitle: String,
    pct: f32,
    actions: AnyElement,
) -> AnyElement {
    h_flex()
        .w(px(340.0))
        .gap_3()
        .p_2()
        .border_1()
        .child(cover_art(format!("{id}-c"), cover, 72.0, 100.0))
        .child(
            v_flex()
                .flex_1()
                .gap_1()
                .child(div().text_size(px(13.0)).child(truncate(&title, 40)))
                .child(div().text_size(px(11.0)).child(truncate(&subtitle, 48)))
                .child(Progress::new(format!("{id}-p")).value(pct))
                .child(actions),
        )
        .into_any_element()
}

/// A labelled setting whose value is stepped through a fixed list.
fn cycle_row(
    label: &str,
    value: &str,
    id: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    h_flex()
        .gap_3()
        .items_center()
        .child(
            div()
                .w(px(96.0))
                .text_size(px(12.0))
                .child(label.to_string()),
        )
        .child(
            div()
                .w(px(88.0))
                .text_size(px(12.0))
                .child(value.to_string()),
        )
        .child(
            Button::new(format!("{id}-cycle"))
                .ghost()
                .label("Change")
                .on_click(on_click),
        )
        .into_any_element()
}

/// A labelled "− value +" stepper row for the typography panel.
fn setting_row(label: &str, value: String, minus: AnyElement, plus: AnyElement) -> AnyElement {
    h_flex()
        .gap_3()
        .items_center()
        .child(
            div()
                .w(px(96.0))
                .text_size(px(12.0))
                .child(label.to_string()),
        )
        .child(
            h_flex()
                .gap_1()
                .items_center()
                .child(minus)
                .child(
                    div()
                        .w(px(64.0))
                        .text_size(px(12.0))
                        .text_align(TextAlign::Center)
                        .child(value),
                )
                .child(plus),
        )
        .into_any_element()
}

impl Render for ReaderView {
    #[allow(refining_impl_trait)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (bg, fg) = self.theme_colors();
        if self.show_library {
            return self.render_library(window, cx, bg, fg);
        }

        let status = if let Some(b) = &self.book {
            let m = b.metadata();
            let mut parts = vec![m.title.clone()];
            if !m.creators.is_empty() {
                parts.push(m.creators.join(", "));
            }
            parts.push(format!(
                "{}/{} sections",
                self.current + 1,
                b.sections().len()
            ));
            parts.join("  ·  ")
        } else {
            "No book".into()
        };
        let font_size = self.font_size;
        let margin = self.margin;
        let max_width = self.max_width;
        let line_height = self.line_height;
        // Reading column in pixels: the page's own width, minus its side
        // margins. Used to size illustrations that the book left unsized.
        let column_px = (max_width - 2.0 * margin).max(160.0);
        let html = constrain_images(&self.html, column_px);
        let pos = if let Some(b) = &self.book {
            format!("{}/{}", self.current + 1, b.sections().len())
        } else {
            String::new()
        };
        let progress = if let Some(b) = &self.book {
            if b.sections().is_empty() {
                0.0
            } else {
                (self.current as f32 + 1.0) / b.sections().len() as f32 * 100.0
            }
        } else {
            0.0
        };

        let on_open = cx.listener(|view, _ev, window, cx| {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Ebook",
                    &[
                        "epub", "mobi", "azw3", "fb2", "pdf", "txt", "md", "docx", "odt", "rtf",
                        "cbz", "lit", "kfx", "kepub",
                    ],
                )
                .pick_file()
            {
                view.open_file(path, window, cx);
            }
        });
        let on_prev = cx.listener(|view, _ev, _w, cx| view.prev(cx));
        let on_next = cx.listener(|view, _ev, _w, cx| view.next(cx));
        let on_search = cx.listener(|view, _ev, _w, cx| view.do_search(cx));
        let on_bm = cx.listener(|view, _ev, window, cx| view.toggle_bookmark(window, cx));
        let on_add_note = cx.listener(|view, _ev, window, cx| view.add_annotation(window, cx));
        let on_highlight =
            cx.listener(|view, _ev, window, cx| view.highlight_selection(window, cx));
        let on_copy = cx.listener(|view, _ev, window, cx| view.copy_selection(window, cx));
        let on_lib = cx.listener(|view, _ev, window, cx| {
            window.set_window_title("eParna");
            view.go_library(cx);
        });
        let on_fs = cx.listener(|view, _ev, window, cx| view.toggle_fullscreen(window, cx));

        // sidebar content
        let sidebar = match self.sidebar_mode {
            SidebarMode::Contents => {
                let on_toc_search = cx.listener(|view, _, _, cx| {
                    let q = view.toc_search_input.read(cx).value().to_string();
                    if q.trim().is_empty() {
                        view.toc_matches.clear();
                    } else if let Some(book) = &view.book {
                        view.toc_matches = book
                            .search_toc(&q)
                            .into_iter()
                            .filter_map(|m| {
                                let idx = href_to_index(book, &m.href)?;
                                Some((m.label, m.breadcrumb, idx))
                            })
                            .collect();
                    }
                    cx.notify();
                });
                let mut v = v_flex().gap_2().p_2();
                v = v.child(Input::new(&self.toc_search_input));
                v = v.child(
                    Button::new("tocsearch")
                        .label("Find in TOC")
                        .on_click(on_toc_search),
                );
                if !self.toc_matches.is_empty() {
                    v = v.child(
                        div()
                            .text_size(px(12.0))
                            .child(format!("{} matches", self.toc_matches.len())),
                    );
                    for (i, (label, crumb, idx)) in self.toc_matches.iter().enumerate() {
                        if i >= 30 {
                            break;
                        }
                        let idx = *idx;
                        let text = format!("{} — {}", label, crumb);
                        let on_jump = cx.listener(move |view, _, _, cx| view.jump(idx, cx));
                        v = v.child(
                            Button::new(format!("tocm{}", i))
                                .ghost()
                                .label(text)
                                .on_click(on_jump),
                        );
                    }
                } else if let Some(ts) = &self.toc_state {
                    v = v.child(Select::new(ts).placeholder("Contents"));
                    v = v.child(
                        div()
                            .text_size(px(12.0))
                            .child(format!("{} entries", self.toc_items.len())),
                    );
                } else {
                    v = v.child("No contents");
                }
                v
            }
            SidebarMode::Bookmarks => {
                let mut v = v_flex().gap_2().p_2();
                if let Some(bs) = &self.bookmark_state {
                    v = v.child(Select::new(bs).placeholder("Bookmarks"));
                } else {
                    v = v.child("No bookmarks");
                }
                v
            }
            SidebarMode::Annotations => {
                let mut v = v_flex().gap_2().p_2();
                v = v.child(Input::new(&self.annotation_input));
                v = v.child(
                    Button::new("addnote")
                        .label("Add note")
                        .on_click(on_add_note),
                );
                for (i, a) in self.annotations.iter().enumerate() {
                    let kind = if a.is_highlight() {
                        "Highlight"
                    } else {
                        "Note"
                    };
                    let idx = a.section;
                    let label = format!(
                        "#{} {} · sec {} · {}",
                        i + 1,
                        kind,
                        a.section + 1,
                        a.summary()
                    );
                    let on_jump = cx.listener(move |view, _ev, _w, cx| view.jump(idx, cx));
                    v = v.child(
                        Button::new(format!("ann{}", i))
                            .ghost()
                            .label(label)
                            .on_click(on_jump),
                    );
                }
                if self.annotations.is_empty() {
                    v = v.child("No annotations");
                }
                v
            }
            SidebarMode::Search => {
                let mut v = v_flex().gap_2().p_2();
                v = v.child(Input::new(&self.search_input));
                v = v.child(Button::new("searchbtn").label("Search").on_click(on_search));
                v = v.child(
                    Button::new("regex")
                        .ghost()
                        .label(if self.search_regex {
                            "Regex: on"
                        } else {
                            "Regex: off"
                        })
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.search_regex = !view.search_regex;
                            cx.notify();
                        })),
                );
                v = v.child(
                    div()
                        .text_size(px(12.0))
                        .child(format!("{} results", self.search_results.len())),
                );
                for r in self.search_results.iter().take(20) {
                    let idx = r.spine_index;
                    let snippet = r.snippet.clone();
                    let on_jump = cx.listener(move |view, _ev, _w, cx| view.jump(idx, cx));
                    v = v.child(
                        Button::new(format!("res{}", idx))
                            .ghost()
                            .label(snippet.chars().take(40).collect::<String>())
                            .on_click(on_jump),
                    );
                }
                v
            }
            SidebarMode::Footnotes => {
                let mut v = v_flex().gap_2().p_2();
                if let Some(book) = &self.book {
                    if let Ok(sec) = book.get_section(self.current) {
                        let fns = sec.extract_footnotes();
                        if fns.is_empty() {
                            v = v.child("No footnotes");
                        }
                        for f in fns {
                            v = v.child(div().p_1().border_1().child(format!(
                                "{}: {}",
                                f.label,
                                f.plain_text.chars().take(120).collect::<String>()
                            )));
                        }
                    }
                } else {
                    v = v.child("No book");
                }
                v
            }
            SidebarMode::Dictionary => {
                let mut v = v_flex().gap_2().p_2();
                v = v.child(Input::new(&self.dictionary_input));
                let on_lookup = cx.listener(|view, _ev, _w, cx| {
                    let word = view.dictionary_input.read(cx).value().to_string();
                    if word.trim().is_empty() {
                        return;
                    }
                    let url = format!(
                        "https://api.dictionaryapi.dev/api/v2/entries/en/{}",
                        word.trim()
                    );
                    view.dictionary_result = match ureq::get(&url).call() {
                        Ok(resp) => {
                            let body = resp.into_body().read_to_string().unwrap_or_default();
                            // extract first definition roughly
                            if let Some(idx) = body.find("\"definition\":\"") {
                                let rest = &body[idx + 14..];
                                if let Some(end) = rest.find('"') {
                                    rest[..end].to_string()
                                } else {
                                    "No definition".into()
                                }
                            } else {
                                "No definition".into()
                            }
                        }
                        Err(_) => "Lookup failed".into(),
                    };
                    cx.notify();
                });
                v = v.child(Button::new("lookup").label("Lookup").on_click(on_lookup));
                let on_wiki = cx.listener(|view, _ev, _w, cx| {
                    let word = view.dictionary_input.read(cx).value().to_string();
                    if word.trim().is_empty() {
                        return;
                    }
                    let url = format!(
                        "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
                        word.trim().replace(' ', "_")
                    );
                    view.dictionary_result = match ureq::get(&url).call() {
                        Ok(resp) => {
                            let body = resp.into_body().read_to_string().unwrap_or_default();
                            if let Some(idx) = body.find("\"extract\":\"") {
                                let rest = &body[idx + 11..];
                                if let Some(end) = rest.find("\"extract_html\"") {
                                    rest[..end].trim_end_matches(&['"', ','][..]).to_string()
                                } else {
                                    "No summary".into()
                                }
                            } else {
                                "No summary".into()
                            }
                        }
                        Err(_) => "Lookup failed".into(),
                    };
                    cx.notify();
                });
                v = v.child(Button::new("wiki").label("Wikipedia").on_click(on_wiki));
                v = v.child(
                    div()
                        .text_size(px(12.0))
                        .child(self.dictionary_result.clone()),
                );
                v
            }
        };

        // Two per row so the six modes occupy a predictable block instead of
        // wrapping unpredictably down a narrow column.
        let sidebar_buttons = h_flex()
            .gap_1()
            .p_2()
            .flex_wrap()
            .child(
                Button::new("sb_contents")
                    .ghost()
                    .label("Contents")
                    .w(px(110.0))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Contents;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_bm")
                    .ghost()
                    .label("Bookmarks")
                    .w(px(110.0))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Bookmarks;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_ann")
                    .ghost()
                    .label("Notes")
                    .w(px(110.0))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Annotations;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_search")
                    .ghost()
                    .label("Search")
                    .w(px(110.0))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Search;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_fn")
                    .ghost()
                    .label("Footnotes")
                    .w(px(110.0))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Footnotes;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_dict")
                    .ghost()
                    .label("Dictionary")
                    .w(px(110.0))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Dictionary;
                        cx.notify();
                    })),
            );
        // ---- primary toolbar ------------------------------------------------
        // Reading actions stay visible; everything else moves into one of two
        // collapsible secondary bars, so the toolbar stops being a wall of
        // twenty-odd buttons.
        // Left group takes the slack so the progress/Aa/menu cluster sits hard
        // against the right edge.
        // ---- primary toolbar ------------------------------------------------
        // Reading actions stay visible; everything else moves into one of two
        // collapsible secondary bars, so the toolbar stops being a wall of
        // twenty-odd buttons.
        let header =
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .px_2()
                .py_2()
                .border_b_1()
                .child(
                    h_flex()
                        .flex_grow(1.0)
                        .gap_2()
                        .items_center()
                        .child(Button::new("open").label("Open").on_click(on_open))
                        .child(Button::new("library").label("Library").on_click(on_lib))
                        .child(
                            Button::new("prev")
                                .ghost()
                                .label("\u{2039}")
                                .on_click(on_prev),
                        )
                        .child(
                            Button::new("next")
                                .ghost()
                                .label("\u{203a}")
                                .on_click(on_next),
                        )
                        .child(Button::new("bm").label("Bookmark").on_click(on_bm))
                        .child(
                            Button::new("highlight")
                                .label("Highlight")
                                .on_click(on_highlight),
                        )
                        .child(Button::new("copy").label("Copy").on_click(on_copy)),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(12.0))
                                .child(format!("{}  ({:.0}%)", pos, progress)),
                        )
                        .child(Button::new("aa").ghost().label("Aa").on_click(cx.listener(
                            |view, _, _, cx| {
                                view.panel = if view.panel == Panel::Typography {
                                    Panel::None
                                } else {
                                    Panel::Typography
                                };
                                cx.notify();
                            },
                        )))
                        .child(Button::new("more").ghost().label("\u{22ef}").on_click(
                            cx.listener(|view, _, _, cx| {
                                view.panel = if view.panel == Panel::Actions {
                                    Panel::None
                                } else {
                                    Panel::Actions
                                };
                                cx.notify();
                            }),
                        )),
                );

        // ---- typography bar -------------------------------------------------
        // Labelled steppers, so the meaning of every "+/-" pair is obvious.
        let typography_panel = h_flex()
            .w_full()
            .flex_wrap()
            .items_center()
            .gap_5()
            .px_3()
            .py_2()
            .border_b_1()
            .child(setting_row(
                "Size",
                format!("{:.0}", self.font_size),
                Button::new("fs-dec")
                    .ghost()
                    .label("\u{2212}")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_font_size(-1.0, cx)))
                    .into_any_element(),
                Button::new("fs-inc")
                    .ghost()
                    .label("+")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_font_size(1.0, cx)))
                    .into_any_element(),
            ))
            .child(setting_row(
                "Line height",
                format!("{:.1}", self.line_height),
                Button::new("lh-dec")
                    .ghost()
                    .label("\u{2212}")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_line_height(-0.1, cx)))
                    .into_any_element(),
                Button::new("lh-inc")
                    .ghost()
                    .label("+")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_line_height(0.1, cx)))
                    .into_any_element(),
            ))
            .child(setting_row(
                "Margin",
                format!("{:.0}", self.margin),
                Button::new("mg-dec")
                    .ghost()
                    .label("\u{2212}")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_margin(-4.0, cx)))
                    .into_any_element(),
                Button::new("mg-inc")
                    .ghost()
                    .label("+")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_margin(4.0, cx)))
                    .into_any_element(),
            ))
            .child(setting_row(
                "Text width",
                format!("{:.0}", self.max_width),
                Button::new("mw-dec")
                    .ghost()
                    .label("\u{2212}")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_width(-40.0, cx)))
                    .into_any_element(),
                Button::new("mw-inc")
                    .ghost()
                    .label("+")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_width(40.0, cx)))
                    .into_any_element(),
            ))
            .child(cycle_row(
                "Typeface",
                &self.font_family,
                "tf",
                cx.listener(|v, _, _, cx| v.cycle_font(cx)),
            ))
            .child(cycle_row(
                "Theme",
                &self.theme,
                "th",
                cx.listener(|v, _, _, cx| v.cycle_theme(cx)),
            ))
            .child(cycle_row(
                "Pages",
                if self.two_page { "two" } else { "one" },
                "pg",
                cx.listener(|v, _, _, cx| v.toggle_two_page(cx)),
            ))
            .child(
                Button::new("reset-display")
                    .ghost()
                    .label("Reset to defaults")
                    .on_click(cx.listener(|v, _, _, cx| v.reset_display(cx))),
            );

        // ---- actions bar ----------------------------------------------------
        let actions_panel = h_flex()
            .w_full()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .child(
                Button::new("info")
                    .ghost()
                    .label("Book info\u{2026}")
                    .on_click(cx.listener(|v, _, _, cx| {
                        if let Some(body) = v.book_info_body() {
                            v.show_modal("Book info", body, cx);
                        }
                    })),
            )
            .child(
                Button::new("validate")
                    .ghost()
                    .label("Validate book\u{2026}")
                    .on_click(cx.listener(|v, _, _, cx| {
                        if let Some(body) = v.validation_body() {
                            v.show_modal("Validation", body, cx);
                        }
                    })),
            )
            .child(
                Button::new("saveepub")
                    .ghost()
                    .label("Save as EPUB\u{2026}")
                    .on_click(cx.listener(|v, _, window, cx| v.save_as_epub(window, cx))),
            )
            .child(
                Button::new("export")
                    .ghost()
                    .label("Export notes\u{2026}")
                    .on_click(cx.listener(|v, _, _, _cx| v.export_annotations())),
            )
            .child(div().w(px(1.0)).h(px(18.0)).border_l_1())
            .child(Input::new(&self.goto_input).w(px(90.0)))
            .child(
                Button::new("go")
                    .ghost()
                    .label("Go to section")
                    .on_click(cx.listener(|v, _ev, _w, cx| {
                        if let Ok(n) = v.goto_input.read(cx).value().parse::<usize>() {
                            v.jump(n.saturating_sub(1), cx);
                        }
                    })),
            )
            .child(
                Button::new("fs")
                    .ghost()
                    .label("Fullscreen")
                    .on_click(on_fs),
            )
            .child(
                Button::new("newwin")
                    .ghost()
                    .label("New window")
                    .on_click(cx.listener(|_view, _ev, _window, cx| {
                        let options = window_options();
                        cx.open_window(options, |window, cx| {
                            let view = cx.new(|cx| ReaderView::new(window, cx));
                            cx.new(|cx| Root::new(view, window, cx))
                        })
                        .ok();
                    })),
            );

        let secondary: AnyElement = match self.panel {
            Panel::Typography => typography_panel.into_any_element(),
            Panel::Actions => actions_panel.into_any_element(),
            Panel::None => div().into_any_element(),
        };

        let html2 = if self.two_page {
            if let Some(book) = &self.book {
                book.get_section(self.current + 1)
                    .map(|s| constrain_images(&s.processed_html, column_px))
                    .unwrap_or_default()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let page = |id: String, h: String| {
            div()
                .min_w(px(0.0))
                .max_w(px(max_width))
                .w_full()
                .h_full()
                .px(px(margin))
                .text_size(px(font_size))
                .line_height(px(font_size * line_height))
                .font_family(self.font_family.clone())
                .text_align(TextAlign::Left)
                .overflow_hidden()
                .child(
                    TextView::html(id.clone(), h)
                        // Enables drag-to-select. The selection is read back with
                        // gpui::TextSelection::selected_text() for copy and highlights.
                        .selectable(true)
                        .scrollable(true),
                )
        };
        let content = if self.two_page {
            h_flex()
                .min_w(px(0.0))
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .h_full()
                .overflow_hidden()
                .child(page("content".to_string(), html.clone()))
                .child(page("content2".to_string(), html2))
        } else {
            div()
                .min_w(px(0.0))
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .h_full()
                .overflow_hidden()
                .flex()
                .justify_center()
                .child(page("content".to_string(), html.clone()))
        };

        let sidebar_panel = div()
            .w(px(260.0))
            .min_w(px(0.0))
            .max_w(px(260.0))
            .flex_shrink(0.0)
            .h_full()
            .overflow_hidden()
            .child(
                v_flex()
                    .size_full()
                    .child(sidebar_buttons)
                    .child(div().flex_1().overflow_hidden().child(sidebar)),
            );

        let body = h_flex()
            .size_full()
            .min_w(px(0.0))
            .child(sidebar_panel)
            .child(content);

        let on_key = cx.listener(|view, event: &KeyDownEvent, window, cx| {
            let key = event.keystroke.key.as_str();
            let ctrl = event.keystroke.modifiers.control;
            if key == "escape" {
                // Escape closes the modal, else the secondary bar, else returns
                // to the library — innermost first, so it always feels like "back".
                if view.modal.is_some() {
                    view.close_modal(cx);
                } else if view.panel != Panel::None {
                    view.panel = Panel::None;
                    cx.notify();
                } else if !view.show_library {
                    window.set_window_title("eParna");
                    view.go_library(cx);
                }
                return;
            }
            if ctrl {
                match key {
                    "o" => {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(
                                "Ebook",
                                &[
                                    "epub", "mobi", "azw3", "fb2", "pdf", "txt", "md", "docx",
                                    "odt", "rtf", "cbz", "lit", "kfx", "kepub", "kfx", "kepub",
                                ],
                            )
                            .pick_file()
                        {
                            view.open_file(path, window, cx);
                        }
                    }
                    "f" => {
                        view.sidebar_mode = SidebarMode::Search;
                        cx.notify();
                    }
                    "b" => view.toggle_bookmark(window, cx),
                    "c" => view.copy_selection(window, cx),
                    "l" => {
                        window.set_window_title("eParna");
                        view.go_library(cx);
                    }
                    _ => {}
                }
            } else {
                match key {
                    "ArrowRight" | "PageDown" | " " => view.next(cx),
                    "ArrowLeft" | "PageUp" => view.prev(cx),
                    "Home" => view.jump(0, cx),
                    "End" => {
                        if let Some(b) = &view.book {
                            let last = b.sections().len().saturating_sub(1);
                            view.jump(last, cx);
                        }
                    }
                    "F11" => view.toggle_fullscreen(window, cx),
                    "+" | "=" => view.change_font(2.0, cx),
                    "-" => view.change_font(-2.0, cx),
                    "j" => {
                        view.justify = !view.justify;
                        view.sync_state();
                        cx.notify();
                    }
                    _ => {}
                }
            }
        });

        // A modal takes over the body, so long reports (book info, validation)
        // get a full reading surface instead of a squeezed toast.
        let body = if let Some(m) = &self.modal {
            v_flex()
                .flex_1()
                .size_full()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    v_flex()
                        .w(px(720.0))
                        .max_h(px(560.0))
                        .gap_3()
                        .border_1()
                        .p_4()
                        .child(div().text_size(px(16.0)).child(m.title.clone()))
                        .child(div().flex_1().max_h(px(440.0)).text_size(px(13.0)).child(
                            TextView::html("modal-body", escape_html(&m.body)).scrollable(true),
                        ))
                        .child(
                            h_flex().justify_end().child(
                                Button::new("modal-close")
                                    .label("Close")
                                    .on_click(cx.listener(|v, _, _, cx| v.close_modal(cx))),
                            ),
                        ),
                )
        } else {
            body
        };

        let root =
            v_flex()
                .size_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .bg(bg)
                .text_color(fg)
                .on_key_down(on_key)
                .child(header)
                .child(secondary)
                .child(div().px_2().child(Progress::new("prog").value(progress)))
                .child(div().px_2().text_size(px(12.0)).child(
                    if self.selection_status.is_empty() {
                        status.clone()
                    } else {
                        format!("{} — {}", status, self.selection_status)
                    },
                ))
                .child(body);

        root.into_any_element()
    }
}

/// A first-run window that actually fits on the display.
///
/// Inheriting the platform default hands back a window larger than the screen
/// on HiDPI setups — GPUI's default is the display's *device* size, but window
/// bounds are logical, so the result overflows badly and the right-hand end of
/// the toolbar is off-screen. A fixed logical size avoids mixing those units,
/// and the user is free to resize or maximise afterwards.
fn window_options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(60.0), px(60.0)),
            size: size(px(1000.0), px(660.0)),
        })),
        window_min_size: Some(size(px(420.0), px(320.0))),
        ..Default::default()
    }
}

fn main() {
    // GPUI element trees can get deep, and unoptimized (debug) builds use much
    // larger stack frames per function. Run the whole app on a worker thread
    // with a generous stack so debug builds don't overflow the default 1 MB.
    let worker = std::thread::Builder::new()
        .name("epustak-main".into())
        .stack_size(256 * 1024 * 1024)
        .spawn(run_app)
        .expect("failed to spawn main thread");
    let _ = worker.join();
}

fn run_app() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let options = window_options();
            cx.spawn(async move |cx| {
                cx.open_window(options, |window, cx| {
                    let view = cx.new(|cx| ReaderView::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("open window");
            })
            .detach();
        });
}
