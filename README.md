# eParna Reader

A fast, lightweight, cross-platform desktop ebook reader written in pure Rust.

The name comes from Sanskrit **पर्ण (parṇa)** — *leaf* (and *feather/wing*). A page
is a leaf, so **eParna** is literally an "e-leaf": a light, quiet place to read.

Built on [ebook-rs](https://crates.io/crates/ebook-rs) for multi-format parsing and
[gpui-kit](https://crates.io/crates/gpui-kit) for a GPU-accelerated, native-feeling UI.

## Features

- **Multi-format support** — EPUB 2/3, MOBI, AZW3, FB2, LIT, CBZ, PDF, ODT, DOCX,
  RTF, TXT and Markdown, detected automatically from file contents.
- **Library view** — recent books with one-click reopen and removal.
- **GPU-accelerated rendering** — smooth scrolling and responsive navigation powered by gpui.
- **Inlined assets** — images and resources are embedded (base64) by ebook-rs, so sections render offline.
- **Table of contents** — jump to any chapter via the Contents selector.
- **Bookmarks** — toggle and revisit bookmarked sections.
- **Annotations** — add notes to the current section and browse them.
- **In-book search** — full-text search with result snippets and jump-to-result.
- **Themes** — Light, Dark, and Sepia.
- **Typography & layout** — adjustable font size, font family, line height, margins, max content width, two-page view.
- **Footnotes** — browse footnotes for the current section.
- **Dictionary & Wikipedia** — look up a word or topic without leaving the book.
- **Library tools** — filter and sort recent books, mark favourites, OPDS catalog browsing/download.
- **Multiple windows** — open additional reader windows.
- **Reading progress** — progress bar, percentage, and `n/total` section indicator.
- **Fullscreen** — toggle fullscreen reading.
- **Native file dialog** — open books through a platform file picker, or pass a path on the command line.
- **State persistence** — recent books, annotations, and settings saved between sessions.
- **Fast startup** — release build uses LTO and opt-level 3.

## Requirements

- Rust 1.85 or newer (edition 2024)
- A supported desktop platform (Windows, macOS, Linux) with a working graphics stack

## Build

```sh
git clone https://github.com/SV-stark/eParna.git
cd eParna
cargo build --release
```

The optimized binary is produced at `target/release/eparna` (or `eparna.exe` on Windows).

## Usage

Open a book from the command line:

```sh
eparna "path/to/book.epub"
```

Or launch without arguments and use the **Open** button to pick a file.

### Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open a book |
| `Ctrl+F` | Focus in-book search |
| `Ctrl+B` | Toggle bookmark on the current section |
| `Ctrl+L` | Back to the library |
| `→` / `PageDown` / `Space` | Next section |
| `←` / `PageUp` | Previous section |
| `Home` / `End` | First / last section |
| `F11` | Toggle fullscreen |
| `+` / `-` | Increase / decrease font size |
| `j` | Toggle justification setting |

## Privacy

Reading is fully offline. The only network requests are the ones you explicitly
trigger:

- **Dictionary / Wikipedia lookup** — queries `dictionaryapi.dev` / `en.wikipedia.org`.
- **OPDS catalogs** — fetches the catalog URL you enter and any book you open from it.

Reading state (recent books, bookmarks, notes and settings) is stored locally in
your OS configuration directory (`%APPDATA%\eParna` on Windows). Nothing is
uploaded anywhere.

## Parity with Foliate

This project targets roughly 80% feature parity with the
[Foliate](https://github.com/johnfactotum/foliate) ebook reader. Implemented so far:

library view, recent files, filtering/sorting, TOC, bookmarks, notes, in-book
search, themes, typography and layout controls, two-page view, footnotes,
dictionary/Wikipedia lookup, OPDS, progress, fullscreen, multiple windows, state
persistence and annotation export.

Known gaps (mostly limited by the current UI stack):

- Text-selection highlights (multi-colour) and pop-up footnote links.
- Text-to-speech.
- True paginated (page-flip) layout and hyphenation; scrolling is used instead.
- Cover images / grid library, metadata editing, and sync.

## Project layout

```
eParna/
├── Cargo.toml
├── src/
│   └── main.rs      # application entry point, ReaderView and render logic
├── README.md
├── CHANGELOG.md
└── LICENSE
```

## Dependencies

| Crate     | Version | Purpose                          |
|-----------|---------|----------------------------------|
| ebook-rs  | 0.17.1  | Ebook parsing & section rendering |
| gpui-kit  | 0.7.3   | UI framework & components        |
| rfd       | 0.15    | Native file dialogs              |
| serde     | 1       | State serialization              |
| ureq      | 3       | OPDS catalogs & online lookups   |

## License

Released under the [MIT License](LICENSE).

Third-party components remain under their respective licenses — see the
[ebook-rs](https://crates.io/crates/ebook-rs) and
[gpui-kit](https://crates.io/crates/gpui-kit) crates.