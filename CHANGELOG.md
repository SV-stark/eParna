# Changelog

All notable changes to **eParna Reader** are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Amazon KFX (.kfx) Export**: Added native KFX export via `Book::export_kfx_bytes()` in the Actions panel, enabling export from any supported ebook format to clean Kindle Format 10 containers.
- **Academic Citation Exporter**: Added interactive Citations modal exporting BibTeX, APA (7th ed.), MLA (9th ed.), and Chicago (17th ed.) citations via `ebook-rs`.
- **Page Geometry & Viewport Dimensions**: Book Info now inspects and displays exact page dimensions (width × height pt) for PDF documents and Fixed-Layout EPUB sections.
- **Automatic Language Detection**: Book Info now displays trigram-based auto-detected language in addition to metadata language tags.
- **Structured Markdown & Table Styling**: Theme CSS resets now properly style tables, code blocks, and blockquotes across Dark, Sepia, Nord, Solarized, and E-Ink themes.

### Changed & Improved
- **Upgraded `ebook-rs` to `v0.18.0`**:
  - Pure in-memory PDF parsing via `unpdf 0.51` with zero disk spooling (no temporary file creation or deletion).
  - Rich PDF metadata extraction (real document title, author, subject, keywords).
  - Two-column reading-order reflow and structured Markdown extraction for PDF pages.
- **Dependency Graph Hardening & Pruning**:
  - Completely removed the `office_oxide = "=0.1.11"` transitive pin, pruning over 800 lines of heavy dependencies including `office_oxide`, `pdf_oxide`, `ttf-parser`, and vulnerable `zip 8.6.0`.
  - Updated `rustls-webpki` to `0.103.16` and `webpki-roots` to `1.0.11`.
  - Cleaned up obsolete advisories in `deny.toml`.

## [0.7.0] - 2026-10-27

### Added
- Manga & Comic RTL reading flow: auto-detects `PageProgressionDirection::Rtl`,
  inverts navigation keys and buttons, and provides status and toggle action.
- In-text visual highlights: persistent `<mark>` style highlight overlay
  rendered directly in the chapter text without breaking layout or tags.
- In-page search highlight and find bar (`Ctrl+F`) with query match highlighting
  and clear action.
- Dark mode & Sepia CSS reset immunity: overrides hardcoded dark publisher inline
  text styling for readability in dark and sepia modes.
- Library view toggle: switch between visual cover grid and detailed tabular list
  view with thumbnail, reading progress, and quick actions.
- Expanded typeface collection: added `OpenDyslexic`, `Bookerly`, `Georgia`, and
  `Courier New`.
- Media overlay inspection: inspect SMIL audio read-along tracks in book metadata.
- Modular architecture: refactored monolithic codebase into `models`, `config`,
  `stardict`, `html_utils`, and `main`.

## [0.6.0] - 2026-10-19

### Added
- Keyboard shortcuts: `b` toggles a bookmark, `t`/`s`/`a` switch the sidebar
  to Contents/Search/Annotations (in addition to the existing arrows, Home/End,
  F11, +/-, Ctrl+O/F/B/C/L).
- Custom theme: a "Custom" theme option with user-chosen background and
  foreground hex colours (editable in the Typography panel).
- Custom theme: a "Custom" theme option with user-chosen background and
  foreground hex colours (editable in the Typography panel).
- Reading statistics: the status line now shows the current section's word
  count and the session reading time (minutes).
- Nested TOC display: child entries are indented in the TOC selector so the
  hierarchy survives the flat dropdown.
- Print page numbers: when the book has a page list, the current print page
  ("p. N") is shown in the status line.
- Image lightbox: clicking an image in the reading view opens a modal
  showing the full-size image (images are wrapped in `img:` links in
  `prepare_section_html` since GPUI has no dedicated image-click handler).
- Footnote pop-ups: clicking a footnote reference in the reading view opens
  a modal showing the note (via GPUI's `on_link_click` on the HTML
  `TextView`). Cross-document footnote links navigate to the target section.
- Dictionary: a "Lookup selection" button looks up the current text selection
  (useful for double-clicked words) in addition to the typed input.
- Offline StarDict dictionary support: a pure-Rust, MIT-licensed reader for
  the StarDict format (`.ifo`/`.idx`/`.dict` or `.dict.dz`), loaded via a
  "Load StarDict" button in the Dictionary sidebar. Lookup prefers the
  offline dictionary and falls back to the online dictionary API. (The
  `stardict` crate is GPL-2.0 and incompatible with eparna's MIT licence,
  so the format is parsed directly.)
- LCP/DRM passphrase prompt: opening a protected book now shows a modal
  asking for the passphrase and retries via
  `Book::from_bytes_with_passphrase` (ebook-rs 0.17.3). Previously the error
  was swallowed.

