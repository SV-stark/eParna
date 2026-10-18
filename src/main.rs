use ebook_rs::SearchResult;
use gpui_kit::base::{h_flex, v_flex};
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
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct Annotation {
    section: usize,
    note: String,
    created: String,
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

    fullscreen: bool,
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
        };
        if let Some(arg) = std::env::args().nth(1) {
            view.open_file(PathBuf::from(arg), window, cx);
        }
        view
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
                // Build TOC
                let mut items: Vec<(String, usize)> = Vec::new();
                for flat in book.flatten_toc() {
                    let href = flat.href.split('#').next().unwrap_or(&flat.href);
                    let idx = book.sections().iter().position(|s| {
                        s.href == href || s.full_path.ends_with(href) || href.ends_with(&s.href)
                    });
                    if let Some(i) = idx
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
                let key = path.to_string_lossy().to_string();
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
                self.state.recent.retain(|r| r.path != key);
                self.state.recent.insert(
                    0,
                    RecentBook {
                        path: key.clone(),
                        title: title.clone(),
                        section: 0,
                        opened: now,
                        favorite: was_fav,
                    },
                );
                if self.state.recent.len() > 50 {
                    self.state.recent.truncate(50);
                }
                self.save_state();

                self.toc_items = items;
                self.toc_state = Some(toc_state);
                self.book = Some(book);
                self.book_path = Some(path);
                self.current = 0;
                self.search_results.clear();
                self.show_library = false;
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
        // persist progress
        if let Some(path) = &self.book_path {
            let key = path.to_string_lossy().to_string();
            if let Some(r) = self.state.recent.iter_mut().find(|r| r.path == key) {
                r.section = self.current;
            }
            self.save_state();
        }
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
            self.search_results = book.search(&q);
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

    fn add_annotation(&mut self, cx: &mut Context<Self>) {
        if self.book.is_none() {
            return;
        }
        let note = self.annotation_input.read(cx).value().to_string();
        if note.trim().is_empty() {
            return;
        }
        let ann = Annotation {
            section: self.current,
            note,
            created: format!(
                "{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            ),
        };
        self.annotations.push(ann);
        // persist
        if let Some(path) = &self.book_path {
            let key = path.to_string_lossy().to_string();
            self.state.annotations.insert(key, self.annotations.clone());
            self.save_state();
        }
        cx.notify();
    }

    fn change_font(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.font_size = (self.font_size + delta).clamp(10.0, 40.0);
        self.sync_state();
        cx.notify();
    }

    fn go_library(&mut self, cx: &mut Context<Self>) {
        self.show_library = true;
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
                        "cbz", "lit",
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
        v = v.child(div().text_size(px(16.0)).child("Recent books"));
        for r in recent {
            if self.library_favorites_only && !r.favorite {
                continue;
            }
            if !q.is_empty()
                && !r.title.to_lowercase().contains(&q)
                && !r.path.to_lowercase().contains(&q)
            {
                continue;
            }
            let path = r.path.clone();
            let title = if r.title.is_empty() {
                r.path.clone()
            } else {
                r.title.clone()
            };
            let sec = r.section;
            let star = if r.favorite { "★" } else { "☆" };
            let on_click = cx.listener(move |view, _ev, window, cx| {
                view.open_file(PathBuf::from(path.clone()), window, cx);
                view.current = sec;
            });
            let on_remove = {
                let p = r.path.clone();
                cx.listener(move |view, _ev, _w, cx| view.remove_recent(p.clone(), cx))
            };
            let on_fav = {
                let p = r.path.clone();
                cx.listener(move |view, _ev, _w, cx| view.toggle_favorite(p.clone(), cx))
            };
            v = v.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new(format!("fav{}", r.path))
                            .ghost()
                            .label(star)
                            .on_click(on_fav),
                    )
                    .child(
                        Button::new(format!("rb{}", r.path))
                            .label(title)
                            .on_click(on_click),
                    )
                    .child(
                        Button::new(format!("rm{}", r.path))
                            .ghost()
                            .label("×")
                            .on_click(on_remove),
                    ),
            );
        }
        if self.state.recent.is_empty() {
            v = v.child("No recent books. Open a file to get started.");
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
}

impl Render for ReaderView {
    #[allow(refining_impl_trait)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (bg, fg) = self.theme_colors();
        if self.show_library {
            return self.render_library(window, cx, bg, fg);
        }
        let status = if let Some(b) = &self.book {
            format!("{} — {} sections", b.metadata().title, b.sections().len())
        } else {
            "No book".into()
        };
        let html = self.html.clone();
        let font_size = self.font_size;
        let margin = self.margin;
        let max_width = self.max_width;
        let line_height = self.line_height;
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
                        "cbz", "lit",
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
        let on_add_note = cx.listener(|view, _ev, _w, cx| view.add_annotation(cx));
        let on_font_inc = cx.listener(|view, _ev, _w, cx| view.change_font(2.0, cx));
        let on_font_dec = cx.listener(|view, _ev, _w, cx| view.change_font(-2.0, cx));
        let on_theme = cx.listener(|view, _ev, _w, cx| {
            view.theme = if view.theme == "Light" {
                "Dark".into()
            } else if view.theme == "Dark" {
                "Sepia".into()
            } else {
                "Light".into()
            };
            view.dark = view.theme == "Dark";
            view.sync_state();
            cx.notify();
        });
        let on_lib = cx.listener(|view, _ev, window, cx| {
            window.set_window_title("eParna");
            view.go_library(cx);
        });
        let on_fs = cx.listener(|view, _ev, window, cx| view.toggle_fullscreen(window, cx));

        // sidebar content
        let sidebar = match self.sidebar_mode {
            SidebarMode::Contents => {
                if let Some(ts) = &self.toc_state {
                    v_flex()
                        .gap_2()
                        .p_2()
                        .child(Select::new(ts).placeholder("Contents"))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .child(format!("{} entries", self.toc_items.len())),
                        )
                } else {
                    v_flex().p_2().child("No contents")
                }
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
                    v = v.child(div().p_1().border_1().child(format!(
                        "#{} (sec {}) {}",
                        i + 1,
                        a.section + 1,
                        a.note.chars().take(60).collect::<String>()
                    )));
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

        let sidebar_buttons = h_flex()
            .gap_1()
            .p_2()
            .child(
                Button::new("sb_contents")
                    .ghost()
                    .label("Contents")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Contents;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_bm")
                    .ghost()
                    .label("Bookmarks")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Bookmarks;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_ann")
                    .ghost()
                    .label("Notes")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Annotations;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_search")
                    .ghost()
                    .label("Search")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Search;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_fn")
                    .ghost()
                    .label("Footnotes")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Footnotes;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("sb_dict")
                    .ghost()
                    .label("Dictionary")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sidebar_mode = SidebarMode::Dictionary;
                        cx.notify();
                    })),
            );
        let header = h_flex()
            .p_2()
            .gap_2()
            .items_center()
            .flex_wrap()
            .child(Button::new("open").label("Open").on_click(on_open))
            .child(Button::new("library").label("Library").on_click(on_lib))
            .child(Button::new("prev").label("Prev").on_click(on_prev))
            .child(Button::new("next").label("Next").on_click(on_next))
            .child(Button::new("bm").label("Bookmark").on_click(on_bm))
            .child(Button::new("fi").label("A+").on_click(on_font_inc))
            .child(Button::new("fd").label("A-").on_click(on_font_dec))
            .child(
                Button::new("ff")
                    .label("Font")
                    .on_click(cx.listener(|view, _, _, cx| {
                        let fonts = ["sans-serif", "serif", "monospace"];
                        let i = fonts
                            .iter()
                            .position(|&f| f == view.font_family)
                            .unwrap_or(0);
                        view.font_family = fonts[(i + 1) % fonts.len()].to_string();
                        view.sync_state();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("lh+")
                    .label("LH+")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.line_height = (view.line_height + 0.1).clamp(1.0, 2.5);
                        view.sync_state();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("mg+")
                    .label("Mg+")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.margin = (view.margin + 4.0).clamp(0.0, 64.0);
                        view.sync_state();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("wd+")
                    .label("W+")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.max_width = (view.max_width + 40.0).clamp(400.0, 1200.0);
                        view.sync_state();
                        cx.notify();
                    })),
            )
            .child(Button::new("th").label("Theme").on_click(on_theme))
            .child(
                Button::new("justify")
                    .label("Justify")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.justify = !view.justify;
                        view.sync_state();
                        cx.notify();
                    })),
            )
            .child(Button::new("twopage").label("2-page").on_click(cx.listener(
                |view, _, _, cx| {
                    view.two_page = !view.two_page;
                    view.sync_state();
                    cx.notify();
                },
            )))
            .child(Button::new("info").label("Info").on_click(cx.listener(
                |view, _, window, cx| {
                    if let Some(book) = &view.book {
                        let msg = format!(
                            "Title: {}\nSections: {}",
                            book.metadata().title,
                            book.sections().len()
                        );
                        use gpui_kit::component::WindowExt;
                        window.push_notification(
                            gpui_kit::component::notification::Notification::new()
                                .title("Book info")
                                .message(msg),
                            cx,
                        );
                    }
                },
            )))
            .child(Input::new(&self.goto_input).w(px(80.0)))
            .child(
                Button::new("go")
                    .label("Go")
                    .on_click(cx.listener(|view, _ev, _w, cx| {
                        if let Ok(n) = view.goto_input.read(cx).value().parse::<usize>() {
                            view.jump(n.saturating_sub(1), cx);
                        }
                    })),
            )
            .child(
                Button::new("export")
                    .label("Export notes")
                    .on_click(cx.listener(|view, _ev, _w, _cx| {
                        let mut out = String::new();
                        for a in &view.annotations {
                            out.push_str(&format!("Section {}: {}\n", a.section + 1, a.note));
                        }
                        let _ = std::fs::write("annotations.txt", out);
                    })),
            )
            .child(Button::new("fs").label("Fullscreen").on_click(on_fs))
            .child(
                Button::new("newwin")
                    .label("New window")
                    .on_click(cx.listener(|_view, _ev, _window, cx| {
                        cx.open_window(WindowOptions::default(), |window, cx| {
                            let view = cx.new(|cx| ReaderView::new(window, cx));
                            cx.new(|cx| Root::new(view, window, cx))
                        })
                        .ok();
                    })),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(12.0))
                    .child(format!("{}  ({:.0}%)", pos, progress)),
            );

        let html2 = if self.two_page {
            if let Some(book) = &self.book {
                book.get_section(self.current + 1)
                    .map(|s| s.processed_html)
                    .unwrap_or_default()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let page = |id: String, h: String| {
            div()
                .max_w(px(max_width))
                .w_full()
                .h_full()
                .px(px(margin))
                .text_size(px(font_size))
                .line_height(px(font_size * line_height))
                .font_family(self.font_family.clone())
                .text_align(TextAlign::Left)
                .overflow_hidden()
                .child(TextView::html(id.clone(), h).scrollable(true))
        };
        let content = if self.two_page {
            h_flex()
                .flex_1()
                .h_full()
                .overflow_hidden()
                .child(page("content".to_string(), html.clone()))
                .child(page("content2".to_string(), html2))
        } else {
            div()
                .flex_1()
                .h_full()
                .overflow_hidden()
                .flex()
                .justify_center()
                .child(page("content".to_string(), html.clone()))
        };

        let sidebar_panel = div().w(px(260.0)).h_full().overflow_hidden().child(
            v_flex()
                .size_full()
                .child(sidebar_buttons)
                .child(div().flex_1().overflow_hidden().child(sidebar)),
        );

        let body = h_flex().size_full().child(sidebar_panel).child(content);

        let on_key = cx.listener(|view, event: &KeyDownEvent, window, cx| {
            let key = event.keystroke.key.as_str();
            let ctrl = event.keystroke.modifiers.control;
            if ctrl {
                match key {
                    "o" => {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(
                                "Ebook",
                                &[
                                    "epub", "mobi", "azw3", "fb2", "pdf", "txt", "md", "docx",
                                    "odt", "rtf", "cbz", "lit",
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

        let root = v_flex()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .on_key_down(on_key)
            .child(header)
            .child(div().px_2().child(Progress::new("prog").value(progress)))
            .child(div().px_2().text_size(px(12.0)).child(status.clone()))
            .child(body);

        root.into_any_element()
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
            cx.spawn(async move |cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let view = cx.new(|cx| ReaderView::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("open window");
            })
            .detach();
        });
}
