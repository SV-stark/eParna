mod config;
mod export;
mod html_utils;
mod library;
mod models;
mod network;
mod renderer;
mod stardict;
mod ui_helpers;

use base64::Engine as _;
use ebook_rs::SearchResult;
use gpui_kit::base::{TextSelection, h_flex, v_flex};
use gpui_kit::component::{
    Root,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    progress::Progress,
    scroll::{ScrollableElement, Scrollbar},
    select::{Select, SelectEvent, SelectState},
    text::TextView,
    theme::{Theme, ThemeMode},
};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::config::{load_state, save_state};
use crate::export::{
    export_book_bytes, format_annotations_markdown, format_book_info, format_citations,
    format_validation, optimize_book_epub3, sanitize_filename,
};
use crate::html_utils::*;
use crate::library::*;
use crate::models::*;
use crate::network::*;
use crate::renderer::{
    ImageCache, ProgressScrubber, RenderConfig, is_palette_dark, render_ebook_dom,
};
use crate::stardict::StarDict;
use crate::ui_helpers::*;

fn pick_ebook_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Ebook", SUPPORTED_EXTENSIONS)
        .pick_file()
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

    toc_items: Vec<TocItem>,
    toc_state: Option<Entity<SelectState<Vec<String>>>>,
    active_toc_index: Option<usize>,

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
    text_align: String,
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
    covers_backfilled: bool,

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

    // Per-section / per-column cached render state
    cached_word_count: usize,
    cached_progress: f32,
    section_start_pct: f32,
    section_end_pct: f32,
    chapter_ticks: Vec<f32>,
    scroll_handle: ScrollHandle,
    cached_constrained_html: String,
    cached_column_px: f32,
    cached_dom: Option<ebook_rs::EbookDomTree>,
    cached_dom2: Option<(usize, ebook_rs::EbookDomTree)>,
    page1_selection: Option<gpui_kit::base::TextSelectionHandle>,
    page2_selection: Option<gpui_kit::base::TextSelectionHandle>,
    image_cache: ImageCache,

    // Resizable & collapsible sidebar, export format picker, synced gpui-kit theme mode
    sidebar_open: bool,
    sidebar_width: f32,
    export_modal: bool,
    synced_dark_mode: Option<bool>,
    focus_handle: FocusHandle,
    tts_engine: Option<tts::Tts>,
    tts_speaking: bool,
    tts_rate: f32,
    tts_text: String,
}

impl ReaderView {
    fn cache_cover(&self, book: &ebook_rs::Book, path: &str) -> String {
        cache_cover_to_disk(book, path)
    }

