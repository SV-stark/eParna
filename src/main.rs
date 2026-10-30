mod config;
mod html_utils;
mod models;
mod stardict;

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
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

use crate::config::{covers_dir, load_state, save_state};
use crate::html_utils::*;
use crate::models::*;
use crate::stardict::StarDict;

fn href_to_index(book: &ebook_rs::Book, href: &str) -> Option<usize> {
    let href = href.split('#').next().unwrap_or(href);
    book.sections()
        .iter()
        .position(|s| s.href == href || s.full_path.ends_with(href) || href.ends_with(&s.href))
}

fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            encoded.push(b as char);
        } else if b == b' ' {
            encoded.push('+');
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}

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
    custom_bg: String,
    custom_fg: String,
    custom_bg_input: Entity<InputState>,
    custom_fg_input: Entity<InputState>,

    library_search_input: Entity<InputState>,
    library_sort: usize,
    library_favorites_only: bool,
    opds_url_input: Entity<InputState>,
    opds_entries: Vec<(String, String)>,
    opds_status: String,

    dictionary_input: Entity<InputState>,
    dictionary_result: String,
    stardict: Option<StarDict>,
    stardict_name: String,

    search_regex: bool,
    toc_search_input: Entity<InputState>,
    toc_matches: Vec<(String, String, usize)>,
    covers: HashMap<String, String>,

    selection_status: String,
    reading_start: std::time::Instant,

    panel: Panel,
    modal: Option<Modal>,

    passphrase_pending: Option<PathBuf>,
    passphrase_input: Entity<InputState>,
    passphrase_error: String,
    passphrase_modal: bool,

    fullscreen: bool,

    // In-page search & highlighting
    find_in_page_input: Entity<InputState>,
    show_find_bar: bool,
    active_search_query: String,

    // Manga & Arabic/Hebrew Right-to-Left (RTL) reading flow
    rtl_mode: bool,

    // Highlight color selection: yellow, green, blue, pink, purple
    highlight_color: String,

    // Library shelf filter: 0 = All, 1 = Reading, 2 = Favorites, 3 = Finished, 4 = Want to Read
    library_shelf_filter: usize,

    // Distraction-free / Zen mode
    zen_mode: bool,

    // Navigation history stack for jump-back (e.g. after following footnotes or links)
    nav_history: Vec<usize>,
}

impl ReaderView {
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

