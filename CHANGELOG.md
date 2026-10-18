# Changelog

All notable changes to **eParna Reader** are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Nothing yet.

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