    fn load_covers(&mut self) {
        if !self.covers_backfilled {
            self.covers_backfilled = true;
            let mut updated = false;
            for r in &mut self.state.recent {
                if !r.cover.is_empty() {
                    continue;
                }
                let book_path = PathBuf::from(&r.path);
                if book_path.exists()
                    && let Ok(book) = ebook_rs::Book::from_file(&book_path)
                {
                    let c = cache_cover_to_disk(&book, &r.path);
                    if !c.is_empty() {
                        r.cover = c;
                        updated = true;
                    }
                }
            }
            if updated {
                self.save_state();
            }
        }
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
                "svg" => "image/svg+xml",
                _ => "image/png",
            };
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            self.covers
                .insert(r.cover.clone(), format!("data:{};base64,{}", mime, b64));
        }
    }

    fn theme_palette(&self) -> ThemePalette {
        theme_palette_for(&self.theme, &self.custom_bg, &self.custom_fg)
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
        self.state.text_align = self.text_align.clone();
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
            text_align: st.text_align.clone(),
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
            active_toc_index: None,
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
            covers_backfilled: false,
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
            cached_word_count: 0,
            cached_progress: 0.0,
            section_start_pct: 0.0,
            section_end_pct: 100.0,
            chapter_ticks: Vec::new(),
            scroll_handle: ScrollHandle::default(),
            cached_constrained_html: String::new(),
            cached_column_px: 0.0,
            cached_dom: None,
            cached_dom2: None,
            page1_selection: Some(gpui_kit::base::TextSelectionHandle::new("", cx)),
            page2_selection: Some(gpui_kit::base::TextSelectionHandle::new("", cx)),
            image_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            sidebar_open: false,
            sidebar_width: 260.0,
            export_modal: false,
            synced_dark_mode: None,
            focus_handle: cx.focus_handle(),
            tts_engine: None,
            tts_speaking: false,
            tts_rate: 1.0,
            tts_text: String::new(),
        };

        view.focus_handle.focus(window, cx);

        if view.show_library {
            view.load_covers();
        }
        if let Some(arg) = std::env::args().nth(1) {
            view.open_file(PathBuf::from(arg), window, cx);
        }
        view
    }

    fn scan_folder(&mut self, folder: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let existing_paths: HashSet<String> =
            self.state.recent.iter().map(|r| r.path.clone()).collect();
        let existing_hashes: HashSet<String> = self
            .state
            .recent
            .iter()
            .filter(|r| !r.content_hash.is_empty())
            .map(|r| r.content_hash.clone())
            .collect();

        notify_simple(
            window,
            cx,
            "Scanning Folder",
            format!("Scanning {} in the background...", folder.display()),
        );

        let bg = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, cx| {
            let scanned = bg
                .spawn(async move { scan_folder_books(folder, existing_paths, existing_hashes) })
                .await;
            let _ = view.update_in(cx, |view, window, cx| {
                let added = merge_scanned_books(&mut view.state.recent, scanned);
                if added > 0 {
                    view.save_state();
                    view.load_covers();
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
            });
        })
        .detach();
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
        let book_result: Result<ebook_rs::Book, ebook_rs::EbookError> =
            if let Some(pp) = &passphrase {
                std::fs::read(&path)
                    .map_err(ebook_rs::EbookError::from)
                    .and_then(|bytes| {
                        let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("eBook");
                        ebook_rs::Book::from_bytes_with_passphrase(&bytes, title, pp)
                    })
            } else {
                ebook_rs::Book::from_file(&path)
            };
        match book_result {
            Ok(mut book) => {
                // Deliver merged book CSS and inline matching <style> rules onto elements
                // so stripping <head> in `prepare_section_html` preserves publisher styling.
                book.layout_mut().asset_delivery = ebook_rs::AssetDeliveryStrategy::InlinedStyle;
                self.focus_handle.focus(window, cx);
                if book.locations().total_locations == 0 {
                    book.generate_locations(1000);
                }
                let title = book.metadata().title.clone();
                window.set_window_title(&format!("{} — eParna", title));

                // Detect Manga / Comic Right-to-Left (RTL) reading flow
                self.rtl_mode =
                    book.metadata().direction == ebook_rs::PageProgressionDirection::Rtl;

                let author = book.metadata().creator().to_string();
                let format = detect_format_label(&path);
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

                let items = build_toc_items(&book);
                let labels: Vec<String> = items.iter().map(TocItem::select_label).collect();
                let toc_state = cx.new(|cx| SelectState::new(labels, None, window, cx));
                let toc_sub =
                    cx.subscribe(&toc_state, |view, _state, event: &SelectEvent<_>, cx| {
                        if let SelectEvent::Confirm(Some(label)) = event
                            && let Some((toc_idx, item)) = view
                                .toc_items
                                .iter()
                                .enumerate()
                                .find(|(_, item)| item.select_label() == *label)
                        {
                            let section_idx = item.section_index;
                            view.jump(section_idx, cx);
                            view.active_toc_index = Some(toc_idx);
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
                self.active_toc_index = None;
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
                let msg = e.to_string();
                match e {
                    ebook_rs::EbookError::DrmProtected(_) if passphrase.is_none() => {
                        self.passphrase_pending = Some(path.clone());
                        self.passphrase_error = msg;
                        self.passphrase_modal = true;
                    }
                    _ if passphrase.is_some() => {
                        self.passphrase_error = msg;
                        self.passphrase_modal = true;
                    }
                    _ => {
                        notify_simple(window, cx, "Open failed", msg);
                    }
                }
            }
        }
        cx.notify();
    }

    fn jump_to_percentage(&mut self, pct: f32, cx: &mut Context<Self>) {
        let clamped = pct.clamp(0.0, 100.0);
        let span = self.section_end_pct - self.section_start_pct;
        if span > 0.05 && clamped >= self.section_start_pct && clamped <= self.section_end_pct {
            let frac = ((clamped - self.section_start_pct) / span).clamp(0.0, 1.0);
            let max_y: f32 = self.scroll_handle.max_offset().y.into();
            if max_y > 1.0 {
                self.scroll_handle
                    .set_offset(point(px(0.0), px(-(frac * max_y))));
                cx.notify();
                return;
            }
        }
        let target_idx = if let Some(book) = &self.book {
            let total = book.sections().len();
            if total == 0 {
                return;
            }
            let by_cfi = if book.locations().total_locations > 0 {
                let target_loc =
                    ((clamped / 100.0) * book.locations().total_locations as f32).round() as usize;
                book.locations()
                    .cfi_from_location(target_loc)
                    .and_then(|cfi| book.get_section_and_offset_by_cfi(&cfi).ok())
                    .map(|(sec, _)| sec.index)
            } else {
                None
            };
            by_cfi
                .unwrap_or_else(|| {
                    (((clamped / 100.0) * total as f32).floor() as usize).min(total - 1)
                })
                .min(total - 1)
        } else {
            return;
        };
        self.jump(target_idx, cx);
    }

    fn load_current(&mut self, cx: &mut Context<Self>) {
        self.stop_tts();
        self.tts_text.clear();
        self.cached_word_count = 0;
        self.cached_progress = 0.0;
        self.section_start_pct = 0.0;
        self.section_end_pct = 100.0;
        self.chapter_ticks.clear();
        self.scroll_handle.set_offset(point(px(0.0), px(0.0)));
        self.cached_dom = None;
        self.cached_dom2 = None;
        self.page1_selection = Some(gpui_kit::base::TextSelectionHandle::new("", cx));
        self.page2_selection = Some(gpui_kit::base::TextSelectionHandle::new("", cx));
        let column_px = (self.max_width - 2.0 * self.margin).max(160.0);
        if let Some(book) = &self.book {
            match book.get_section(self.current) {
                Ok(sec) => {
                    self.cached_word_count = sec.plain_text.split_whitespace().count();
                    self.tts_text = sec.plain_text.clone();
                    let inlined_svg = inline_svg_images(&sec.processed_html, book, &sec.full_path);
                    self.cached_dom = Some(ebook_rs::EbookDomTree::parse(&inlined_svg));
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
                    let err_html = format!("<p>Error: {}</p>", e);
                    self.cached_dom = Some(ebook_rs::EbookDomTree::parse(&err_html));
                    self.html = err_html;
                }
            }
            if self.two_page
                && let Ok(sec2) = book.get_section(self.current + 1)
            {
                let inlined2 = inline_svg_images(&sec2.processed_html, book, &sec2.full_path);
                self.cached_dom2 =
                    Some((self.current + 1, ebook_rs::EbookDomTree::parse(&inlined2)));
            }
            if !book.sections().is_empty() {
                let total = book.sections().len();
                let has_locs = book.locations().total_locations > 0;
                let pct_for_sec = |idx: usize| -> f32 {
                    if idx >= total {
                        return 100.0;
                    }
                    if has_locs
                        && let Ok(loc) = book.to_readium_locator(idx, 0)
                        && let Some(cfi) = loc.locations.cfi
                    {
                        return (book.locations().percentage_from_cfi(&cfi) * 100.0) as f32;
                    }
                    (idx as f32 / total as f32) * 100.0
                };
                self.section_start_pct = pct_for_sec(self.current);
                self.section_end_pct = pct_for_sec(self.current + 1).max(self.section_start_pct);
                self.cached_progress = if has_locs {
                    self.section_start_pct
                } else {
                    (self.current as f32 + 1.0) / total as f32 * 100.0
                };

                // Build chapter boundary ticks (capped to keep dense books clean)
                let step = (total / 40).max(1);
                self.chapter_ticks = (1..total)
                    .step_by(step)
                    .map(|idx| (pct_for_sec(idx) / 100.0).clamp(0.0, 1.0))
                    .collect();
            }
        }
        self.cached_column_px = column_px;
        self.cached_constrained_html = constrain_images(&self.html, column_px);
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
            self.active_toc_index = None;
            self.load_current(cx);
            cx.notify();
        }
    }

    fn prev_section(&mut self, cx: &mut Context<Self>) {
        if self.current > 0 {
            self.current -= 1;
            self.active_toc_index = None;
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
            self.active_toc_index = None;
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
            self.active_toc_index = None;
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
        let bg = cx.background_executor().clone();
        cx.spawn(async move |view, cx| {
            let res = bg
                .spawn(async move { fetch_dictionary_definition(&word_owned) })
                .await;
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
        self.stop_tts();
        self.show_library = true;
        self.load_covers();
        self.sync_state();
        cx.notify();
    }

    fn render_library(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
        palette: ThemePalette,
    ) -> AnyElement {
        let on_open = cx.listener(|view, _ev, window, cx| {
            if let Some(path) = pick_ebook_file() {
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

        let shelf_btn = |id: &str, label: &str, idx: usize, active: usize| {
            let b = Button::new(id.to_string()).label(label.to_string());
            let b = if active == idx {
                b.primary()
            } else {
                b.ghost()
            };
            b.on_click(cx.listener(move |v, _, _, cx| {
                v.library_shelf_filter = idx;
                cx.notify();
            }))
        };

        let header =
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .gap_3()
                .pb_2()
                .border_b_1()
                .border_color(palette.border)
                .child(
                    h_flex()
                        .id("lib-about")
                        .cursor_pointer()
                        .items_center()
                        .gap_2()
                        .on_click(cx.listener(|v, _, _, cx| v.show_about_modal(cx)))
                        .child(div().w(px(32.0)).h(px(32.0)).overflow_hidden().child(
                            TextView::html(
                                "app-logo",
                                format!(
                                    "<img src=\"{}\" width=\"32\" height=\"32\"/>",
                                    app_icon_data_uri()
                                ),
                            ),
                        ))
                        .child(
                            div()
                                .text_size(px(20.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("eParna Library"),
                        ),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("openlib")
                                .primary()
                                .label("Open file")
                                .on_click(on_open),
                        )
                        .child(
                            Button::new("scanlib")
                                .outline()
                                .label("Scan folder")
                                .on_click(on_scan),
                        )
                        .child(
                            Button::new("sortlib")
                                .ghost()
                                .label(sort_label)
                                .on_click(on_sort),
                        )
                        .child(
                            Button::new("viewmode")
                                .ghost()
                                .label(if is_list_view {
                                    "View: List"
                                } else {
                                    "View: Grid"
                                })
                                .on_click(on_toggle_view),
                        ),
                );

        let shelves = h_flex()
            .gap_2()
            .items_center()
            .flex_wrap()
            .child(shelf_btn("shelf_all", "All", 0, self.library_shelf_filter))
            .child(shelf_btn(
                "shelf_reading",
                "Reading",
                1,
                self.library_shelf_filter,
            ))
            .child(shelf_btn(
                "shelf_fav",
                "Favorites",
                2,
                self.library_shelf_filter,
            ))
            .child(shelf_btn(
                "shelf_fin",
                "Finished",
                3,
                self.library_shelf_filter,
            ))
            .child(shelf_btn(
                "shelf_want",
                "Want to Read",
                4,
                self.library_shelf_filter,
            ));

        let mut scroll_body = v_flex().w_full().gap_4().pb_6();
        scroll_body = scroll_body
            .child(shelves)
            .child(Input::new(&self.library_search_input));
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
            scroll_body = scroll_body.child(section_heading("Continue reading"));
            let mut row = h_flex().gap_3().flex_wrap();
            for (i, r) in in_progress.iter().take(4).enumerate() {
                let id = format!("cont{i}");
                let on_open_book = {
                    let p = r.path.clone();
                    cx.listener(move |view, _ev, window, cx| {
                        view.open_file(PathBuf::from(p.clone()), window, cx);
                    })
                };
                let on_fav = {
                    let p = r.path.clone();
                    cx.listener(move |view, _ev, _w, cx| view.toggle_favorite(p.clone(), cx))
                };
                let on_open_card = {
                    let p = r.path.clone();
                    cx.listener(move |view, _ev, window, cx| {
                        view.open_file(PathBuf::from(p.clone()), window, cx);
                    })
                };
                row = row.child(wide_tile(
                    BookTileMeta {
                        id: id.clone(),
                        cover: cover_for(&self.covers, r),
                        title: book_title(r),
                        subtitle: book_subtitle(r),
                        format: r.format.clone(),
                        pct: book_pct(r),
                    },
                    palette,
                    on_open_card,
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(
                            Button::new(format!("{id}-open"))
                                .outline()
                                .label("Resume")
                                .on_click(on_open_book),
                        )
                        .child(
                            Button::new(format!("{id}-fav"))
                                .ghost()
                                .label(star_label(r.favorite))
                                .tooltip("Toggle favorite")
                                .on_click(on_fav),
                        )
                        .into_any_element(),
                ));
            }
            scroll_body = scroll_body.child(row);
        }

        scroll_body = scroll_body.child(section_heading(&format!("All books ({})", recent.len())));

        let mut shown = 0usize;
        if is_list_view {
            // ---- List View Layout -----------------------------------------------
            let mut list_col = v_flex().gap_2().w_full();
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
                let on_open_book = {
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

                let on_open_row = {
                    let p = r.path.clone();
                    cx.listener(move |view, _ev, window, cx| {
                        view.open_file(PathBuf::from(p.clone()), window, cx);
                    })
                };
                list_col = list_col.child(
                    h_flex()
                        .w_full()
                        .p_2()
                        .gap_3()
                        .rounded_md()
                        .bg(palette.surface)
                        .border_1()
                        .border_color(palette.border)
                        .hover(|s| s.border_color(palette.muted))
                        .items_center()
                        .child(
                            h_flex()
                                .id(format!("{id}-row"))
                                .flex_1()
                                .gap_3()
                                .items_center()
                                .on_click(on_open_row)
                                .child(cover_art(
                                    format!("{id}-c"),
                                    cover_for(&self.covers, r),
                                    &book_title(r),
                                    &r.format,
                                    palette,
                                    38.0,
                                    52.0,
                                ))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(book_title(r)),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(palette.muted)
                                                .child(book_subtitle(r)),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(palette.muted)
                                                .child(format!(
                                                    "{} min read · {} sections",
                                                    r.reading_time_secs / 60,
                                                    r.total
                                                )),
                                        ),
                                ),
                        )
                        .child(
                            v_flex()
                                .w(px(120.0))
                                .child(Progress::new(format!("{id}-p")).value(pct))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(palette.muted)
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
                                        .outline()
                                        .label("Read")
                                        .on_click(on_open_book),
                                )
                                .child(
                                    Button::new(format!("{id}-st"))
                                        .ghost()
                                        .label(status_badge)
                                        .tooltip("Cycle reading status")
                                        .on_click(on_status),
                                )
                                .child(
                                    Button::new(format!("{id}-fav"))
                                        .ghost()
                                        .label(star_label(r.favorite))
                                        .tooltip("Toggle favorite")
                                        .on_click(on_fav),
                                )
                                .child(
                                    Button::new(format!("{id}-rm"))
                                        .ghost()
                                        .label("\u{2715}")
                                        .tooltip("Remove from library")
                                        .on_click(on_remove),
                                ),
                        ),
                );
            }
            if shown > 0 {
                scroll_body = scroll_body.child(list_col);
            }
        } else {
            // ---- Grid View Layout -----------------------------------------------
            let mut grid = h_flex().gap_3().flex_wrap();
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
                let on_open_book = {
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
                    BookTileMeta {
                        id: id.clone(),
                        cover: cover_for(&self.covers, r),
                        title: book_title(r),
                        subtitle: book_subtitle(r),
                        format: r.format.clone(),
                        pct,
                    },
                    palette,
                    on_open_book,
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .child(
                            Button::new(format!("{id}-st"))
                                .ghost()
                                .label(status_badge)
                                .tooltip("Cycle reading status")
                                .on_click(on_status),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    Button::new(format!("{id}-fav"))
                                        .ghost()
                                        .label(star_label(r.favorite))
                                        .tooltip("Toggle favorite")
                                        .on_click(on_fav),
                                )
                                .child(
                                    Button::new(format!("{id}-rm"))
                                        .ghost()
                                        .label("\u{2715}")
                                        .tooltip("Remove from library")
                                        .on_click(on_remove),
                                ),
                        )
                        .into_any_element(),
                ));
            }
            if shown > 0 {
                scroll_body = scroll_body.child(grid);
            }
        }

        if shown == 0 {
            let empty_msg = if self.state.recent.is_empty() {
                "Your library is empty. Open a book file or scan a folder to build your shelf."
            } else {
                "No books match the current shelf or search filter."
            };
            let mut empty_card = v_flex()
                .w_full()
                .p_6()
                .gap_3()
                .items_center()
                .rounded_md()
                .bg(palette.surface)
                .border_1()
                .border_color(palette.border)
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(palette.muted)
                        .child(empty_msg),
                );
            if self.state.recent.is_empty() {
                let on_open_empty = cx.listener(|view, _ev, window, cx| {
                    if let Some(path) = pick_ebook_file() {
                        view.open_file(path, window, cx);
                    }
                });
                let on_scan_empty = cx.listener(|view, _ev, window, cx| {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        view.scan_folder(folder, window, cx);
                    }
                });
                empty_card = empty_card.child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("empty-open")
                                .primary()
                                .label("Open file")
                                .on_click(on_open_empty),
                        )
                        .child(
                            Button::new("empty-scan")
                                .outline()
                                .label("Scan folder")
                                .on_click(on_scan_empty),
                        ),
                );
            }
            scroll_body = scroll_body.child(empty_card);
        }

        // OPDS Catalog
        scroll_body = scroll_body.child(section_heading("OPDS catalog"));
        scroll_body = scroll_body.child(Input::new(&self.opds_url_input));
        let on_fetch = cx.listener(|view, _ev, _w, cx| {
            let url = view.opds_url_input.read(cx).value().to_string();
            if url.trim().is_empty() {
                return;
            }
            view.opds_status = "Fetching OPDS catalog...".into();
            cx.notify();
            let bg = cx.background_executor().clone();
            cx.spawn(async move |view, cx| {
                let res = bg.spawn(async move { fetch_opds_feed(&url) }).await;
                let _ = view.update(cx, |view, cx| {
                    view.opds_status = res.0;
                    view.opds_entries = res.1;
                    cx.notify();
                });
            })
            .detach();
        });

        scroll_body = scroll_body.child(Button::new("fetchopds").label("Fetch").on_click(on_fetch));
        scroll_body = scroll_body.child(
            div()
                .text_size(px(12.0))
                .text_color(palette.muted)
                .child(self.opds_status.clone()),
        );
        for (title, href) in self.opds_entries.clone() {
            let href2 = href.clone();
            let title_name = title.clone();
            let on_open_entry = cx.listener(move |view, _ev, window, cx| {
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
                let bg = cx.background_executor().clone();
                cx.spawn_in(window, async move |view, cx| {
                    let dl_res = bg
                        .spawn(async move {
                            let bytes = download_opds_book(&href)?;
                            let hash = simple_hash(&href);
                            let tmp =
                                std::env::temp_dir().join(format!("{clean_title}_{hash}.epub"));
                            std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
                            Ok::<PathBuf, String>(tmp)
                        })
                        .await;
                    match dl_res {
                        Ok(tmp) => {
                            let _ = view.update_in(cx, |view, window, cx| {
                                view.opds_status = format!("Opened '{}'", title);
                                view.open_file(tmp, window, cx);
                                cx.notify();
                            });
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
            scroll_body = scroll_body.child(
                Button::new(format!("opds{}", title))
                    .ghost()
                    .label(title)
                    .on_click(on_open_entry),
            );
        }

        v_flex()
            .relative()
            .size_full()
            .bg(palette.bg)
            .text_color(palette.fg)
            .p_4()
            .gap_4()
            .child(header)
            .child(
                div()
                    .id("library-scroll")
                    .flex_1()
                    .overflow_y_scrollbar()
                    .child(scroll_body),
            )
            .children(self.render_modal_overlay(palette, cx))
            .into_any_element()
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
            self.jump(idx, cx);
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
        let recent = self.book_path.as_ref().and_then(|path| {
            let key = path.to_string_lossy().to_string();
            self.state.recent.iter().find(|r| r.path == key)
        });
        Some(format_book_info(book, self.current, self.rtl_mode, recent))
    }

    fn validation_body(&self) -> Option<String> {
        self.book.as_ref().map(format_validation)
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
    fn stop_tts(&mut self) {
        if let Some(engine) = &mut self.tts_engine {
            let _ = engine.stop();
        }
        self.tts_speaking = false;
    }

    fn apply_tts_rate(engine: &mut tts::Tts, mult: f32) {
        let normal = engine.normal_rate();
        let min = engine.min_rate();
        let max = engine.max_rate();
        let rate = if mult <= 1.0 {
            let t = ((mult - 0.5) / 0.5).clamp(0.0, 1.0);
            min + t * (normal - min)
        } else {
            let t = ((mult - 1.0) / 1.5).clamp(0.0, 1.0);
            normal + t * (max - normal)
        };
        let _ = engine.set_rate(rate);
    }

    fn toggle_tts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tts_speaking {
            self.stop_tts();
            cx.notify();
            return;
        }
        let selected = Self::selected_text(window, cx);
        let text = if !selected.is_empty() {
            selected
        } else {
            self.tts_text.trim().to_string()
        };
        if text.is_empty() {
            notify_simple(
                window,
                cx,
                "Read Aloud",
                "No text to speak in this section.".into(),
            );
            return;
        }
        if self.tts_engine.is_none() {
            match tts::Tts::default() {
                Ok(engine) => self.tts_engine = Some(engine),
                Err(e) => {
                    notify_simple(window, cx, "TTS Unavailable", format!("{e}"));
                    return;
                }
            }
        }
        if let Some(engine) = &mut self.tts_engine {
            Self::apply_tts_rate(engine, self.tts_rate);
            if let Err(e) = engine.speak(text, true) {
                self.tts_speaking = false;
                notify_simple(window, cx, "TTS Error", format!("{e}"));
            } else {
                self.tts_speaking = true;
            }
        }
        cx.notify();
    }

    fn adjust_tts_rate(&mut self, delta: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.tts_rate = (self.tts_rate + delta).clamp(0.5, 2.5);
        if let Some(engine) = &mut self.tts_engine {
            Self::apply_tts_rate(engine, self.tts_rate);
        }
        if self.tts_speaking {
            self.stop_tts();
            self.toggle_tts(window, cx);
        } else {
            cx.notify();
        }
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
    fn set_theme(&mut self, theme: &str, cx: &mut Context<Self>) {
        self.theme = theme.to_string();
        self.dark = is_dark_theme(&self.theme);
        self.sync_state();
        self.load_current(cx);
        cx.notify();
    }
    fn cycle_theme(&mut self, cx: &mut Context<Self>) {
        let next = match self.theme.as_str() {
            "Light" => "Dark",
            "Dark" => "Sepia",
            "Sepia" => "Nord",
            "Nord" => "Solarized",
            "Solarized" => "Solarized Dark",
            "Solarized Dark" => "E-Ink",
            "E-Ink" => "OLED Black",
            "OLED Black" => "Custom",
            _ => "Light",
        };
        self.set_theme(next, cx);
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
    fn show_about_modal(&mut self, cx: &mut Context<Self>) {
        let about = format!(
            "eParna v{}\n{}\n\n\
            Built in pure Rust with ebook-rs & gpui-kit (gpui-fast).\n\
            Supports 25+ ebook & document formats (EPUB, KEPUB, KFX, MOBI, AZW3, FB2, LIT, PDF, CBZ, DOCX, ODT, RTF, HTML, Markdown, Typst, LaTeX, and more).\n\n\
            Author: {}\n\
            License: MIT\n\
            Repository: {}",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            env!("CARGO_PKG_AUTHORS"),
            env!("CARGO_PKG_REPOSITORY"),
        );
        self.show_modal("About eParna", about, cx);
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
  v                    Read aloud / Stop OS Text-to-Speech

Sidebars & Panels:
  Ctrl + \\            Toggle sidebar visibility
  t                    Table of Contents sidebar
  s                    Search in book sidebar
  a                    Annotations & Highlights sidebar
  d                    Dictionary & Reference sidebar
  b / Ctrl + B         Toggle Bookmark for current section
  p / Ctrl + P         Toggle Typography & Reading Settings popover
  Ctrl + C             Copy selected text
  ? / F1               This Keyboard Shortcuts cheatsheet";

        self.show_modal("Keyboard Shortcuts", help.into(), cx);
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

    fn export_book_as(
        &mut self,
        label: &str,
        ext: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(book) = &self.book else {
            return;
        };
        let stem = sanitize_filename(&book.metadata().title);
        let filter_ext = if ext == "kepub.epub" { "epub" } else { ext };
        let chosen = rfd::FileDialog::new()
            .set_file_name(format!("{stem}.{ext}"))
            .add_filter(label, &[filter_ext])
            .save_file();
        let Some(path) = chosen else {
            return;
        };
        self.export_modal = false;
        match export_book_bytes(book, &path) {
            Ok(bytes) => {
                let n = bytes.len();
                let ok = std::fs::write(&path, &bytes).is_ok();
                notify_simple(
                    window,
                    cx,
                    if ok { "Exported Book" } else { "Write failed" },
                    format!("{} ({n} bytes)", path.display()),
                );
            }
            Err(e) => notify_simple(window, cx, "Export failed", e.to_string()),
        }
        cx.notify();
    }

    fn export_book_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = &self.book else {
            return;
        };
        let stem = sanitize_filename(&book.metadata().title);
        let chosen = rfd::FileDialog::new()
            .set_file_name(format!("{stem}.epub"))
            .add_filter("EPUB 3", &["epub"])
            .add_filter("Kobo KEPUB", &["kepub.epub", "kepub"])
            .add_filter("Amazon KFX", &["kfx"])
            .add_filter("Kindle MOBI", &["mobi"])
            .add_filter("Kindle AZW3", &["azw3"])
            .add_filter("PDF Document", &["pdf"])
            .add_filter("Markdown", &["md"])
            .add_filter("Plain Text", &["txt"])
            .add_filter("HTML5", &["html"])
            .add_filter("Word DOCX", &["docx"])
            .add_filter("OpenDocument ODT", &["odt"])
            .add_filter("Rich Text RTF", &["rtf"])
            .add_filter("FictionBook 2", &["fb2"])
            .add_filter("PowerPoint PPTX", &["pptx"])
            .add_filter("LaTeX", &["tex"])
            .add_filter("Typst", &["typ"])
            .add_filter("AsciiDoc", &["adoc"])
            .add_filter("Org-mode", &["org"])
            .add_filter("reStructuredText", &["rst"])
            .add_filter("HTMLZ / TXTZ", &["htmlz", "txtz"])
            .add_filter("OEB / ZIP", &["oeb", "zip"])
            .add_filter(
                "Legacy (LIT/PDB/PMLZ/RB/TCR)",
                &["lit", "pdb", "pmlz", "rb", "tcr"],
            )
            .add_filter("RAG Chunks JSON", &["json"])
            .save_file();
        let Some(path) = chosen else {
            return;
        };
        match export_book_bytes(book, &path) {
            Ok(bytes) => {
                let n = bytes.len();
                let ok = std::fs::write(&path, &bytes).is_ok();
                notify_simple(
                    window,
                    cx,
                    if ok { "Exported Book" } else { "Write failed" },
                    format!("{} ({n} bytes)", path.display()),
                );
            }
            Err(e) => notify_simple(window, cx, "Export failed", e.to_string()),
        }
    }

    fn optimize_epub(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = &self.book else {
            return;
        };
        let stem = sanitize_filename(&book.metadata().title);
        let chosen = rfd::FileDialog::new()
            .set_file_name(format!("{stem}.optimized.epub"))
            .add_filter("EPUB 3", &["epub"])
            .save_file();
        let Some(path) = chosen else {
            return;
        };
        match optimize_book_epub3(book) {
            Ok((bytes, report)) => {
                let n = bytes.len();
                let ok = std::fs::write(&path, &bytes).is_ok();
                notify_simple(
                    window,
                    cx,
                    if ok {
                        "Saved Optimized EPUB"
                    } else {
                        "Write failed"
                    },
                    format!(
                        "{} ({n} bytes · saved {} B, {} deduped assets, {} purged CSS rules)",
                        path.display(),
                        report.saved_bytes,
                        report.deduplicated_assets_count,
                        report.purged_css_rules_count
                    ),
                );
            }
            Err(e) => notify_simple(window, cx, "Optimize failed", e.to_string()),
        }
    }

    fn citations_body(&self) -> Option<String> {
        self.book.as_ref().map(format_citations)
    }

    fn annotations_to_markdown(&self, title: &str, author: &str) -> String {
        format_annotations_markdown(title, author, &self.annotations)
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
    fn render_modal_overlay(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let m = self.modal.as_ref()?;
        Some(
            div()
                .absolute()
                .inset_0()
                .bg(rgba(0x00000088))
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|v, _, _, cx| v.close_modal(cx)),
                )
                .child(
                    v_flex()
                        .w(px(720.0))
                        .max_h(px(560.0))
                        .gap_3()
                        .rounded_lg()
                        .bg(palette.bg)
                        .text_color(palette.fg)
                        .border_1()
                        .border_color(palette.border)
                        .p_4()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(m.title.clone()),
                                )
                                .child(
                                    Button::new("modal-x")
                                        .ghost()
                                        .label("\u{2715}")
                                        .tooltip("Close (Esc)")
                                        .on_click(cx.listener(|v, _, _, cx| v.close_modal(cx))),
                                ),
                        )
                        .child(if let Some(src) = &m.image {
                            if let Some(im) =
                                crate::renderer::decode_data_uri_image(src, &self.image_cache)
                            {
                                div()
                                    .flex_1()
                                    .max_h(px(440.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .overflow_hidden()
                                    .child(
                                        img(ImageSource::Image(im))
                                            .max_w(px(660.0))
                                            .max_h(px(420.0))
                                            .object_fit(ObjectFit::ScaleDown),
                                    )
                                    .into_any_element()
                            } else {
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
                            }
                        } else {
                            div()
                                .id("modal-body-scroll")
                                .flex_1()
                                .max_h(px(440.0))
                                .overflow_y_scrollbar()
                                .p_3()
                                .rounded_md()
                                .bg(palette.surface)
                                .border_1()
                                .border_color(palette.border)
                                .text_size(px(12.5))
                                .line_height(px(19.0))
                                .font_family(crate::renderer::resolve_font_family("monospace"))
                                .text_color(palette.fg)
                                .child(gpui_kit::base::SelectableText::new(
                                    "modal-body-text",
                                    m.body.clone(),
                                ))
                                .into_any_element()
                        })
                        .child(
                            h_flex().justify_end().child(
                                Button::new("modal-close")
                                    .primary()
                                    .label("Close")
                                    .on_click(cx.listener(|v, _, _, cx| v.close_modal(cx))),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }
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
        let palette = self.theme_palette();
        let want_dark = is_palette_dark(&self.theme, &palette);
        if self.synced_dark_mode != Some(want_dark) {
            self.synced_dark_mode = Some(want_dark);
            Theme::change(
                if want_dark {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                },
                Some(window),
                cx,
            );
        }
        let (bg, fg) = (palette.bg, palette.fg);
        if self.show_library {
            return self.render_library(window, cx, palette);
        }

        let max_scroll_y: f32 = self.scroll_handle.max_offset().y.into();
        let cur_scroll_y: f32 = (-self.scroll_handle.offset().y).into();
        let scroll_frac = if max_scroll_y > 1.0 {
            (cur_scroll_y / max_scroll_y).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let live_progress = if self.section_end_pct > self.section_start_pct {
            (self.section_start_pct + scroll_frac * (self.section_end_pct - self.section_start_pct))
                .clamp(0.0, 100.0)
        } else {
            self.cached_progress
        };

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
            let sec_words = self.cached_word_count;
            if sec_words > 0 {
                let rem_words = ((1.0 - scroll_frac) * sec_words as f32).round() as usize;
                let sec_mins = ((rem_words as f32 / wpm).ceil() as usize).max(1);
                parts.push(format!("{rem_words} words (~{sec_mins}m left in ch.)"));
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
            parts.push(format!("{live_progress:.1}%"));
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
        if (column_px - self.cached_column_px).abs() > 0.5 {
            self.cached_column_px = column_px;
            self.cached_constrained_html = constrain_images(&self.html, column_px);
        }
        let html = self.cached_constrained_html.clone();
        let pos = if let Some(b) = &self.book {
            format!("{}/{}", self.current + 1, b.sections().len())
        } else {
            String::new()
        };
        let progress = live_progress;

        let on_open = cx.listener(|view, _ev, window, cx| {
            if let Some(path) = pick_ebook_file() {
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
                                let idx = href_to_index(book, &m.full_path)
                                    .or_else(|| href_to_index(book, &m.href))?;
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
                    v = v.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(palette.muted)
                            .child("Chapters:"),
                    );
                    let active_toc_i = self
                        .active_toc_index
                        .filter(|&ai| {
                            self.toc_items.get(ai).map(|t| t.section_index) == Some(self.current)
                        })
                        .or_else(|| {
                            self.toc_items
                                .iter()
                                .position(|item| item.section_index == self.current)
                        });
                    let mut chapter_list = v_flex().gap_1();
                    for (i, item) in self.toc_items.iter().enumerate() {
                        let idx = item.section_index;
                        let is_active = active_toc_i == Some(i);
                        let indent = (item.depth.min(4) as f32) * 14.0;
                        let max_chars = 32usize.saturating_sub(item.depth.min(4) * 2).max(16);
                        let display_label = if item.depth > 0 {
                            format!("· {}", truncate(&item.label, max_chars))
                        } else {
                            truncate(&item.label, max_chars)
                        };
                        let btn = Button::new(format!("toc_ch_{i}")).label(display_label);
                        let btn = if is_active {
                            btn.outline()
                        } else {
                            btn.ghost()
                        };
                        chapter_list = chapter_list.child(div().pl(px(indent)).child(
                            btn.on_click(cx.listener(move |view, _, _, cx| {
                                view.jump(idx, cx);
                                view.active_toc_index = Some(i);
                                cx.notify();
                            })),
                        ));
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
                        let label = if let Some(item) = self
                            .toc_items
                            .iter()
                            .find(|item| item.section_index == sec_idx)
                        {
                            format!("Sec {}: {}", sec_idx + 1, truncate(item.label.trim(), 22))
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
                                .child({
                                    let btn = Button::new(format!("bmlink_{sec_idx}")).label(label);
                                    let btn = if is_current {
                                        btn.outline()
                                    } else {
                                        btn.ghost()
                                    };
                                    btn.on_click(cx.listener(move |view, _, _, cx| {
                                        view.jump(sec_idx, cx);
                                    }))
                                })
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
                let mut color_row = h_flex().gap_2().items_center().child(
                    div()
                        .text_size(px(11.0))
                        .text_color(palette.muted)
                        .child("Color:"),
                );
                for color_name in ["yellow", "green", "blue", "pink", "purple"] {
                    let c = color_name.to_string();
                    let selected = self.highlight_color == color_name;
                    color_row = color_row.child(highlight_dot(
                        format!("col-{color_name}"),
                        color_name,
                        selected,
                        palette.fg,
                        cx.listener(move |v, _, _, cx| {
                            v.highlight_color = c.clone();
                            cx.notify();
                        }),
                    ));
                }
                v = v.child(color_row);
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
                    let word = word.trim().to_string();
                    if word.is_empty() {
                        return;
                    }
                    view.dictionary_result = format!("Searching Wikipedia for “{}”...", word);
                    cx.notify();
                    let bg = cx.background_executor().clone();
                    cx.spawn(async move |view, cx| {
                        let res = bg
                            .spawn(async move { fetch_wikipedia_summary(&word) })
                            .await;
                        let _ = view.update(cx, |view, cx| {
                            view.dictionary_result = res;
                            cx.notify();
                        });
                    })
                    .detach();
                });
                let on_translate = cx.listener(|view, _ev, _w, cx| {
                    let text = view.dictionary_input.read(cx).value().to_string();
                    let text = text.trim().to_string();
                    if text.is_empty() {
                        return;
                    }
                    view.dictionary_result = format!("Translating “{}”...", text);
                    cx.notify();
                    let bg = cx.background_executor().clone();
                    cx.spawn(async move |view, cx| {
                        let res = bg.spawn(async move { fetch_translation(&text) }).await;
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

        let sb_tab = |id: &str, label: &str, mode: SidebarMode, active: SidebarMode| {
            let b = Button::new(id.to_string()).label(label.to_string());
            let b = if active == mode {
                b.outline()
            } else {
                b.ghost()
            };
            b.on_click(cx.listener(move |view, _, _, cx| {
                view.sidebar_mode = mode;
                cx.notify();
            }))
        };

        let sidebar_buttons = h_flex()
            .gap_1()
            .p_2()
            .flex_wrap()
            .items_center()
            .border_b_1()
            .border_color(palette.border)
            .child(sb_tab(
                "sb_contents",
                "Contents",
                SidebarMode::Contents,
                self.sidebar_mode,
            ))
            .child(sb_tab(
                "sb_bm",
                "Bookmarks",
                SidebarMode::Bookmarks,
                self.sidebar_mode,
            ))
            .child(sb_tab(
                "sb_ann",
                "Notes",
                SidebarMode::Annotations,
                self.sidebar_mode,
            ))
            .child(sb_tab(
                "sb_search",
                "Search",
                SidebarMode::Search,
                self.sidebar_mode,
            ))
            .child(sb_tab(
                "sb_fn",
                "Footnotes",
                SidebarMode::Footnotes,
                self.sidebar_mode,
            ))
            .child(sb_tab(
                "sb_dict",
                "Dictionary",
                SidebarMode::Dictionary,
                self.sidebar_mode,
            ))
            .child(
                Button::new("sb-narrow")
                    .ghost()
                    .label("−")
                    .tooltip("Narrow sidebar")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_width = (view.sidebar_width - 24.0).max(180.0);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb-wide")
                    .ghost()
                    .label("+")
                    .tooltip("Widen sidebar")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_width = (view.sidebar_width + 24.0).min(440.0);
                        cx.notify();
                    })),
            );

        let is_bm = self.bookmarks.contains(&self.current);
        let bm_label = if is_bm { "★" } else { "☆" };
        let back_btn = self.nav_history.last().map(|&last_sec| {
            Button::new("jump_back")
                .ghost()
                .label(format!("⤺ Sec {}", last_sec + 1))
                .tooltip("Jump back (Alt+← / Backspace)")
                .on_click(cx.listener(|v, _, _, cx| v.jump_back(cx)))
                .into_any_element()
        });

        let sb_toggle_btn = {
            let b = Button::new("toggle-sb")
                .label("Sidebar")
                .tooltip("Toggle sidebar (Ctrl+\\)");
            let b = if self.sidebar_open {
                b.outline()
            } else {
                b.ghost()
            };
            b.on_click(cx.listener(|v, _, _, cx| {
                v.sidebar_open = !v.sidebar_open;
                cx.notify();
            }))
        };

        let mut left_controls = h_flex().gap_1().items_center().child(sb_toggle_btn).child(
            Button::new("library")
                .ghost()
                .label("Library")
                .tooltip("Back to library (Ctrl+L)")
                .on_click(on_lib),
        );

        if let Some(btn) = back_btn {
            left_controls = left_controls.child(btn);
        }

        let current_chapter_title = self
            .active_toc_index
            .and_then(|idx| self.toc_items.get(idx))
            .or_else(|| {
                self.toc_items
                    .iter()
                    .find(|item| item.section_index == self.current)
            })
            .map(|item| truncate(item.label.trim(), 34));

        let book_title_short = self
            .book
            .as_ref()
            .map(|b| truncate(&b.metadata().title, 36))
            .unwrap_or_default();

        let header_title = match &current_chapter_title {
            Some(ch) if !ch.is_empty() && ch != &book_title_short => {
                format!("{book_title_short} · {ch}")
            }
            _ => book_title_short,
        };

        let find_btn = {
            let b = Button::new("find-btn")
                .label("Find")
                .tooltip("Find in chapter (Ctrl+F)");
            let b = if self.show_find_bar {
                b.outline()
            } else {
                b.ghost()
            };
            b.on_click(cx.listener(|v, _, _, cx| {
                v.show_find_bar = !v.show_find_bar;
                cx.notify();
            }))
        };

        let aa_btn = {
            let b = Button::new("aa")
                .label("Aa")
                .tooltip("Typography & themes (P)");
            let b = if self.panel == Panel::Typography {
                b.outline()
            } else {
                b.ghost()
            };
            b.on_click(cx.listener(|view, _, _, cx| {
                view.panel = if view.panel == Panel::Typography {
                    Panel::None
                } else {
                    Panel::Typography
                };
                cx.notify();
            }))
        };

        let more_btn = {
            let b = Button::new("more")
                .label("\u{22ef}")
                .tooltip("Book actions");
            let b = if self.panel == Panel::Actions {
                b.outline()
            } else {
                b.ghost()
            };
            b.on_click(cx.listener(|view, _, _, cx| {
                view.panel = if view.panel == Panel::Actions {
                    Panel::None
                } else {
                    Panel::Actions
                };
                cx.notify();
            }))
        };

        let tts_btn = {
            let b = Button::new("tts-btn")
                .label(if self.tts_speaking {
                    "Stop TTS"
                } else {
                    "Speak"
                })
                .tooltip("Read aloud / Stop OS Text-to-Speech (V)");
            let b = if self.tts_speaking {
                b.outline()
            } else {
                b.ghost()
            };
            b.on_click(cx.listener(|v, _, window, cx| v.toggle_tts(window, cx)))
        };

        let right_controls = h_flex()
            .gap_1()
            .items_center()
            .child(tts_btn)
            .child(
                Button::new("bm")
                    .ghost()
                    .label(bm_label)
                    .tooltip("Toggle bookmark (Ctrl+B)")
                    .on_click(on_bm),
            )
            .child(
                Button::new("highlight")
                    .ghost()
                    .label("Highlight")
                    .tooltip("Highlight selected text")
                    .on_click(on_highlight),
            )
            .child(highlight_dot(
                "hl-col".to_string(),
                &self.highlight_color,
                true,
                palette.border,
                cx.listener(|v, _, _, cx| v.cycle_highlight_color(cx)),
            ))
            .child(find_btn)
            .child(
                Button::new("zen-hdr")
                    .ghost()
                    .label("Zen")
                    .tooltip("Distraction-free mode (Z)")
                    .on_click(cx.listener(|view, _, _, cx| view.toggle_zen_mode(cx))),
            )
            .child(aa_btn)
            .child(more_btn);

        let header = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(palette.surface)
            .border_b_1()
            .border_color(palette.border)
            .child(left_controls)
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.muted)
                    .child(header_title),
            )
            .child(right_controls);

        let mut theme_grid = h_flex().gap_2().flex_wrap();
        for &(theme_name, short_label) in THEME_PRESETS {
            let selected = self.theme == theme_name;
            let name_owned = theme_name.to_string();
            let swatch_palette = theme_palette_for(theme_name, &self.custom_bg, &self.custom_fg);
            theme_grid = theme_grid.child(theme_swatch(
                format!("th-sw-{theme_name}"),
                short_label,
                swatch_palette,
                selected,
                palette.fg,
                cx.listener(move |v, _, _, cx| v.set_theme(&name_owned, cx)),
            ));
        }

        let mut typography_popover = v_flex()
            .w(px(344.0))
            .max_h(px(520.0))
            .gap_3()
            .p_4()
            .rounded_lg()
            .bg(palette.surface)
            .text_color(palette.fg)
            .border_1()
            .border_color(palette.border)
            .shadow_lg()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Appearance & Typography"),
                    )
                    .child(
                        Button::new("reset-display")
                            .ghost()
                            .label("Reset")
                            .tooltip("Reset typography to defaults")
                            .on_click(cx.listener(|v, _, _, cx| v.reset_display(cx))),
                    ),
            )
            .child(theme_grid);

        if self.theme == "Custom" {
            typography_popover = typography_popover.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(palette.muted)
                            .child("Bg"),
                    )
                    .child(Input::new(&self.custom_bg_input).w(px(110.0)))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(palette.muted)
                            .child("Fg"),
                    )
                    .child(Input::new(&self.custom_fg_input).w(px(110.0))),
            );
        }

        typography_popover = typography_popover
            .child(div().w_full().h(px(1.0)).bg(palette.border))
            .child(setting_row(
                "Font size",
                format!("{:.0} px", self.font_size),
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
                format!("{:.0} px", self.margin),
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
                format!("{:.0} px", self.max_width),
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
            .child(setting_row(
                "TTS speed",
                format!("{:.1}x", self.tts_rate),
                Button::new("tts-dec")
                    .ghost()
                    .label("\u{2212}")
                    .on_click(cx.listener(|v, _, window, cx| v.adjust_tts_rate(-0.1, window, cx)))
                    .into_any_element(),
                Button::new("tts-inc")
                    .ghost()
                    .label("+")
                    .on_click(cx.listener(|v, _, window, cx| v.adjust_tts_rate(0.1, window, cx)))
                    .into_any_element(),
            ))
            .child(div().w_full().h(px(1.0)).bg(palette.border))
            .child(cycle_row(
                "Typeface",
                &self.font_family,
                "tf",
                cx.listener(|v, _, _, cx| v.cycle_font(cx)),
            ))
            .child(cycle_row(
                "Align",
                &self.text_align,
                "al",
                cx.listener(|v, _, _, cx| {
                    v.text_align = match v.text_align.as_str() {
                        "Default" => "Left".to_string(),
                        "Left" => "Center".to_string(),
                        "Center" => "Right".to_string(),
                        _ => "Default".to_string(),
                    };
                    v.justify = v.text_align == "Left";
                    v.sync_state();
                    cx.notify();
                }),
            ))
            .child(cycle_row(
                "Pages",
                if self.two_page { "Two" } else { "One" },
                "pg",
                cx.listener(|v, _, _, cx| v.toggle_two_page(cx)),
            ));

        let actions_popover = v_flex()
            .w(px(284.0))
            .gap_2()
            .p_3()
            .rounded_lg()
            .bg(palette.surface)
            .text_color(palette.fg)
            .border_1()
            .border_color(palette.border)
            .shadow_lg()
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.muted)
                    .child("NAVIGATE"),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Input::new(&self.goto_input).flex_1())
                    .child(
                        Button::new("go")
                            .outline()
                            .label("Go")
                            .on_click(cx.listener(|v, _ev, _w, cx| {
                                let s = v.goto_input.read(cx).value().trim().to_string();
                                if s.ends_with('%') {
                                    if let Ok(pct) = s.trim_end_matches('%').trim().parse::<f32>()
                                        && let Some(book) = &v.book
                                    {
                                        let total = book.sections().len();
                                        if total > 0 {
                                            let idx =
                                                ((pct / 100.0) * (total as f32)).round() as usize;
                                            v.jump(idx.min(total.saturating_sub(1)), cx);
                                        }
                                    }
                                } else if let Ok(n) = s.parse::<usize>() {
                                    v.jump(n.saturating_sub(1), cx);
                                }
                                v.panel = Panel::None;
                                cx.notify();
                            })),
                    ),
            )
            .child(div().w_full().h(px(1.0)).bg(palette.border))
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.muted)
                    .child("BOOK"),
            )
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        Button::new("info")
                            .ghost()
                            .label("Book info\u{2026}")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.panel = Panel::None;
                                if let Some(body) = v.book_info_body() {
                                    v.show_modal("Book info", body, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("validate")
                            .ghost()
                            .label("Validate\u{2026}")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.panel = Panel::None;
                                if let Some(body) = v.validation_body() {
                                    v.show_modal("Validation", body, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("citations")
                            .ghost()
                            .label("Citations\u{2026}")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.panel = Panel::None;
                                if let Some(body) = v.citations_body() {
                                    v.show_modal("Citations (BibTeX, APA, MLA, Chicago)", body, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("exportbook")
                            .ghost()
                            .label("Export / Convert\u{2026}")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.panel = Panel::None;
                                v.export_modal = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("optimizeepub")
                            .ghost()
                            .label("Optimize EPUB\u{2026}")
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.panel = Panel::None;
                                v.optimize_epub(window, cx);
                            })),
                    )
                    .child(
                        Button::new("export")
                            .ghost()
                            .label("Export notes\u{2026}")
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.panel = Panel::None;
                                v.export_annotations(window, cx);
                            })),
                    ),
            )
            .child(div().w_full().h(px(1.0)).bg(palette.border))
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.muted)
                    .child("READING & WINDOW"),
            )
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        Button::new("copy-act")
                            .ghost()
                            .label("Copy selection")
                            .on_click(on_copy),
                    )
                    .child(
                        Button::new("flow-act")
                            .ghost()
                            .label(if self.rtl_mode {
                                "Flow: RTL"
                            } else {
                                "Flow: LTR"
                            })
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.rtl_mode = !v.rtl_mode;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("open-act")
                            .ghost()
                            .label("Open file\u{2026}")
                            .on_click(on_open),
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
                    )
                    .child(
                        Button::new("sc-modal")
                            .ghost()
                            .label("Shortcuts (?)")
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.panel = Panel::None;
                                v.show_shortcuts_modal(cx);
                            })),
                    ),
            );

        // In-page search bar
        let find_bar = if self.show_find_bar {
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .bg(palette.surface)
                .border_b_1()
                .border_color(palette.border)
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

        let floating_popover: Option<AnyElement> = match self.panel {
            Panel::Typography => Some(
                div()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|v, _, _, cx| {
                            v.panel = Panel::None;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(50.0))
                            .right(px(12.0))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(typography_popover),
                    )
                    .into_any_element(),
            ),
            Panel::Actions => Some(
                div()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|v, _, _, cx| {
                            v.panel = Panel::None;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(50.0))
                            .right(px(12.0))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(actions_popover),
                    )
                    .into_any_element(),
            ),
            Panel::None => None,
        };

        let dom2 = if self.two_page {
            if let Some((idx, dom)) = &self.cached_dom2
                && *idx == self.current + 1
            {
                Some(dom.clone())
            } else if let Some(book) = &self.book {
                book.get_section(self.current + 1).ok().map(|s| {
                    let inlined = inline_svg_images(&s.processed_html, book, &s.full_path);
                    ebook_rs::EbookDomTree::parse(&inlined)
                })
            } else {
                None
            }
        } else {
            None
        };
        let view = cx.entity().clone();
        let on_link_click: crate::renderer::LinkClickCallback = std::sync::Arc::new({
            let view = view.clone();
            move |url: &str, _window: &mut Window, cx: &mut App| {
                let url = url.to_string();
                view.update(cx, |v, cx| v.handle_link_click(&url, cx));
            }
        });
        let active_search = if self.active_search_query.trim().is_empty() {
            None
        } else {
            Some(self.active_search_query.as_str())
        };
        let render_page =
            |id: String,
             section_idx: usize,
             dom: Option<&ebook_rs::EbookDomTree>,
             fallback_html: String,
             sel_handle: Option<gpui_kit::base::TextSelectionHandle>| {
                let page_div = div()
                    .min_w(px(0.0))
                    .max_w(px(max_width))
                    .w_full()
                    .h_full()
                    .px(px(margin))
                    .text_size(px(font_size))
                    .line_height(px(font_size * line_height))
                    .font_family(crate::renderer::resolve_font_family(&self.font_family))
                    .text_align(TextAlign::Left)
                    .overflow_hidden();

                if let Some(dom) = dom {
                    let sec_annotations: Vec<Annotation> = self
                        .annotations
                        .iter()
                        .filter(|a| a.section == section_idx)
                        .cloned()
                        .collect();
                    let cfg = RenderConfig {
                        id_prefix: &id,
                        theme: &self.theme,
                        palette,
                        base_font_size: font_size,
                        line_height,
                        font_family: &self.font_family,
                        text_align: &self.text_align,
                        rtl_mode: self.rtl_mode,
                        column_px,
                        annotations: &sec_annotations,
                        active_search,
                        selection_handle: sel_handle,
                        image_cache: &self.image_cache,
                        on_link_click: on_link_click.clone(),
                    };
                    let rendered = render_ebook_dom(dom, &cfg);
                    if section_idx == self.current {
                        page_div
                            .relative()
                            .child(
                                div()
                                    .id(SharedString::from(format!("{id}-scroll")))
                                    .w_full()
                                    .h_full()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll_handle)
                                    .py_2()
                                    .child(rendered),
                            )
                            .child(Scrollbar::vertical(&self.scroll_handle))
                    } else {
                        page_div.child(
                            div()
                                .id(SharedString::from(format!("{id}-scroll")))
                                .w_full()
                                .h_full()
                                .overflow_y_scrollbar()
                                .py_2()
                                .child(rendered),
                        )
                    }
                } else {
                    page_div.child(
                        TextView::html(id.clone(), fallback_html)
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
                }
            };
        let margin_prev = div().flex().items_center().px_1().child(
            Button::new("edge-prev")
                .ghost()
                .label(if self.rtl_mode {
                    "\u{203a}"
                } else {
                    "\u{2039}"
                })
                .tooltip("Previous section (←)")
                .on_click(cx.listener(|v, _, _, cx| v.prev(cx))),
        );
        let margin_next = div().flex().items_center().px_1().child(
            Button::new("edge-next")
                .ghost()
                .label(if self.rtl_mode {
                    "\u{2039}"
                } else {
                    "\u{203a}"
                })
                .tooltip("Next section (→)")
                .on_click(cx.listener(|v, _, _, cx| v.next(cx))),
        );

        let content = h_flex()
            .min_w(px(0.0))
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .h_full()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|v, _, window, cx| {
                    v.focus_handle.focus(window, cx);
                }),
            )
            .child(margin_prev)
            .child(if self.two_page {
                h_flex()
                    .min_w(px(0.0))
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .child(render_page(
                        "content".to_string(),
                        self.current,
                        self.cached_dom.as_ref(),
                        html.clone(),
                        self.page1_selection.clone(),
                    ))
                    .child(render_page(
                        "content2".to_string(),
                        self.current + 1,
                        dom2.as_ref(),
                        String::new(),
                        self.page2_selection.clone(),
                    ))
                    .into_any_element()
            } else {
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .flex()
                    .justify_center()
                    .child(render_page(
                        "content".to_string(),
                        self.current,
                        self.cached_dom.as_ref(),
                        html.clone(),
                        self.page1_selection.clone(),
                    ))
                    .into_any_element()
            })
            .child(margin_next);

        let sidebar_w = self.sidebar_width;
        let sidebar_panel = div()
            .w(px(sidebar_w))
            .min_w(px(sidebar_w))
            .max_w(px(sidebar_w))
            .flex_shrink(0.0)
            .h_full()
            .border_r_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .overflow_hidden()
            .child(
                v_flex().size_full().child(sidebar_buttons).child(
                    div()
                        .id("sidebar-scroll")
                        .flex_1()
                        .overflow_y_scrollbar()
                        .child(sidebar),
                ),
            );

        let body = if self.zen_mode || !self.sidebar_open {
            h_flex().flex_1().size_full().min_w(px(0.0)).child(content)
        } else {
            h_flex()
                .flex_1()
                .size_full()
                .min_w(px(0.0))
                .child(sidebar_panel)
                .child(content)
        };

        let on_key = cx.listener(|view, event: &KeyDownEvent, window, cx| {
            let key = event.keystroke.key.to_ascii_lowercase();
            let ctrl = event.keystroke.modifiers.control;
            let alt = event.keystroke.modifiers.alt;

            if key == "escape" {
                if view.passphrase_modal {
                    view.cancel_passphrase(cx);
                } else if view.export_modal {
                    view.export_modal = false;
                    cx.notify();
                } else if view.modal.is_some() {
                    view.close_modal(cx);
                } else if view.show_find_bar {
                    view.show_find_bar = false;
                    view.active_search_query.clear();
                    view.load_current(cx);
                    cx.notify();
                } else if view.zen_mode {
                    view.zen_mode = false;
                    cx.notify();
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
                match key.as_str() {
                    "o" => {
                        if let Some(path) = pick_ebook_file() {
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
                    "\\" => {
                        view.sidebar_open = !view.sidebar_open;
                        cx.notify();
                    }
                    "l" => {
                        window.set_window_title("eParna");
                        view.go_library(cx);
                    }
                    _ => {}
                }
            } else if alt {
                if key == "arrowleft" || key == "left" {
                    view.jump_back(cx);
                }
            } else {
                match key.as_str() {
                    "right" | "arrowright" | "pagedown" | "j" => view.next(cx),
                    "left" | "arrowleft" | "pageup" | "k" => view.prev(cx),
                    "backspace" if !view.show_library => view.jump_back(cx),
                    "z" if !view.show_library => view.toggle_zen_mode(cx),
                    "p" if !view.show_library => {
                        view.panel = if view.panel == Panel::Typography {
                            Panel::None
                        } else {
                            Panel::Typography
                        };
                        cx.notify();
                    }
                    " " | "space" => {
                        if event.keystroke.modifiers.shift {
                            view.prev(cx);
                        } else {
                            view.next(cx);
                        }
                    }
                    "home" => view.jump(0, cx),
                    "end" => {
                        if let Some(b) = &view.book {
                            let last = b.sections().len().saturating_sub(1);
                            view.jump(last, cx);
                        }
                    }
                    "f11" => view.toggle_fullscreen(window, cx),
                    "?" | "f1" => view.show_shortcuts_modal(cx),
                    "/" if event.keystroke.modifiers.shift => view.show_shortcuts_modal(cx),
                    "+" | "=" => view.adjust_font_size(1.0, cx),
                    "-" | "_" => view.adjust_font_size(-1.0, cx),
                    "m" if !view.show_library => view.cycle_theme(cx),
                    "r" if !view.show_library => view.toggle_rtl(cx),
                    "v" if !view.show_library => view.toggle_tts(window, cx),
                    "t" if !view.show_library => {
                        view.sidebar_open = true;
                        view.sidebar_mode = SidebarMode::Contents;
                        cx.notify();
                    }
                    "s" if !view.show_library => {
                        view.sidebar_open = true;
                        view.sidebar_mode = SidebarMode::Search;
                        cx.notify();
                    }
                    "a" if !view.show_library => {
                        view.sidebar_open = true;
                        view.sidebar_mode = SidebarMode::Annotations;
                        cx.notify();
                    }
                    "d" if !view.show_library => {
                        view.sidebar_open = true;
                        view.sidebar_mode = SidebarMode::Dictionary;
                        cx.notify();
                    }
                    "b" if !view.show_library => view.toggle_bookmark(window, cx),
                    _ => {}
                }
            }
        });

        let overlay: Option<AnyElement> =
            if self.export_modal {
                const EXPORT_GROUPS: &[(&str, &[(&str, &str)])] = &[
                    (
                        "E-Readers",
                        &[
                            ("EPUB 3", "epub"),
                            ("Kobo KEPUB", "kepub.epub"),
                            ("Amazon KFX", "kfx"),
                            ("Kindle MOBI", "mobi"),
                            ("Kindle AZW3", "azw3"),
                        ],
                    ),
                    (
                        "Documents",
                        &[
                            ("PDF", "pdf"),
                            ("Word DOCX", "docx"),
                            ("OpenDocument ODT", "odt"),
                            ("Rich Text RTF", "rtf"),
                            ("PowerPoint PPTX", "pptx"),
                            ("FictionBook 2", "fb2"),
                        ],
                    ),
                    (
                        "Markup & Web",
                        &[
                            ("Markdown", "md"),
                            ("Plain Text", "txt"),
                            ("HTML5", "html"),
                            ("HTMLZ", "htmlz"),
                            ("TXTZ", "txtz"),
                            ("OEB Package", "oeb"),
                            ("ZIP Bundle", "zip"),
                        ],
                    ),
                    (
                        "Academic & Technical",
                        &[
                            ("LaTeX (.tex)", "tex"),
                            ("Typst (.typ)", "typ"),
                            ("AsciiDoc (.adoc)", "adoc"),
                            ("Org-mode (.org)", "org"),
                            ("reST (.rst)", "rst"),
                        ],
                    ),
                    (
                        "Legacy & AI",
                        &[
                            ("Microsoft Reader (.lit)", "lit"),
                            ("PalmDB (.pdb)", "pdb"),
                            ("PMLZ", "pmlz"),
                            ("Rocket eBook (.rb)", "rb"),
                            ("Psion TCR", "tcr"),
                            ("RAG Chunks (.json)", "json"),
                        ],
                    ),
                ];
                let mut groups_col = v_flex().gap_3();
                for &(group_name, formats) in EXPORT_GROUPS {
                    let mut row = h_flex().gap_2().flex_wrap();
                    for &(label, ext) in formats {
                        let lbl = label.to_string();
                        let ext_s = ext.to_string();
                        row = row.child(
                            Button::new(format!("exp-fmt-{ext}"))
                                .outline()
                                .label(label)
                                .on_click(cx.listener(move |v, _, window, cx| {
                                    v.export_book_as(&lbl, &ext_s, window, cx);
                                })),
                        );
                    }
                    groups_col = groups_col.child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.muted)
                                    .child(group_name.to_uppercase()),
                            )
                            .child(row),
                    );
                }
                Some(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00000088))
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_6()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _, _, cx| {
                                v.export_modal = false;
                                cx.notify();
                            }),
                        )
                        .child(
                            v_flex()
                                .w(px(640.0))
                                .max_h(px(560.0))
                                .gap_3()
                                .rounded_lg()
                                .bg(palette.bg)
                                .text_color(palette.fg)
                                .border_1()
                                .border_color(palette.border)
                                .p_4()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(
                                    h_flex()
                                        .justify_between()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_size(px(16.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child("Export / Convert Book"),
                                        )
                                        .child(
                                            Button::new("exp-modal-x")
                                                .ghost()
                                                .label("\u{2715}")
                                                .tooltip("Close (Esc)")
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.export_modal = false;
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("exp-modal-scroll")
                                        .flex_1()
                                        .overflow_y_scrollbar()
                                        .child(groups_col),
                                )
                                .child(
                                    h_flex()
                                        .justify_between()
                                        .items_center()
                                        .child(
                                            Button::new("exp-all-dialog")
                                                .ghost()
                                                .label("Custom filename\u{2026}")
                                                .on_click(cx.listener(|v, _, window, cx| {
                                                    v.export_modal = false;
                                                    v.export_book_dialog(window, cx);
                                                })),
                                        )
                                        .child(
                                            Button::new("exp-modal-close")
                                                .ghost()
                                                .label("Cancel")
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.export_modal = false;
                                                    cx.notify();
                                                })),
                                        ),
                                ),
                        )
                        .into_any_element(),
                )
            } else if self.modal.is_some() {
                self.render_modal_overlay(palette, cx)
            } else if self.passphrase_modal {
                Some(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00000088))
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_6()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _, _, cx| v.cancel_passphrase(cx)),
                        )
                        .child(
                            v_flex()
                                .w(px(420.0))
                                .gap_3()
                                .rounded_lg()
                                .bg(palette.bg)
                                .text_color(palette.fg)
                                .border_1()
                                .border_color(palette.border)
                                .p_4()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Passphrase required"),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .text_color(palette.muted)
                                        .child(self.passphrase_error.clone()),
                                )
                                .child(Input::new(&self.passphrase_input))
                                .child(h_flex().justify_end().gap_2().children([
                                    Button::new("pp-cancel").ghost().label("Cancel").on_click(
                                        cx.listener(|v, _, _, cx| v.cancel_passphrase(cx)),
                                    ),
                                    Button::new("pp-submit").primary().label("Open").on_click(
                                        cx.listener(|v, _, window, cx| {
                                            v.submit_passphrase(window, cx)
                                        }),
                                    ),
                                ])),
                        )
                        .into_any_element(),
                )
            } else {
                None
            };

        let status_line = if self.selection_status.is_empty() {
            status
        } else {
            format!("{} — {}", status, self.selection_status)
        };

        let total_secs = self.book.as_ref().map(|b| b.sections().len()).unwrap_or(0);
        let scrubber_preview = std::sync::Arc::new({
            let total_secs = total_secs.max(1);
            move |pct: f32| {
                let sec_num =
                    (((pct / 100.0) * total_secs as f32).floor() as usize + 1).min(total_secs);
                format!("Sec {sec_num}/{total_secs} · {pct:.1}%")
            }
        });
        let scrubber_seek = std::sync::Arc::new({
            let view = view.clone();
            move |pct: f32, _window: &mut Window, cx: &mut App| {
                view.update(cx, |v, cx| v.jump_to_percentage(pct, cx));
            }
        });
        let accent_color: Hsla = if is_palette_dark(&self.theme, &palette) {
            rgb(0x60a5fa).into()
        } else {
            rgb(0x2563eb).into()
        };

        let bottom_bar = v_flex()
            .w_full()
            .bg(palette.surface)
            .border_t_1()
            .border_color(palette.border)
            .px_3()
            .py_1()
            .gap_1()
            .child(ProgressScrubber {
                id: ElementId::Name("prog-scrubber".into()),
                progress,
                ticks: self.chapter_ticks.clone(),
                track_color: palette.border,
                fill_color: accent_color,
                tick_color: palette.muted,
                thumb_color: accent_color,
                preview_bg: palette.surface,
                preview_fg: palette.fg,
                preview_border: palette.border,
                format_preview: scrubber_preview,
                on_seek: scrubber_seek,
            })
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(palette.muted)
                            .child(status_line),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                Button::new("prev")
                                    .ghost()
                                    .label(if self.rtl_mode {
                                        "\u{203a}"
                                    } else {
                                        "\u{2039}"
                                    })
                                    .tooltip("Previous section (←)")
                                    .on_click(on_prev),
                            )
                            .child(
                                Button::new("pos-jump")
                                    .ghost()
                                    .label(format!("{pos} ({progress:.0}%)"))
                                    .tooltip("Go to section or percentage")
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.panel = if v.panel == Panel::Actions {
                                            Panel::None
                                        } else {
                                            Panel::Actions
                                        };
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("next")
                                    .ghost()
                                    .label(if self.rtl_mode {
                                        "\u{2039}"
                                    } else {
                                        "\u{203a}"
                                    })
                                    .tooltip("Next section (→)")
                                    .on_click(on_next),
                            ),
                    ),
            );

        let mut root = if self.zen_mode
            && self.modal.is_none()
            && !self.passphrase_modal
            && !self.export_modal
        {
            v_flex()
                .relative()
                .size_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .bg(bg)
                .text_color(fg)
                .track_focus(&self.focus_handle)
                .on_key_down(on_key)
                .child(div().flex_1().size_full().overflow_hidden().child(body))
                .child(
                    div().absolute().top_2().right_3().child(
                        Button::new("exit-zen")
                            .ghost()
                            .label("Exit Zen (Esc)")
                            .on_click(cx.listener(|v, _, _, cx| v.toggle_zen_mode(cx))),
                    ),
                )
                .child(
                    div()
                        .absolute()
                        .bottom_2()
                        .right_4()
                        .text_size(px(11.0))
                        .text_color(palette.muted)
                        .child(format!("{progress:.0}%")),
                )
        } else {
            v_flex()
                .relative()
                .size_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .bg(bg)
                .text_color(fg)
                .track_focus(&self.focus_handle)
                .on_key_down(on_key)
                .child(header)
                .child(find_bar)
                .child(body)
                .child(bottom_bar)
        };

        if let Some(pop) = floating_popover
            && !self.zen_mode
            && overlay.is_none()
        {
            root = root.child(pop);
        }

        if let Some(ov) = overlay {
            root = root.child(ov);
        }

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