    fn cache_cover(&self, book: &ebook_rs::Book, path: &str) -> String {
        let Some((bytes, mime)) = book.cover_image() else {
            return String::new();
        };
        if bytes.len() > 256 * 1024 {
            return String::new();
        }
        let dir = covers_dir();
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

    fn parse_hex_color(s: &str) -> Option<Hsla> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)).into())
    }

    fn theme_colors(&self) -> (Hsla, Hsla) {
        match self.theme.as_str() {
            "Dark" => (rgb(0x1e1e1e).into(), rgb(0xe0e0e0).into()),
            "OLED Black" => (rgb(0x000000).into(), rgb(0xd4d4d4).into()),
            "Sepia" => (rgb(0xf4ecd8).into(), rgb(0x5b4636).into()),
            "Nord" => (rgb(0x2e3440).into(), rgb(0xeceff4).into()),
            "Solarized" => (rgb(0xfdf6e3).into(), rgb(0x657b83).into()),
            "Solarized Dark" => (rgb(0x002b36).into(), rgb(0x839496).into()),
            "E-Ink" => (rgb(0xffffff).into(), rgb(0x000000).into()),
            "Custom" => (
                Self::parse_hex_color(&self.custom_bg).unwrap_or(rgb(0xffffff).into()),
                Self::parse_hex_color(&self.custom_fg).unwrap_or(rgb(0x202020).into()),
            ),
            _ => (rgb(0xffffff).into(), rgb(0x202020).into()),
        }
    }

    fn save_state(&self) {
        save_state(&self.state);
    }

    fn sync_state(&mut self) {
        let elapsed = self.reading_start.elapsed().as_secs();
        if elapsed > 0 {
            self.state.total_reading_secs = self.state.total_reading_secs.saturating_add(elapsed);
            if let Some(path) = &self.book_path {
                let key = path.to_string_lossy().to_string();
                if let Some(r) = self.state.recent.iter_mut().find(|r| r.path == key) {
                    r.reading_time_secs = r.reading_time_secs.saturating_add(elapsed);
                }
            }
            self.reading_start = std::time::Instant::now();
        }
        self.state.font_size = self.font_size;
        self.state.line_height = self.line_height;
        self.state.margin = self.margin;
        self.state.max_width = self.max_width;
        self.state.justify = self.justify;
        self.state.font_family = self.font_family.clone();
        self.state.two_page = self.two_page;
        self.state.theme = self.theme.clone();
        self.state.dark = self.dark;
        self.state.custom_bg = self.custom_bg.clone();
        self.state.custom_fg = self.custom_fg.clone();
        self.save_state();
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.set_window_title("eParna");
        let st = load_state();
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
        let passphrase_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Passphrase..."));
        let custom_bg_input = cx.new(|cx| InputState::new(window, cx).placeholder("#ffffff"));
        let custom_fg_input = cx.new(|cx| InputState::new(window, cx).placeholder("#202020"));
        let find_in_page_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Find in current chapter..."));

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
            custom_bg: st.custom_bg.clone(),
            custom_fg: st.custom_fg.clone(),
            custom_bg_input,
            custom_fg_input,
            state: st,
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
            stardict: None,
            stardict_name: String::new(),
            search_regex: false,
            toc_search_input,
            passphrase_pending: None,
            passphrase_input,
            passphrase_error: String::new(),
            passphrase_modal: false,
            toc_matches: Vec::new(),
            covers: HashMap::new(),
            selection_status: String::new(),
            reading_start: std::time::Instant::now(),
            panel: Panel::None,
            modal: None,
            find_in_page_input,
            show_find_bar: false,
            active_search_query: String::new(),
            rtl_mode: false,
            highlight_color: "yellow".into(),
            library_shelf_filter: 0,
            zen_mode: false,
            nav_history: Vec::new(),
        };

        if view.show_library {
            view.load_covers();
        }
        if let Some(arg) = std::env::args().nth(1) {
            view.open_file(PathBuf::from(arg), window, cx);
        }
        view
    }

    fn scan_folder(&mut self, folder: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        const EXTENSIONS: &[&str] = &[
            "epub", "mobi", "azw3", "fb2", "pdf", "txt", "md", "docx", "odt", "rtf", "cbz", "lit",
            "kfx", "kepub",
        ];
        let mut found = Vec::new();
        let mut stack = vec![folder];
        while let Some(dir) = stack.pop() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.is_file()
                        && let Some(ext) = p.extension().and_then(|e| e.to_str())
                        && EXTENSIONS.contains(&ext.to_lowercase().as_str())
                    {
                        found.push(p);
                    }
                }
            }
        }

        let mut added = 0usize;
        let now = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        for p in found {
            let key = p.to_string_lossy().to_string();
            if self.state.recent.iter().any(|r| r.path == key) {
                continue;
            }
            if let Ok(book) = ebook_rs::Book::from_file(&p) {
                let title = book.metadata().title.clone();
                let author = book.metadata().creator().to_string();
                let format = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let total = book.sections().len();
                let cover = self.cache_cover(&book, &key);
                let file_size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                let content_hash = if file_size <= 32 * 1024 * 1024 {
                    book.fingerprint().content_hash
                } else {
                    String::new()
                };

                self.state.recent.push(RecentBook {
                    path: key,
                    title,
                    section: 0,
                    opened: now.clone(),
                    favorite: false,
                    status: "reading".into(),
                    content_hash,
                    cfi: String::new(),
                    author,
                    format,
                    total,
                    cover,
                    reading_time_secs: 0,
                });
                added += 1;
            }
        }

        if added > 0 {
            self.save_state();
            self.load_covers();
            notify_simple(
                window,
                cx,
                "Folder Scanned",
                format!("Added {} book(s) to your library", added),
            );
            cx.notify();
        } else {
            notify_simple(
                window,
                cx,
                "Scan Complete",
                "No new books found in folder".into(),
            );
        }
    }

    fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open_file_inner(path, None, window, cx);
    }

    fn open_file_inner(
        &mut self,
        path: PathBuf,
        passphrase: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let book_result = if let Some(pp) = &passphrase {
            std::fs::read(&path)
                .map_err(|e| format!("io: {e}"))
                .and_then(|bytes| {
                    let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("eBook");
                    ebook_rs::Book::from_bytes_with_passphrase(&bytes, title, pp)
                        .map_err(|e| e.to_string())
                })
        } else {
            ebook_rs::Book::from_file(&path).map_err(|e| e.to_string())
        };
        match book_result {
            Ok(mut book) => {
                if book.locations().total_locations == 0 {
                    book.generate_locations(1000);
                }
                let title = book.metadata().title.clone();
                window.set_window_title(&format!("{} — eParna", title));

                // Detect Manga / Comic Right-to-Left (RTL) reading flow
                self.rtl_mode =
                    book.metadata().direction == ebook_rs::PageProgressionDirection::Rtl;

                let author = book.metadata().creator().to_string();
                let format = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let total = book.sections().len();
                let key = path.to_string_lossy().to_string();
                let saved = self
                    .state
                    .recent
                    .iter()
                    .find(|r| r.path == key)
                    .map(|r| (r.section, r.cfi.clone()));
                let cover = self.cache_cover(&book, &key);
                let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let content_hash = if file_size <= 32 * 1024 * 1024 {
                    book.fingerprint().content_hash
                } else {
                    String::new()
                };

                let mut items: Vec<(String, usize)> = Vec::new();
                for flat in book.flatten_toc() {
                    if let Some(i) = href_to_index(&book, &flat.href)
                        && !items.iter().any(|(_, j)| *j == i)
                    {
                        let label = format!("{}{}", "  ".repeat(flat.depth), flat.label);
                        items.push((label, i));
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

                self.annotations = self
                    .state
                    .annotations
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                self.bookmarks = self.state.bookmarks.get(&key).cloned().unwrap_or_default();
                self.nav_history.clear();
                self.refresh_bookmark_state(window, cx);

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
                let prev_reading_time = self
                    .state
                    .recent
                    .iter()
                    .find(|r| r.path == key)
                    .map(|r| r.reading_time_secs)
                    .unwrap_or(0);

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
                            status: "reading".into(),
                            content_hash,
                            cfi: String::new(),
                            author: author.clone(),
                            format: format.clone(),
                            total,
                            cover,
                            reading_time_secs: prev_reading_time,
                        },
                    );
                }
                if self.state.recent.len() > 100 {
                    self.state.recent.truncate(100);
                }
                self.save_state();

                self.toc_items = items;
                self.toc_state = Some(toc_state);
                self.book = Some(book);
                self.book_path = Some(path);
                self.reading_start = std::time::Instant::now();
                self.search_results.clear();
                self.show_library = false;

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
                let msg = e;
                if (msg.to_lowercase().contains("drm")
                    || msg.to_lowercase().contains("passphrase")
                    || msg.to_lowercase().contains("lcp"))
                    && passphrase.is_none()
                {
                    self.passphrase_pending = Some(path.clone());
                    self.passphrase_error = msg;
                    self.passphrase_modal = true;
                } else if passphrase.is_some() {
                    self.passphrase_error = msg;
                    self.passphrase_modal = true;
                }
            }
        }
        cx.notify();
    }

    fn load_current(&mut self, _cx: &mut Context<Self>) {
        if let Some(book) = &self.book {
            match book.get_section(self.current) {
                Ok(sec) => {
                    let current_annotations: Vec<Annotation> = self
                        .annotations
                        .iter()
                        .filter(|a| a.section == self.current)
                        .cloned()
                        .collect();
                    let active_search = if self.active_search_query.trim().is_empty() {
                        None
                    } else {
                        Some(self.active_search_query.as_str())
                    };
                    self.html = prepare_section_html(
                        &sec.processed_html,
                        book,
                        &sec.full_path,
                        &current_annotations,
                        active_search,
                        &self.theme,
                        self.justify,
                    );
                }
                Err(e) => {
                    self.html = format!("<p>Error: {}</p>", e);
                }
            }
        }
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

    fn section_from_cfi(&self, cfi: &str) -> Option<usize> {
        let book = self.book.as_ref()?;
        if cfi.trim().is_empty() {
            return None;
        }
        book.get_section_and_offset_by_cfi(cfi)
            .ok()
            .map(|(sec, _offset)| sec.index)
            .filter(|i| *i < book.sections().len())
    }

    fn next(&mut self, cx: &mut Context<Self>) {
        if self.rtl_mode {
            self.prev_section(cx);
        } else {
            self.next_section(cx);
        }
    }

    fn prev(&mut self, cx: &mut Context<Self>) {
        if self.rtl_mode {
            self.next_section(cx);
        } else {
            self.prev_section(cx);
        }
    }

    fn next_section(&mut self, cx: &mut Context<Self>) {
        if let Some(book) = &self.book
            && self.current + 1 < book.sections().len()
        {
            self.current += 1;
            self.load_current(cx);
            cx.notify();
        }
    }

    fn prev_section(&mut self, cx: &mut Context<Self>) {
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
            if idx != self.current {
                self.nav_history.push(self.current);
                if self.nav_history.len() > 50 {
                    self.nav_history.remove(0);
                }
            }
            self.current = idx;
            self.load_current(cx);
            cx.notify();
        }
    }

    fn jump_back(&mut self, cx: &mut Context<Self>) {
        if let Some(prev) = self.nav_history.pop()
            && let Some(book) = &self.book
            && prev < book.sections().len()
        {
            self.current = prev;
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
            self.active_search_query = q.clone();
            self.search_results = if self.search_regex {
                book.search_regex_limited(&q, Some(500)).unwrap_or_default()
            } else {
                book.search_limited(&q, Some(500))
            };
            if let Some(r) = self.search_results.first() {
                self.current = r.spine_index;
            }
            self.load_current(cx);
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
        if let Some(path) = &self.book_path {
            let key = path.to_string_lossy().to_string();
            self.state.bookmarks.insert(key, self.bookmarks.clone());
            self.save_state();
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

    fn push_annotation(&mut self, ann: Annotation, cx: &mut Context<Self>) {
        self.annotations.push(ann);
        if let Some(path) = &self.book_path {
            let key = path.to_string_lossy().to_string();
            self.state.annotations.insert(key, self.annotations.clone());
            self.save_state();
        }
        // Refresh reading HTML to display the newly added in-text highlight immediately
        self.load_current(cx);
        cx.notify();
    }

    fn selected_text(window: &mut Window, cx: &mut App) -> String {
        TextSelection::selected_text(window, cx).trim().to_string()
    }

    fn lookup_word(&mut self, word: &str, cx: &mut Context<Self>) {
        let word = word.trim();
        if word.is_empty() {
            return;
        }
        if let Some(sd) = &self.stardict
            && let Some(def) = sd.lookup(word)
        {
            self.dictionary_result = def;
            self.sidebar_mode = SidebarMode::Dictionary;
            cx.notify();
            return;
        }

        self.sidebar_mode = SidebarMode::Dictionary;
        self.dictionary_result = format!("Looking up “{}”...", word);
        cx.notify();

        let word_owned = word.to_string();
        cx.spawn(async move |view, cx| {
            let url = format!(
                "https://api.dictionaryapi.dev/api/v2/entries/en/{}",
                word_owned
            );
            let res = match ureq::get(&url).call() {
                Ok(resp) => {
                    let body = resp.into_body().read_to_string().unwrap_or_default();
                    if let Some(idx) = body.find("\"definition\":\"") {
                        let rest = &body[idx + 14..];
                        if let Some(end) = rest.find('"') {
                            rest[..end].to_string()
                        } else {
                            "No definition found".into()
                        }
                    } else {
                        "No definition found".into()
                    }
                }
                Err(_) => "Lookup failed (offline or word not found)".into(),
            };
            let _ = view.update(cx, |view, cx| {
                view.dictionary_result = res;
                cx.notify();
            });
        })
        .detach();
    }

    fn copy_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = Self::selected_text(window, cx);
        if text.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.selection_status = format!("Copied {} characters", text.chars().count());
        cx.notify();
    }

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
                color: self.highlight_color.clone(),
            },
            cx,
        );
        self.selection_status = format!("Highlighted {} characters", chars);
    }

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
                color: self.highlight_color.clone(),
            },
            cx,
        );
    }

    fn go_library(&mut self, cx: &mut Context<Self>) {
        self.show_library = true;
        self.load_covers();
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
        let on_scan = cx.listener(|view, _ev, window, cx| {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                view.scan_folder(folder, window, cx);
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

        let is_list_view = self.state.library_view_mode == "list";
        let on_toggle_view = cx.listener(|view, _ev, _w, cx| {
            view.state.library_view_mode = if view.state.library_view_mode == "list" {
                "grid".into()
            } else {
                "list".into()
            };
            view.save_state();
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
                    .child(
                        Button::new("scanlib")
                            .label("Scan folder")
                            .on_click(on_scan),
                    )
                    .child(Button::new("sortlib").label(sort_label).on_click(on_sort))
                    .child(
                        Button::new("favlib")
                            .label(fav_label)
                            .on_click(on_fav_toggle),
                    )
                    .child(
                        Button::new("viewmode")
                            .label(if is_list_view {
                                "View: List"
                            } else {
                                "View: Grid"
                            })
                            .on_click(on_toggle_view),
                    ),
            );

        v = v.child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    Button::new("shelf_all")
                        .ghost()
                        .label(if self.library_shelf_filter == 0 {
                            "● All"
                        } else {
                            "All"
                        })
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.library_shelf_filter = 0;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("shelf_reading")
                        .ghost()
                        .label(if self.library_shelf_filter == 1 {
                            "● Reading"
                        } else {
                            "Reading"
                        })
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.library_shelf_filter = 1;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("shelf_fav")
                        .ghost()
                        .label(if self.library_shelf_filter == 2 {
                            "● Favorites"
                        } else {
                            "Favorites"
                        })
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.library_shelf_filter = 2;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("shelf_fin")
                        .ghost()
                        .label(if self.library_shelf_filter == 3 {
                            "● Finished"
                        } else {
                            "Finished"
                        })
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.library_shelf_filter = 3;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("shelf_want")
                        .ghost()
                        .label(if self.library_shelf_filter == 4 {
                            "● Want to Read"
                        } else {
                            "Want to Read"
                        })
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.library_shelf_filter = 4;
                            cx.notify();
                        })),
                ),
        );

        v = v.child(Input::new(&self.library_search_input));
        let q = self.library_search_input.read(cx).value().to_lowercase();
        let mut recent = self.state.recent.clone();
        match self.library_sort {
            1 => recent.sort_by_key(|a| a.title.to_lowercase()),
            2 => recent.sort_by_key(|a| a.path.to_lowercase()),
            _ => {}
        }

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

        v = v.child(section_heading(&format!("All books ({})", recent.len())));

        if is_list_view {
            // ---- List View Layout -----------------------------------------------
            let mut list_col = v_flex().gap_2().w_full();
            let mut shown = 0usize;
            for (i, r) in recent.iter().enumerate() {
                if self.library_favorites_only && !r.favorite {
                    continue;
                }
                match self.library_shelf_filter {
                    1 if r.status == "finished" || r.status == "want" => continue,
                    2 if !r.favorite => continue,
                    3 if r.status != "finished" && !(r.total > 0 && r.section + 1 >= r.total) => {
                        continue;
                    }
                    4 if r.status != "want" => continue,
                    _ => {}
                }
                if !q.is_empty()
                    && !r.title.to_lowercase().contains(&q)
                    && !r.author.to_lowercase().contains(&q)
                    && !r.path.to_lowercase().contains(&q)
                {
                    continue;
                }
                shown += 1;
                let id = format!("list-bk{i}");
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
                let on_status = {
                    let p = r.path.clone();
                    cx.listener(move |view, _ev, _w, cx| view.cycle_book_status(p.clone(), cx))
                };
                let status_badge = match r.status.as_str() {
                    "finished" => "Done",
                    "want" => "Want",
                    _ => "Reading",
                };

                list_col = list_col.child(
                    h_flex()
                        .w_full()
                        .p_2()
                        .gap_3()
                        .border_1()
                        .items_center()
                        .child(cover_art(
                            format!("{id}-c"),
                            cover_for(&self.covers, r),
                            38.0,
                            52.0,
                        ))
                        .child(
                            v_flex()
                                .flex_1()
                                .child(div().text_size(px(14.0)).child(book_title(r)))
                                .child(div().text_size(px(12.0)).child(book_subtitle(r)))
                                .child(div().text_size(px(11.0)).child(format!(
                                    "{} min read · {} sections",
                                    r.reading_time_secs / 60,
                                    r.total
                                ))),
                        )
                        .child(
                            v_flex()
                                .w(px(120.0))
                                .child(Progress::new(format!("{id}-p")).value(pct))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_align(TextAlign::Center)
                                        .child(format!("{pct:.0}%")),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    Button::new(format!("{id}-open"))
                                        .label("Read")
                                        .on_click(on_open),
                                )
                                .child(
                                    Button::new(format!("{id}-st"))
                                        .ghost()
                                        .label(status_badge)
                                        .on_click(on_status),
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
                                ),
                        ),
                );
            }
            v = v.child(list_col);
            if shown == 0 {
                v = v.child(
                    div()
                        .text_size(px(13.0))
                        .child("No books match the current filter."),
                );
            }
        } else {
            // ---- Grid View Layout -----------------------------------------------
            let mut grid = h_flex().gap_3().flex_wrap();
            let mut shown = 0usize;
            for (i, r) in recent.iter().enumerate() {
                if self.library_favorites_only && !r.favorite {
                    continue;
                }
                match self.library_shelf_filter {
                    1 if r.status == "finished" || r.status == "want" => continue,
                    2 if !r.favorite => continue,
                    3 if r.status != "finished" && !(r.total > 0 && r.section + 1 >= r.total) => {
                        continue;
                    }
                    4 if r.status != "want" => continue,
                    _ => {}
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
                let on_status = {
                    let p = r.path.clone();
                    cx.listener(move |view, _ev, _w, cx| view.cycle_book_status(p.clone(), cx))
                };
                let status_badge = match r.status.as_str() {
                    "finished" => "Done",
                    "want" => "Want",
                    _ => "Reading",
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
                            Button::new(format!("{id}-st"))
                                .ghost()
                                .label(status_badge)
                                .on_click(on_status),
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
                            "No books yet \u{2014} open a file or scan a folder to get started."
                        } else {
                            "Nothing matches the current filter."
                        }),
                );
            }
        }

        // OPDS Catalog
        v = v.child(div().text_size(px(16.0)).child("OPDS catalog"));
        v = v.child(Input::new(&self.opds_url_input));
        let on_fetch = cx.listener(|view, _ev, _w, cx| {
            let url = view.opds_url_input.read(cx).value().to_string();
            if url.trim().is_empty() {
                return;
            }
            view.opds_status = "Fetching OPDS catalog...".into();
            cx.notify();
            cx.spawn(async move |view, cx| {
                let res = match ureq::get(&url).call() {
                    Ok(resp) => {
                        let body = resp.into_body().read_to_string().unwrap_or_default();
                        let feed = if body.trim_start().starts_with('{') {
                            ebook_rs::OpdsFeed::parse_json(&body)
                        } else {
                            ebook_rs::OpdsFeed::parse_atom_xml(&body)
                        };
                        match feed {
                            Ok(f) => {
                                let entries: Vec<(String, String)> = f
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
                                let count = entries.len();
                                (format!("{} entries", count), entries)
                            }
                            Err(e) => (format!("Parse error: {e}"), Vec::new()),
                        }
                    }
                    Err(e) => (format!("Fetch error: {e}"), Vec::new()),
                };
                let _ = view.update(cx, |view, cx| {
                    view.opds_status = res.0;
                    view.opds_entries = res.1;
                    cx.notify();
                });
            })
            .detach();
        });

        v = v.child(Button::new("fetchopds").label("Fetch").on_click(on_fetch));
        v = v.child(div().text_size(px(12.0)).child(self.opds_status.clone()));
        for (title, href) in self.opds_entries.clone() {
            let href2 = href.clone();
            let title_name = title.clone();
            let on_open_entry = cx.listener(move |view, _ev, _window, cx| {
                if href2.is_empty() {
                    return;
                }
                let href = href2.clone();
                let title = title_name.clone();
                view.opds_status = format!("Downloading '{}'...", title);
                cx.notify();
                let clean_title = title
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                    .collect::<String>();
                cx.spawn(async move |view, cx| {
                    let dl_res = ureq::get(&href).call().and_then(|resp| {
                        let mut bytes = Vec::new();
                        resp.into_body().as_reader().read_to_end(&mut bytes)?;
                        Ok(bytes)
                    });
                    match dl_res {
                        Ok(bytes) => {
                            let hash = Self::simple_hash(&href);
                            let tmp =
                                std::env::temp_dir().join(format!("{clean_title}_{hash}.epub"));
                            if std::fs::write(&tmp, &bytes).is_ok() {
                                let _ = view.update(cx, |view, cx| {
                                    view.opds_status =
                                        format!("Downloaded '{}'. Opening...", title);
                                    cx.notify();
                                });
                            }
                        }
                        Err(e) => {
                            let _ = view.update(cx, |view, cx| {
                                view.opds_status = format!("Download failed: {e}");
                                cx.notify();
                            });
                        }
                    }
                })
                .detach();
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

    fn show_modal(&mut self, title: &str, body: String, cx: &mut Context<Self>) {
        self.modal = Some(Modal {
            title: title.into(),
            body,
            image: None,
        });
        self.panel = Panel::None;
        cx.notify();
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        cx.notify();
    }

    fn handle_link_click(&mut self, url: &str, cx: &mut Context<Self>) {
        if let Some(src) = url.strip_prefix("img:") {
            self.modal = Some(Modal {
                title: "Image".into(),
                body: String::new(),
                image: Some(src.to_string()),
            });
            cx.notify();
            return;
        }
        let (path_part, frag) = match url.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (url, None),
        };
        if let Some(frag) = frag
            && let Some(book) = &self.book
        {
            let mut found: Option<ebook_rs::footnote::Footnote> = None;
            if let Ok(sec) = book.get_section(self.current) {
                found = sec
                    .extract_footnotes()
                    .into_iter()
                    .find(|f| f.target_id == frag || f.id == frag || f.href.ends_with(frag));
            }
            if found.is_none()
                && !path_part.is_empty()
                && let Ok(sec) = book.get_section_by_href(path_part)
                && let Ok(sec) = book.get_section(sec.index)
            {
                found = sec
                    .extract_footnotes()
                    .into_iter()
                    .find(|f| f.target_id == frag || f.id == frag);
            }
            if let Some(f) = found {
                self.modal = Some(Modal {
                    title: format!("Footnote {}", f.label),
                    body: if f.plain_text.trim().is_empty() {
                        f.html_content.clone()
                    } else {
                        f.plain_text.clone()
                    },
                    image: None,
                });
                cx.notify();
                return;
            }
        }
        if !path_part.is_empty()
            && let Some(book) = &self.book
            && let Ok(sec) = book.get_section_by_href(path_part)
        {
            let idx = sec.index;
            if idx < book.sections().len() {
                self.current = idx;
                self.load_current(cx);
                cx.notify();
            }
        }
    }

    fn submit_passphrase(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pp = self.passphrase_input.read(cx).value().to_string();
        if pp.trim().is_empty() {
            return;
        }
        if let Some(path) = self.passphrase_pending.clone() {
            self.passphrase_modal = false;
            self.open_file_inner(path, Some(pp), window, cx);
            cx.notify();
        }
    }

    fn cancel_passphrase(&mut self, cx: &mut Context<Self>) {
        self.passphrase_pending = None;
        self.passphrase_error.clear();
        self.passphrase_modal = false;
        cx.notify();
    }

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

        // Display current section page / viewport dimensions if present (e.g. PDF or Fixed-Layout)
        if let Ok(sec) = book.get_section(self.current)
            && let (Some(w), Some(h)) = (sec.viewport_width, sec.viewport_height)
        {
            out.push_str(&format!("Page size: {:.0} × {:.0} pt\n", w, h));
        }

        if let Some(path) = &self.book_path {
            let key = path.to_string_lossy().to_string();
            if let Some(r) = self.state.recent.iter().find(|r| r.path == key) {
                let mins = r.reading_time_secs / 60;
                out.push_str(&format!("Time read: {} min\n", mins));
            }
        }

        // Display EPUB 3 Media Overlays if present
        if !book.media_overlays.is_empty() {
            out.push_str(&format!(
                "Audio read-along (SMIL): {} overlay package(s) available\n",
                book.media_overlays.len()
            ));
        }

        // Display reading flow
        out.push_str(&format!(
            "Reading direction: {}\n",
            if self.rtl_mode {
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
        Some(out.trim_end().to_string())
    }

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
    fn adjust_wpm(&mut self, delta: i32, cx: &mut Context<Self>) {
        let cur = self.state.wpm as i32;
        self.state.wpm = (cur + delta).clamp(80, 800) as u32;
        self.save_state();
        cx.notify();
    }
    fn toggle_zen_mode(&mut self, cx: &mut Context<Self>) {
        self.zen_mode = !self.zen_mode;
        cx.notify();
    }
    fn cycle_font(&mut self, cx: &mut Context<Self>) {
        const FONTS: [&str; 10] = [
            "sans-serif",
            "serif",
            "monospace",
            "system-ui",
            "Georgia",
            "Bookerly",
            "Palatino",
            "OpenDyslexic",
            "Cascadia Code",
            "Courier New",
        ];
        let i = FONTS
            .iter()
            .position(|f| *f == self.font_family)
            .unwrap_or(0);
        self.font_family = FONTS[(i + 1) % FONTS.len()].to_string();
        self.sync_state();
        cx.notify();
    }
    fn cycle_theme(&mut self, cx: &mut Context<Self>) {
        self.theme = match self.theme.as_str() {
            "Light" => "Dark".into(),
            "Dark" => "Sepia".into(),
            "Sepia" => "Nord".into(),
            "Nord" => "Solarized".into(),
            "Solarized" => "Solarized Dark".into(),
            "Solarized Dark" => "E-Ink".into(),
            "E-Ink" => "OLED Black".into(),
            "OLED Black" => "Custom".into(),
            _ => "Light".into(),
        };
        self.dark = matches!(
            self.theme.as_str(),
            "Dark" | "OLED Black" | "Nord" | "Solarized Dark"
        );
        self.sync_state();
        self.load_current(cx);
        cx.notify();
    }
    fn toggle_rtl(&mut self, cx: &mut Context<Self>) {
        self.rtl_mode = !self.rtl_mode;
        cx.notify();
    }
    fn cycle_highlight_color(&mut self, cx: &mut Context<Self>) {
        self.highlight_color = match self.highlight_color.as_str() {
            "yellow" => "green".into(),
            "green" => "blue".into(),
            "blue" => "pink".into(),
            "pink" => "purple".into(),
            _ => "yellow".into(),
        };
        cx.notify();
    }
    fn cycle_book_status(&mut self, path: String, cx: &mut Context<Self>) {
        if let Some(r) = self.state.recent.iter_mut().find(|r| r.path == path) {
            r.status = match r.status.as_str() {
                "reading" => "finished".into(),
                "finished" => "want".into(),
                "want" => "reading".into(),
                _ => "reading".into(),
            };
            self.save_state();
            cx.notify();
        }
    }
    fn show_shortcuts_modal(&mut self, cx: &mut Context<Self>) {
        let help = "\
Keyboard Shortcuts:

Navigation:
  → / PgDown / Space   Next section (or Previous in Manga RTL)
  ← / PgUp / Shift+Spc Previous section (or Next in Manga RTL)
  Alt + ← / Backspace  Jump back to previous location
  Home                 Jump to beginning of book
  End                  Jump to end of book
  Ctrl + O             Open book file
  Ctrl + L             Return to Library
  z                    Toggle Zen / Distraction-free reading mode
  F11                  Toggle Fullscreen
  Esc                  Close dialog / exit Zen mode / return to library

Reading & Display:
  + / =                Increase font size
  - / _                Decrease font size
  m                    Cycle theme (Light, Dark, OLED Black, Sepia, Nord, Solarized, E-Ink)
  r                    Toggle Manga / RTL reading flow
  Ctrl + F             Find in current chapter

Sidebars & Panels:
  t                    Table of Contents sidebar
  s                    Search in book sidebar
  a                    Annotations & Highlights sidebar
  d                    Dictionary & Reference sidebar
  b / Ctrl + B         Toggle Bookmark for current section
  p / Ctrl + P         Toggle Typography & Reading Settings panel
  Ctrl + C             Copy selected text
  ? / F1               This Keyboard Shortcuts cheatsheet";

        self.modal = Some(Modal {
            title: "Keyboard Shortcuts".into(),
            body: help.into(),
            image: None,
        });
        cx.notify();
    }
    fn toggle_two_page(&mut self, cx: &mut Context<Self>) {
        self.two_page = !self.two_page;
        self.sync_state();
        cx.notify();
    }
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
        self.sync_state();
        self.load_current(cx);
        cx.notify();
    }

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

    fn save_as_kfx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = &self.book else {
            return;
        };
        match book.export_kfx_bytes() {
            Ok(bytes) => {
                let name = format!(
                    "{}.kfx",
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
                    .add_filter("Amazon KFX", &["kfx"])
                    .save_file();
                if let Some(path) = chosen {
                    let n = bytes.len();
                    let ok = std::fs::write(&path, &bytes).is_ok();
                    notify_simple(
                        window,
                        cx,
                        if ok { "Saved KFX" } else { "Write failed" },
                        format!("{} ({n} bytes)", path.display()),
                    );
                }
            }
            Err(e) => notify_simple(window, cx, "Export failed", e),
        }
    }

    fn save_as_txt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = &self.book else {
            return;
        };
        let mut full_text = String::new();
        full_text.push_str(&format!("{}\n", book.metadata().title));
        if !book.metadata().creators.is_empty() {
            full_text.push_str(&format!("By {}\n", book.metadata().creators.join(", ")));
        }
        full_text.push_str("\n========================================\n\n");
        for i in 0..book.sections().len() {
            if let Ok(sec) = book.get_section(i) {
                let t = sec.plain_text.trim();
                if !t.is_empty() {
                    full_text.push_str(t);
                    full_text.push_str("\n\n---\n\n");
                }
            }
        }
        let name = format!(
            "{}.txt",
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
            .add_filter("Plain Text", &["txt"])
            .save_file();
        if let Some(path) = chosen {
            let n = full_text.len();
            let ok = std::fs::write(&path, full_text.as_bytes()).is_ok();
            notify_simple(
                window,
                cx,
                if ok {
                    "Saved Plain Text"
                } else {
                    "Write failed"
                },
                format!("{} ({n} bytes)", path.display()),
            );
        }
    }

    fn citations_body(&self) -> Option<String> {
        let book = self.book.as_ref()?;
        let mut out = String::new();
        out.push_str("=== BibTeX ===\n");
        out.push_str(&book.to_bibtex());
        out.push_str("\n\n=== APA (7th ed.) ===\n");
        out.push_str(&book.to_apa());
        out.push_str("\n\n=== MLA (9th ed.) ===\n");
        out.push_str(&book.to_mla());
        out.push_str("\n\n=== Chicago (17th ed.) ===\n");
        out.push_str(&book.to_chicago());
        Some(out)
    }

    fn annotations_to_markdown(&self, title: &str, author: &str) -> String {
        let mut out = String::new();
        out.push_str("---\n");
        out.push_str(&format!("title: \"{}\"\n", title.replace('"', "\\\"")));
        if !author.is_empty() {
            out.push_str(&format!("author: \"{}\"\n", author.replace('"', "\\\"")));
        }
        out.push_str("tags:\n  - reading-notes\n  - eParna\n");
        out.push_str("---\n\n");
        out.push_str(&format!("# Reading Notes: {title}\n\n"));
        if self.annotations.is_empty() {
            out.push_str("_No annotations yet._\n");
            return out;
        }
        for (i, a) in self.annotations.iter().enumerate() {
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

    fn copy_annotations_markdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .book
            .as_ref()
            .map(|b| b.metadata().title.clone())
            .unwrap_or_else(|| "Untitled".into());
        let author = self
            .book
            .as_ref()
            .map(|b| b.metadata().creator().to_string())
            .unwrap_or_default();
        let md = self.annotations_to_markdown(&title, &author);
        cx.write_to_clipboard(ClipboardItem::new_string(md));
        self.selection_status = "Copied notes to clipboard as Markdown".into();
        notify_simple(
            window,
            cx,
            "Copied Notes",
            "Formatted Markdown notes copied to clipboard".into(),
        );
    }

    fn export_annotations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .book
            .as_ref()
            .map(|b| b.metadata().title.clone())
            .unwrap_or_else(|| "Untitled".into());
        let author = self
            .book
            .as_ref()
            .map(|b| b.metadata().creator().to_string())
            .unwrap_or_default();
        let filename = format!(
            "{}_notes.md",
            title
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
            .set_file_name(&filename)
            .add_filter("Markdown", &["md", "markdown"])
            .save_file();
        if let Some(path) = chosen {
            let out = self.annotations_to_markdown(&title, &author);
            let ok = std::fs::write(&path, out).is_ok();
            notify_simple(
                window,
                cx,
                if ok { "Saved Notes" } else { "Export Failed" },
                format!("{}", path.display()),
            );
        }
    }
}

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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

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
                .w(px(108.0))
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
        let bg_in = self.custom_bg_input.read(cx).value().to_string();
        if bg_in != self.custom_bg {
            self.custom_bg = bg_in;
        }
        let fg_in = self.custom_fg_input.read(cx).value().to_string();
        if fg_in != self.custom_fg {
            self.custom_fg = fg_in;
        }
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
            if self.rtl_mode {
                parts.push("RTL flow".into());
            }
            if let Some(page) = current_page_label(b, self.current) {
                parts.push(page);
            }
            let wpm = (self.state.wpm.max(100) as f32).max(100.0);
            let mut sec_words = 0usize;
            if let Ok(sec) = b.get_section(self.current) {
                sec_words = sec.plain_text.split_whitespace().count();
                if sec_words > 0 {
                    let sec_mins = ((sec_words as f32 / wpm).ceil() as usize).max(1);
                    parts.push(format!("{sec_words} words (~{sec_mins}m left)"));
                }
            }
            let total_sections = b.sections().len();
            if total_sections > self.current + 1 && sec_words > 0 {
                let rem_sec = total_sections - (self.current + 1);
                let rem_mins =
                    (((rem_sec * sec_words.max(180)) as f32 / wpm).round() as usize).max(1);
                if rem_mins >= 60 {
                    parts.push(format!("~{}h {}m in book", rem_mins / 60, rem_mins % 60));
                } else {
                    parts.push(format!("~{rem_mins}m in book"));
                }
            }
            let mins = self.reading_start.elapsed().as_secs() / 60;
            if mins > 0 {
                parts.push(format!("{mins} min session"));
            }
            parts.join("  ·  ")
        } else {
            "No book".into()
        };
        let font_size = self.font_size;
        let margin = self.margin;
        let max_width = self.max_width;
        let line_height = self.line_height;
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
                let cfi = b
                    .to_readium_locator(self.current, 0)
                    .ok()
                    .and_then(|l| l.locations.cfi);
                if let Some(cfi) = cfi
                    && b.locations().total_locations > 0
                {
                    (b.locations().percentage_from_cfi(&cfi) * 100.0) as f32
                } else {
                    (self.current as f32 + 1.0) / b.sections().len() as f32 * 100.0
                }
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
                    Button::new("toc-find")
                        .ghost()
                        .label("Find")
                        .on_click(on_toc_search),
                );
                if !self.toc_matches.is_empty() {
                    v = v.child(div().text_size(px(11.0)).child("Search matches:"));
                    for (i, (label, breadcrumb, idx)) in self.toc_matches.iter().enumerate() {
                        let idx = *idx;
                        let mut item = v_flex().gap_1().child(
                            Button::new(format!("tm-{i}"))
                                .ghost()
                                .label(label.clone())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.current = idx;
                                    view.load_current(cx);
                                    cx.notify();
                                })),
                        );
                        if !breadcrumb.is_empty() {
                            item = item.child(
                                div().text_size(px(10.0)).child(format!("in: {breadcrumb}")),
                            );
                        }
                        v = v.child(item);
                    }
                }
                if let Some(state) = &self.toc_state {
                    v = v.child(div().text_size(px(11.0)).child("Quick section:"));
                    v = v.child(Select::new(state));
                }
                if !self.toc_items.is_empty() {
                    v = v.child(div().text_size(px(11.0)).child("Chapters:"));
                    let mut chapter_list = v_flex().gap_1();
                    for (i, (label, idx)) in self.toc_items.iter().take(80).enumerate() {
                        let idx = *idx;
                        let is_active = idx == self.current;
                        let text = if is_active {
                            format!("▶ {}", label.trim())
                        } else {
                            label.clone()
                        };
                        chapter_list = chapter_list.child(
                            Button::new(format!("toc_ch_{i}"))
                                .ghost()
                                .label(truncate(&text, 30))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.jump(idx, cx);
                                })),
                        );
                    }
                    v = v.child(chapter_list);
                }
                v
            }
            SidebarMode::Bookmarks => {
                let mut v = v_flex().gap_2().p_2();
                let is_bm = self.bookmarks.contains(&self.current);
                v = v.child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().text_size(px(14.0)).child("Bookmarks"))
                        .child(
                            Button::new("bm_toggle_sb")
                                .ghost()
                                .label(if is_bm { "− Remove" } else { "+ Add here" })
                                .on_click(cx.listener(|view, _ev, window, cx| {
                                    view.toggle_bookmark(window, cx)
                                })),
                        ),
                );
                if self.bookmarks.is_empty() {
                    v = v.child(
                        div()
                            .text_size(px(12.0))
                            .child("No bookmarks yet in this book."),
                    );
                } else {
                    for &sec in &self.bookmarks {
                        let sec_idx = sec;
                        let is_current = sec_idx == self.current;
                        let label = if let Some((l, _)) =
                            self.toc_items.iter().find(|(_, i)| *i == sec_idx)
                        {
                            format!("Sec {}: {}", sec_idx + 1, truncate(l.trim(), 22))
                        } else {
                            format!("Section {}", sec_idx + 1)
                        };
                        let on_del_bm = cx.listener(move |view, _, window, cx| {
                            if let Some(pos) = view.bookmarks.iter().position(|&i| i == sec_idx) {
                                view.bookmarks.remove(pos);
                                if let Some(path) = &view.book_path {
                                    let key = path.to_string_lossy().to_string();
                                    view.state.bookmarks.insert(key, view.bookmarks.clone());
                                    view.save_state();
                                }
                                view.refresh_bookmark_state(window, cx);
                                cx.notify();
                            }
                        });
                        v = v.child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    Button::new(format!("bmlink_{sec_idx}"))
                                        .ghost()
                                        .label(if is_current {
                                            format!("▶ {label}")
                                        } else {
                                            label
                                        })
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.jump(sec_idx, cx);
                                        })),
                                )
                                .child(
                                    Button::new(format!("bmdel_{sec_idx}"))
                                        .ghost()
                                        .label("\u{2715}")
                                        .on_click(on_del_bm),
                                ),
                        );
                    }
                }
                v
            }
            SidebarMode::Annotations => {
                let mut v = v_flex().gap_2().p_2();
                v = v.child(Input::new(&self.annotation_input));
                let on_copy_md =
                    cx.listener(|view, _ev, window, cx| view.copy_annotations_markdown(window, cx));
                let on_export_file =
                    cx.listener(|view, _ev, window, cx| view.export_annotations(window, cx));
                v = v.child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("addnote")
                                .label("Add note")
                                .on_click(on_add_note),
                        )
                        .child(
                            Button::new("copymd")
                                .ghost()
                                .label("Copy MD")
                                .on_click(on_copy_md),
                        )
                        .child(
                            Button::new("exportmd")
                                .ghost()
                                .label("Export")
                                .on_click(on_export_file),
                        ),
                );
                v = v.child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(div().text_size(px(11.0)).child("Color:"))
                        .child(
                            Button::new("col-y")
                                .ghost()
                                .label(if self.highlight_color == "yellow" {
                                    "● Yellow"
                                } else {
                                    "Yellow"
                                })
                                .on_click(cx.listener(|v, _, _, cx| {
                                    v.highlight_color = "yellow".into();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("col-g")
                                .ghost()
                                .label(if self.highlight_color == "green" {
                                    "● Green"
                                } else {
                                    "Green"
                                })
                                .on_click(cx.listener(|v, _, _, cx| {
                                    v.highlight_color = "green".into();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("col-b")
                                .ghost()
                                .label(if self.highlight_color == "blue" {
                                    "● Blue"
                                } else {
                                    "Blue"
                                })
                                .on_click(cx.listener(|v, _, _, cx| {
                                    v.highlight_color = "blue".into();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("col-p")
                                .ghost()
                                .label(if self.highlight_color == "pink" {
                                    "● Pink"
                                } else {
                                    "Pink"
                                })
                                .on_click(cx.listener(|v, _, _, cx| {
                                    v.highlight_color = "pink".into();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("col-v")
                                .ghost()
                                .label(if self.highlight_color == "purple" {
                                    "● Purple"
                                } else {
                                    "Purple"
                                })
                                .on_click(cx.listener(|v, _, _, cx| {
                                    v.highlight_color = "purple".into();
                                    cx.notify();
                                })),
                        ),
                );
                for (i, a) in self.annotations.iter().enumerate() {
                    let sec = a.section;
                    let summary = a.summary();
                    let kind_badge = if a.is_highlight() { "H" } else { "N" };
                    let on_remove = cx.listener(move |view, _ev, _w, cx| {
                        view.annotations.remove(i);
                        if let Some(path) = &view.book_path {
                            let key = path.to_string_lossy().to_string();
                            view.state.annotations.insert(key, view.annotations.clone());
                            view.save_state();
                        }
                        view.load_current(cx);
                        cx.notify();
                    });
                    v = v.child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(div().text_size(px(10.0)).child(format!("[{kind_badge}]")))
                            .child(
                                Button::new(format!("ann{}", i))
                                    .ghost()
                                    .label(format!("S{}: {}", sec + 1, summary))
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.current = sec;
                                        view.load_current(cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(format!("delann{}", i))
                                    .ghost()
                                    .label("\u{2715}")
                                    .on_click(on_remove),
                            ),
                    );
                }
                v
            }
            SidebarMode::Search => {
                let mut v = v_flex().gap_2().p_2();
                v = v.child(Input::new(&self.search_input));
                let regex_label = if self.search_regex {
                    "Regex: on"
                } else {
                    "Regex: off"
                };
                let on_toggle_regex = cx.listener(|view, _, _, cx| {
                    view.search_regex = !view.search_regex;
                    cx.notify();
                });
                v = v.child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("dosearch").label("Search").on_click(on_search))
                        .child(
                            Button::new("toggleregex")
                                .ghost()
                                .label(regex_label)
                                .on_click(on_toggle_regex),
                        ),
                );
                for (i, r) in self.search_results.iter().take(50).enumerate() {
                    let sec = r.spine_index;
                    let snippet = r.snippet.clone();
                    v = v.child(
                        Button::new(format!("sr{}", i))
                            .ghost()
                            .label(format!("Sec {}: {}", sec + 1, snippet))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.current = sec;
                                view.load_current(cx);
                                cx.notify();
                            })),
                    );
                }
                v
            }
            SidebarMode::Footnotes => {
                let mut v = v_flex().gap_2().p_2();
                v = v.child(div().text_size(px(14.0)).child("Footnotes"));
                if let Some(book) = &self.book
                    && let Ok(sec) = book.get_section(self.current)
                {
                    let notes = sec.extract_footnotes();
                    if notes.is_empty() {
                        v = v.child(div().text_size(px(12.0)).child("No footnotes in section."));
                    } else {
                        for (i, f) in notes.into_iter().enumerate() {
                            let text = if f.plain_text.trim().is_empty() {
                                f.html_content.clone()
                            } else {
                                f.plain_text.clone()
                            };
                            let text_clone = text.clone();
                            let label = f.label.clone();
                            v = v.child(
                                Button::new(format!("fn{}", i))
                                    .ghost()
                                    .label(format!("{}: {}", label, text))
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.show_modal(
                                            &format!("Footnote {label}"),
                                            text_clone.clone(),
                                            cx,
                                        );
                                    })),
                            );
                        }
                    }
                }
                v
            }
            SidebarMode::Dictionary => {
                let mut v = v_flex().gap_2().p_2();
                v = v.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().text_size(px(14.0)).child(if self.stardict.is_some() {
                            format!("Dict: {}", self.stardict_name)
                        } else {
                            "Dictionary".into()
                        }))
                        .child(
                            Button::new("load_stardict")
                                .ghost()
                                .label("Load .ifo\u{2026}")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("StarDict .ifo", &["ifo"])
                                        .pick_file()
                                    {
                                        match StarDict::load(&path) {
                                            Ok(sd) => {
                                                view.stardict_name = path
                                                    .file_stem()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or("stardict")
                                                    .to_string();
                                                view.stardict = Some(sd);
                                                notify_simple(
                                                    window,
                                                    cx,
                                                    "Dictionary loaded",
                                                    format!("Loaded {}", view.stardict_name),
                                                );
                                            }
                                            Err(e) => {
                                                notify_simple(window, cx, "Dict load failed", e);
                                            }
                                        }
                                        cx.notify();
                                    }
                                })),
                        ),
                );
                v = v.child(Input::new(&self.dictionary_input));
                let on_lookup = cx.listener(|view, _ev, _w, cx| {
                    let word = view.dictionary_input.read(cx).value().to_string();
                    view.lookup_word(&word, cx);
                });
                let on_lookup_sel = cx.listener(|view, _ev, window, cx| {
                    let word = Self::selected_text(window, cx);
                    if word.trim().is_empty() {
                        view.dictionary_result = "Select a word first".into();
                        cx.notify();
                        return;
                    }
                    view.dictionary_input.update(cx, |s, cx| {
                        s.set_value(word.clone(), window, cx);
                    });
                    view.lookup_word(word.trim(), cx);
                });
                v = v.child(
                    Button::new("lookup-sel")
                        .ghost()
                        .label("Lookup selection")
                        .on_click(on_lookup_sel),
                );
                let on_wiki = cx.listener(|view, _ev, _w, cx| {
                    let word = view.dictionary_input.read(cx).value().to_string();
                    let word = word.trim();
                    if word.is_empty() {
                        return;
                    }
                    view.dictionary_result = format!("Searching Wikipedia for “{}”...", word);
                    cx.notify();
                    let query = word.replace(' ', "_");
                    cx.spawn(async move |view, cx| {
                        let url = format!(
                            "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
                            query
                        );
                        let res = match ureq::get(&url)
                            .header(
                                "User-Agent",
                                "eParna-Reader/0.7.5 (https://github.com/SV-stark/eParna)",
                            )
                            .call()
                        {
                            Ok(resp) => {
                                let body = resp.into_body().read_to_string().unwrap_or_default();
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                                    if let Some(extract) =
                                        json.get("extract").and_then(|v| v.as_str())
                                    {
                                        extract.to_string()
                                    } else {
                                        "No summary found on Wikipedia".into()
                                    }
                                } else {
                                    "No summary found on Wikipedia".into()
                                }
                            }
                            Err(_) => {
                                "Wikipedia lookup failed (network error or page not found)".into()
                            }
                        };
                        let _ = view.update(cx, |view, cx| {
                            view.dictionary_result = res;
                            cx.notify();
                        });
                    })
                    .detach();
                });
                let on_translate = cx.listener(|view, _ev, _w, cx| {
                    let text = view.dictionary_input.read(cx).value().to_string();
                    let text = text.trim();
                    if text.is_empty() {
                        return;
                    }
                    view.dictionary_result = format!("Translating “{}”...", text);
                    cx.notify();
                    let encoded = url_encode(text);
                    cx.spawn(async move |view, cx| {
                        let url = format!(
                            "https://api.mymemory.translated.net/get?q={}&langpair=autodetect|en",
                            encoded
                        );
                        let res = match ureq::get(&url)
                            .header(
                                "User-Agent",
                                "eParna-Reader/0.7.5 (https://github.com/SV-stark/eParna)",
                            )
                            .call()
                        {
                            Ok(resp) => {
                                let body = resp.into_body().read_to_string().unwrap_or_default();
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                                    if let Some(trans) = json
                                        .get("responseData")
                                        .and_then(|d| d.get("translatedText"))
                                        .and_then(|t| t.as_str())
                                    {
                                        format!("Translation (en):\n{}", trans)
                                    } else {
                                        "Could not parse translation response".into()
                                    }
                                } else {
                                    "Invalid translation response".into()
                                }
                            }
                            Err(e) => format!("Translation failed: {e}"),
                        };
                        let _ = view.update(cx, |view, cx| {
                            view.dictionary_result = res;
                            cx.notify();
                        });
                    })
                    .detach();
                });
                v = v.child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("lookup").label("Lookup").on_click(on_lookup))
                        .child(Button::new("wiki").label("Wikipedia").on_click(on_wiki))
                        .child(
                            Button::new("translate")
                                .label("Translate")
                                .on_click(on_translate),
                        ),
                );
                v = v.child(
                    div()
                        .text_size(px(12.0))
                        .child(self.dictionary_result.clone()),
                );
                v
            }
        };

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

        let is_bm = self.bookmarks.contains(&self.current);
        let bm_label = if is_bm {
            "★ Bookmarked"
        } else {
            "☆ Bookmark"
        };
        let back_btn = if let Some(last_sec) = self.nav_history.last() {
            let last_sec = *last_sec;
            Some(
                Button::new("jump_back")
                    .ghost()
                    .label(format!("⤺ Sec {}", last_sec + 1))
                    .on_click(cx.listener(|v, _, _, cx| v.jump_back(cx)))
                    .into_any_element(),
            )
        } else {
            None
        };

        let mut left_controls = h_flex()
            .flex_grow(1.0)
            .gap_2()
            .items_center()
            .child(Button::new("open").label("Open").on_click(on_open))
            .child(Button::new("library").label("Library").on_click(on_lib));

        if let Some(btn) = back_btn {
            left_controls = left_controls.child(btn);
        }

        left_controls = left_controls
            .child(
                Button::new("prev")
                    .ghost()
                    .label(if self.rtl_mode {
                        "\u{203a}"
                    } else {
                        "\u{2039}"
                    })
                    .on_click(on_prev),
            )
            .child(
                Button::new("next")
                    .ghost()
                    .label(if self.rtl_mode {
                        "\u{2039}"
                    } else {
                        "\u{203a}"
                    })
                    .on_click(on_next),
            )
            .child(Button::new("bm").label(bm_label).on_click(on_bm))
            .child(
                Button::new("highlight")
                    .label("Highlight")
                    .on_click(on_highlight),
            )
            .child(
                Button::new("hl-col")
                    .ghost()
                    .label(format!("[{}]", self.highlight_color))
                    .on_click(cx.listener(|v, _, _, cx| v.cycle_highlight_color(cx))),
            )
            .child(Button::new("copy").label("Copy").on_click(on_copy))
            .child(
                Button::new("find-btn")
                    .ghost()
                    .label("Find")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.show_find_bar = !v.show_find_bar;
                        cx.notify();
                    })),
            );

        let header = h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .px_2()
            .py_2()
            .border_b_1()
            .child(left_controls)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .child(format!("{}  ({:.0}%)", pos, progress)),
                    )
                    .child(
                        Button::new("zen-hdr")
                            .ghost()
                            .label("Zen")
                            .on_click(cx.listener(|view, _, _, cx| view.toggle_zen_mode(cx))),
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
                    .child(
                        Button::new("more")
                            .ghost()
                            .label("\u{22ef}")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.panel = if view.panel == Panel::Actions {
                                    Panel::None
                                } else {
                                    Panel::Actions
                                };
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("shortcuts-btn")
                            .ghost()
                            .label("?")
                            .on_click(cx.listener(|view, _, _, cx| view.show_shortcuts_modal(cx))),
                    ),
            );

        let mut typography_panel = h_flex()
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
                "Align",
                if self.justify { "Justify" } else { "Left" },
                "al",
                cx.listener(|v, _, _, cx| {
                    v.justify = !v.justify;
                    v.sync_state();
                    v.load_current(cx);
                    cx.notify();
                }),
            ))
            .child(cycle_row(
                "Pages",
                if self.two_page { "two" } else { "one" },
                "pg",
                cx.listener(|v, _, _, cx| v.toggle_two_page(cx)),
            ))
            .child(setting_row(
                "Speed",
                format!("{} wpm", self.state.wpm),
                Button::new("wpm-dec")
                    .ghost()
                    .label("\u{2212}")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_wpm(-20, cx)))
                    .into_any_element(),
                Button::new("wpm-inc")
                    .ghost()
                    .label("+")
                    .on_click(cx.listener(|v, _, _, cx| v.adjust_wpm(20, cx)))
                    .into_any_element(),
            ))
            .child(
                Button::new("reset-display")
                    .ghost()
                    .label("Reset to defaults")
                    .on_click(cx.listener(|v, _, _, cx| v.reset_display(cx))),
            );

        if self.theme == "Custom" {
            typography_panel = typography_panel.child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(div().w(px(96.0)).text_size(px(12.0)).child("Custom bg"))
                    .child(Input::new(&self.custom_bg_input).w(px(120.0)))
                    .child(div().w(px(96.0)).text_size(px(12.0)).child("Custom fg"))
                    .child(Input::new(&self.custom_fg_input).w(px(120.0))),
            );
        }

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
                Button::new("savekfx")
                    .ghost()
                    .label("Save as KFX\u{2026}")
                    .on_click(cx.listener(|v, _, window, cx| v.save_as_kfx(window, cx))),
            )
            .child(
                Button::new("savetxt")
                    .ghost()
                    .label("Save as TXT\u{2026}")
                    .on_click(cx.listener(|v, _, window, cx| v.save_as_txt(window, cx))),
            )
            .child(
                Button::new("citations")
                    .ghost()
                    .label("Citations\u{2026}")
                    .on_click(cx.listener(|v, _, _, cx| {
                        if let Some(body) = v.citations_body() {
                            v.show_modal("Citations (BibTeX, APA, MLA, Chicago)", body, cx);
                        }
                    })),
            )
            .child(
                Button::new("export")
                    .ghost()
                    .label("Export notes\u{2026}")
                    .on_click(cx.listener(|v, _, window, cx| v.export_annotations(window, cx))),
            )
            .child(cycle_row(
                "Reading Flow",
                if self.rtl_mode {
                    "RTL (Right-to-Left)"
                } else {
                    "LTR (Left-to-Right)"
                },
                "flow",
                cx.listener(|v, _, _, cx| {
                    v.rtl_mode = !v.rtl_mode;
                    cx.notify();
                }),
            ))
            .child(
                Button::new("sc-modal")
                    .ghost()
                    .label("Shortcuts (?)")
                    .on_click(cx.listener(|v, _, _, cx| v.show_shortcuts_modal(cx))),
            )
            .child(
                Button::new("zen-btn")
                    .ghost()
                    .label("Zen mode (Z)")
                    .on_click(cx.listener(|v, _, _, cx| v.toggle_zen_mode(cx))),
            )
            .child(div().w(px(1.0)).h(px(18.0)).border_l_1())
            .child(Input::new(&self.goto_input).w(px(100.0)))
            .child(
                Button::new("go")
                    .ghost()
                    .label("Go to sec/%")
                    .on_click(cx.listener(|v, _ev, _w, cx| {
                        let s = v.goto_input.read(cx).value().trim().to_string();
                        if s.ends_with('%') {
                            if let Ok(pct) = s.trim_end_matches('%').trim().parse::<f32>()
                                && let Some(book) = &v.book
                            {
                                let total = book.sections().len();
                                if total > 0 {
                                    let idx = ((pct / 100.0) * (total as f32)).round() as usize;
                                    v.jump(idx.min(total - 1), cx);
                                }
                            }
                        } else if let Ok(n) = s.parse::<usize>() {
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

        // In-page search bar
        let find_bar = if self.show_find_bar {
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .child(div().text_size(px(12.0)).child("Find in chapter:"))
                .child(Input::new(&self.find_in_page_input).w(px(220.0)))
                .child(
                    Button::new("find-hl")
                        .label("Highlight")
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.active_search_query =
                                v.find_in_page_input.read(cx).value().to_string();
                            v.load_current(cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("find-clear")
                        .ghost()
                        .label("Clear")
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.active_search_query.clear();
                            v.load_current(cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("find-close")
                        .ghost()
                        .label("\u{2715}")
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.show_find_bar = false;
                            v.active_search_query.clear();
                            v.load_current(cx);
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let secondary: AnyElement = match self.panel {
            Panel::Typography => typography_panel.into_any_element(),
            Panel::Actions => actions_panel.into_any_element(),
            Panel::None => div().into_any_element(),
        };

        let html2 = if self.two_page {
            if let Some(book) = &self.book {
                book.get_section(self.current + 1)
                    .map(|s| {
                        let current_annotations: Vec<Annotation> = self
                            .annotations
                            .iter()
                            .filter(|a| a.section == self.current + 1)
                            .cloned()
                            .collect();
                        let active_search = if self.active_search_query.trim().is_empty() {
                            None
                        } else {
                            Some(self.active_search_query.as_str())
                        };
                        let prepared = prepare_section_html(
                            &s.processed_html,
                            book,
                            &s.full_path,
                            &current_annotations,
                            active_search,
                            &self.theme,
                            self.justify,
                        );
                        constrain_images(&prepared, column_px)
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let view = cx.entity().clone();
        let page = |id: String, h: String| {
            let page_div = div()
                .min_w(px(0.0))
                .max_w(px(max_width))
                .w_full()
                .h_full()
                .px(px(margin))
                .text_size(px(font_size))
                .line_height(px(font_size * line_height))
                .font_family(self.font_family.clone())
                .text_align(TextAlign::Left)
                .overflow_hidden();
            page_div.child(
                TextView::html(id.clone(), h)
                    .selectable(true)
                    .scrollable(true)
                    .on_link_click({
                        let view = view.clone();
                        move |url, _event, _window, cx| {
                            let url = url.to_string();
                            view.update(cx, |v, cx| v.handle_link_click(&url, cx));
                        }
                    }),
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

        let body = if self.zen_mode {
            h_flex().size_full().min_w(px(0.0)).child(content)
        } else {
            h_flex()
                .size_full()
                .min_w(px(0.0))
                .child(sidebar_panel)
                .child(content)
        };

        let on_key = cx.listener(|view, event: &KeyDownEvent, window, cx| {
            let key = event.keystroke.key.as_str();
            let ctrl = event.keystroke.modifiers.control;
            let alt = event.keystroke.modifiers.alt;

            if key == "escape" {
                if view.zen_mode {
                    view.zen_mode = false;
                    cx.notify();
                } else if view.show_find_bar {
                    view.show_find_bar = false;
                    view.active_search_query.clear();
                    view.load_current(cx);
                    cx.notify();
                } else if view.modal.is_some() {
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
                                    "odt", "rtf", "cbz", "lit", "kfx", "kepub",
                                ],
                            )
                            .pick_file()
                        {
                            view.open_file(path, window, cx);
                        }
                    }
                    "f" => {
                        view.show_find_bar = true;
                        cx.notify();
                    }
                    "b" => view.toggle_bookmark(window, cx),
                    "c" => view.copy_selection(window, cx),
                    "p" => {
                        view.panel = if view.panel == Panel::Typography {
                            Panel::None
                        } else {
                            Panel::Typography
                        };
                        cx.notify();
                    }
                    "l" => {
                        window.set_window_title("eParna");
                        view.go_library(cx);
                    }
                    _ => {}
                }
            } else if alt {
                if key == "ArrowLeft" || key == "Left" {
                    view.jump_back(cx);
                }
            } else {
                match key {
                    "ArrowRight" | "PageDown" | "j" => view.next(cx),
                    "ArrowLeft" | "PageUp" | "k" => view.prev(cx),
                    "Backspace" if !view.show_library => view.jump_back(cx),
                    "z" if !view.show_library => view.toggle_zen_mode(cx),
                    "p" if !view.show_library => {
                        view.panel = if view.panel == Panel::Typography {
                            Panel::None
                        } else {
                            Panel::Typography
                        };
                        cx.notify();
                    }
                    " " => {
                        if event.keystroke.modifiers.shift {
                            view.prev(cx);
                        } else {
                            view.next(cx);
                        }
                    }
                    "Home" => view.jump(0, cx),
                    "End" => {
                        if let Some(b) = &view.book {
                            let last = b.sections().len().saturating_sub(1);
                            view.jump(last, cx);
                        }
                    }
                    "F11" => view.toggle_fullscreen(window, cx),
                    "?" | "F1" => view.show_shortcuts_modal(cx),
                    "+" | "=" => view.adjust_font_size(1.0, cx),
                    "-" | "_" => view.adjust_font_size(-1.0, cx),
                    "m" if !view.show_library => view.cycle_theme(cx),
                    "r" if !view.show_library => view.toggle_rtl(cx),
                    "t" if !view.show_library => {
                        view.sidebar_mode = SidebarMode::Contents;
                        cx.notify();
                    }
                    "s" if !view.show_library => {
                        view.sidebar_mode = SidebarMode::Search;
                        cx.notify();
                    }
                    "a" if !view.show_library => {
                        view.sidebar_mode = SidebarMode::Annotations;
                        cx.notify();
                    }
                    "d" if !view.show_library => {
                        view.sidebar_mode = SidebarMode::Dictionary;
                        cx.notify();
                    }
                    "b" if !view.show_library => view.toggle_bookmark(window, cx),
                    _ => {}
                }
            }
        });

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
                        .child(if let Some(src) = &m.image {
                            div()
                                .flex_1()
                                .max_h(px(440.0))
                                .child(
                                    TextView::html(
                                        "modal-image",
                                        format!("<img src='{}' style='max-width:100%' />", src),
                                    )
                                    .scrollable(true),
                                )
                                .into_any_element()
                        } else {
                            div()
                                .flex_1()
                                .max_h(px(440.0))
                                .text_size(px(13.0))
                                .child(
                                    TextView::html("modal-body", escape_html(&m.body))
                                        .scrollable(true),
                                )
                                .into_any_element()
                        })
                        .child(
                            h_flex().justify_end().child(
                                Button::new("modal-close")
                                    .label("Close")
                                    .on_click(cx.listener(|v, _, _, cx| v.close_modal(cx))),
                            ),
                        ),
                )
        } else if self.passphrase_modal {
            v_flex()
                .flex_1()
                .size_full()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    v_flex()
                        .w(px(420.0))
                        .gap_3()
                        .border_1()
                        .p_4()
                        .child(div().text_size(px(16.0)).child("Passphrase required"))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .child(self.passphrase_error.clone()),
                        )
                        .child(Input::new(&self.passphrase_input))
                        .child(
                            h_flex().justify_end().gap_2().children([
                                Button::new("pp-cancel")
                                    .label("Cancel")
                                    .on_click(cx.listener(|v, _, _, cx| v.cancel_passphrase(cx))),
                                Button::new("pp-submit").label("Open").on_click(
                                    cx.listener(|v, _, window, cx| v.submit_passphrase(window, cx)),
                                ),
                            ]),
                        ),
                )
        } else {
            body
        };

        let root = if self.zen_mode && self.modal.is_none() && !self.passphrase_modal {
            v_flex()
                .size_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .bg(bg)
                .text_color(fg)
                .on_key_down(on_key)
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .px_4()
                        .py_1()
                        .child(div().text_size(px(11.0)).child(format!("{:.0}%", progress)))
                        .child(
                            Button::new("exit-zen")
                                .ghost()
                                .label("Exit Zen (Esc / Z)")
                                .on_click(cx.listener(|v, _, _, cx| v.toggle_zen_mode(cx))),
                        ),
                )
                .child(div().flex_1().size_full().overflow_hidden().child(body))
        } else {
            v_flex()
                .size_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .bg(bg)
                .text_color(fg)
                .on_key_down(on_key)
                .child(header)
                .child(secondary)
                .child(find_bar)
                .child(div().px_2().child(Progress::new("prog").value(progress)))
                .child(div().px_2().text_size(px(12.0)).child(
                    if self.selection_status.is_empty() {
                        status.clone()
                    } else {
                        format!("{} — {}", status, self.selection_status)
                    },
                ))
                .child(body)
        };

        root.into_any_element()
    }
}

fn window_options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(60.0), px(40.0)),
            size: size(px(1000.0), px(600.0)),
        })),
        window_min_size: Some(size(px(420.0), px(320.0))),
        ..Default::default()
    }
}

fn main() {
    let worker = std::thread::Builder::new()
        .name("eparna-main".into())
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