### Changed
- Reading progress is now char-accurate: the progress bar and the persisted
  CFI use the Readium location map (`locations_from_sections` /
  `percentage_from_cfi`) instead of a coarse section-count ratio. Resume uses
  `get_section_and_offset_by_cfi` so the in-section offset is preserved in
  the CFI (scroll-to-offset is deferred until GPUI exposes programmatic
  scroll for HTML views).
- Upgraded `ebook-rs` `0.17.1` → `0.17.3`: SVG-wrapped covers are now
  inlined upstream, `<title>` is stripped from `processed_html`, CSS is
  available as a merged `<style>` block (`AssetDeliveryStrategy::InlinedStyle`
  + `Book::get_merged_stylesheet`), the DOM layer gained a selector engine
  with computed styles (`EbookDomTree::query_selector_all` / `style_of`), LCP
  books can be opened with a passphrase
  (`from_bytes_with_title_and_passphrase`), and CFI offsets are preserved
  (`Book::get_section_and_offset_by_cfi` / `Book::resolve_cfi`). The
  eparna-side SVG/`<title>` workarounds in `prepare_section_html` are now
  redundant but harmless.
- Bumped MSRV from `1.85` → `1.90` (edition 2024; `ebook-rs 0.17.3`
  requires Rust 1.88 and the locked graph, e.g. `ordered-float 5.5.0`,
  requires Rust 1.90). `rust-version` in `Cargo.toml`, the README install
  note, and `cargo-deny` are kept in sync.
- `zip` moved `7.4.0` → `9.0.5` in the direct dependency (ebook-rs 0.17.3),
  deduplicating the `pdf_oxide`/`office_oxide` transitive graph. The
  RUSTSEC-2026-0367/0368/0369 ignores are retained because `office_oxide
  0.1.11` (pinned for `pdf_oxide 0.3.78`) still pulls `zip 8.6.0`.

## [0.5.0] - 2026-10-19

### Added
- Text selection in the reading view (drag to select), with **Copy** / `Ctrl+C`
  to copy the selection and **Highlight** to save it as an annotation.
- Highlights capture the selected passage; notes now quote whatever is
  selected, so a note is readable against the text it refers to.
- Annotations are exported as Markdown (`annotations.md`, replacing the old
  plain-text `annotations.txt`) with block-quoted passages.
- Clicking an annotation in the sidebar jumps back to its section.
- Reading position is now persisted as a Readium CFI
  (`Book::to_readium_locator` / `get_section_by_cfi`), so resume survives a
  re-parse or a re-converted copy instead of drifting to the wrong section.
- KFX and KEPUB added to the file-picker filters; `ebook-rs` already
  auto-detected them (`KfxBook::is_kfx`).

### Changed
- **Toolbar restructured.** The primary bar now carries only the reading
  actions (Open, Library, ‹ ›, Bookmark, Highlight, Copy) together with the
  `n/N (pct%)` readout, an **Aa** typography toggle and a **⋯** actions
  toggle. Everything else moved into two collapsible second bars, so the
  toolbar stopped being a wall of twenty-odd buttons.
- The typography bar uses labelled `− value +` steppers (Size, Line height,
  Margin, Text width) plus Typeface / Theme / Pages cycles and
  **Reset to defaults**. The actions bar holds Book info, Validate book,
  Save as EPUB, Export notes, go-to-section, Fullscreen and New window.
  Long reports (book info, validation) now open in a scrollable modal
  instead of a transient toast.
- **Library redesigned.** The duplicated "Recent books" heading is gone; the
  page is now a cover-forward view with a "Continue reading" row of wide
  tiles for books in progress, followed by an `All books (N)` grid of
  fixed-size tiles showing cover, title, author and format with a progress
  bar and Open/Resume, favourite and remove actions. Library search now
  matches the author as well as the title.
- Sidebar mode buttons are laid out two per row, and the status line reads
  `title · author · n/N sections`.
- Version bumped to 0.5.0.

### Fixed
- Cover pages now render. Many EPUBs set their cover as
  `<svg><image xlink:href="cover.jpeg"/></svg>`, which the HTML renderer cannot
  draw; such `<image>` elements are now rewritten to `<img>` with the resource
  inlined as a `data:` URI.
- The document head no longer leaks into the page: a book whose XHTML carries
  `<title>Unknown</title>` was showing the word "Unknown" as the first line of
  every section, because the renderer treats every element's text as body copy.
  `<head>` and `<title>` blocks are now stripped before rendering.
- The first-run window no longer opens larger than the screen. GPUI's default
  inherits the display's *device* size while window bounds are logical, which
  on HiDPI displays made the window wider and taller than the screen and put
  the right-hand end of the toolbar permanently off-screen.
- Wide book content can no longer blow the layout out: the content and
  sidebar panes are given an explicit zero minimum width, so a long line of
  text no longer forces the whole window wider than its bounds.
