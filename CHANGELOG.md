# Changelog

All notable changes to **eParna Reader** are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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