- Opening a book from the library no longer overwrites the CFI the reader
  just resolved, so **Resume** returns to the saved passage.
- Images that carry no explicit size are constrained to the text column, so
  full-bleed figures no longer overflow the page.

### Notes
- The claim that gpui-kit had no text-selection support was wrong: `TextView`
  exposes `.selectable(true)` and `gpui::TextSelection::selected_text()`.
- DRM (Readium LCP) is still unsupported and is now documented as a known gap:
  `ebook-rs` returns `EbookError::DrmProtected` when a container contains
  `META-INF/license.lcpl`, so no passphrase path is reachable from outside.
- Code signing remains outstanding; it needs a purchased certificate
  (SSL.com or Azure Artifact Signing) and repository secrets.

## [0.4.0] - 2026-10-18

### Added
- Regex in-book search (`Regex: on/off`) with a configurable result cap.
- Deep table-of-contents search with breadcrumbs ("Find in TOC").
- Richer book info: author, publisher, language, dates, subjects, identifier,
  rights and description.
- Structural validation report ("Validate").
- Cover thumbnails in the library (cached under the app data directory).
- Library deduplication by content hash: reopening the same book from another
  path updates the existing entry instead of adding a duplicate.
- "Save as EPUB" conversion for any supported format.
- Enabled additional `ebook-rs` features: `pdf`, `parallel`, `mmap`, `zstd`
  (PDF parsing, parallel ZIP export, memory-mapped I/O, Zstd state caching).
- Automated cross-platform releases with [cargo-dist](https://axodotdev.github.io/cargo-dist/)
  (`.github/workflows/release.yml`): Windows MSI, macOS PKG (arm64 + x64),
  Linux tarball, `curl|sh` / `irm|iex` installers, and SHA-256 checksums.
- Linux AppImage built by a dedicated job using `cargo-bundle` (cargo-dist has
  no AppImage support).
- CI workflow running `rustfmt`, `clippy -D warnings` and a release build on
  Linux, Windows and macOS.
- Application icon (SVG-free PNG set) used by the installers.

### Changed
- Version bumped to 0.4.0.
- Pinned `office_oxide` to `0.1.11`: `0.1.12` added a required `defined_names`
  field to `DocumentIR`, which broke `pdf_oxide 0.3.78`'s struct literal and
  prevented the `pdf` feature from compiling.

## [0.3.0] - 2026-10-18

### Added
- Footnotes panel listing footnotes for the current section.
- Two-page (side-by-side) reading layout.
- Dictionary lookup (dictionaryapi.dev) and Wikipedia summary lookup.
- OPDS catalog support: fetch a catalog, browse entries, download and open a book.
- Library filtering and sorting (Recent / Title / Path).
- Favourites (star) with a favourites-only library filter.
- Multiple reader windows ("New window").
- Reading percentage alongside the `n/total` section indicator.

### Changed
- Renamed the application from **ePustak** to **eParna** (from Sanskrit *पर्ण*,
  "leaf"). Crate/binary is now `eparna`; settings live in `%APPDATA%\eParna` and
  existing `%APPDATA%\ePustak` state is migrated automatically on first launch.
- App now runs on a worker thread with a large stack so unoptimized (debug)
  builds no longer overflow the default 1 MB stack while rendering.
- Added `ureq` HTTP client dependency for OPDS and lookup features.

## [0.2.0] - 2026-10-18

### Added
- Library view with recent books, reopen and removal.
- Table of contents selector for chapter jumps.
- Bookmarks with a dedicated selector.
- Annotations/notes per section with persistence.
- In-book full-text search with snippets and jump-to-result.
- Themes: Light, Dark, Sepia.
- Typography controls: font size, font family, line height, margins, max content width.
- Reading progress bar and section indicator.
- Fullscreen toggle.
- Persistent application state (recent books, annotations, settings).
- Keyboard shortcuts for navigation and common actions.
- Book info dialog and annotation export.
- Go-to-section input.

## [0.1.0] - 2026-10-18

### Added
- Initial release of the eParna Reader desktop application.
- Multi-format ebook loading via `ebook-rs` (EPUB, MOBI, AZW3, FB2, LIT, CBZ,
  PDF, ODT, DOCX, RTF, TXT, Markdown) with automatic format detection.
- GPU-accelerated UI built with `gpui-kit`, including a scrollable HTML content view.
- Native file picker (via `rfd`) for opening books.
- Command-line argument support to open a book on startup.
- Prev/Next section navigation with a live position indicator (`n/total`).
- Status bar showing book title and section count.
- Release profile tuned with LTO and `opt-level = 3` for fast startup and rendering.

### Known limitations
- Navigation is section-based; in-book search, bookmarks and table-of-contents jumping are not yet implemented.
- Rendering relies on `TextView` HTML support; complex CSS/layout may be simplified.