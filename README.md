# eParna Reader

A fast, lightweight, cross-platform desktop ebook reader written in pure Rust.

The name comes from Sanskrit **पर्ण (parṇa)** — *leaf* (and *feather/wing*). A page
is a leaf, so **eParna** is literally an "e-leaf": a light, quiet place to read.

Built on [ebook-rs](https://crates.io/crates/ebook-rs) for multi-format parsing and
[gpui-kit](https://crates.io/crates/gpui-kit) for a GPU-accelerated, native-feeling UI.

## Features

- **Multi-format support** — EPUB 2/3, KEPUB, KFX, MOBI, AZW3, FB2, LIT, CBZ, PDF,
  ODT, DOCX, RTF, TXT and Markdown, detected automatically from file contents.
- **Text selection** — drag to select, `Ctrl+C` (or **Copy**) to copy the
  selection, and **Highlight** to save it.
- **Highlights & notes** — highlights capture the quoted passage; notes quote
  whatever is selected. Click any entry to jump back to it. Export as Markdown.
- **Library view** — a cover-forward grid of books with a "Continue reading" row
  for works in progress; each tile shows cover, title, author and format with a
  progress bar and Open/Resume, favourite and remove actions. Filtering, sorting
  and one-click reopen included.
- **Deduplication** — the same book opened from another path updates the existing
  entry (content-hash based) instead of adding a duplicate.
- **GPU-accelerated rendering** — smooth scrolling and responsive navigation powered by gpui.
- **Inlined assets** — images and resources are embedded (base64) by ebook-rs, so sections render offline.
- **Table of contents** — jump to any chapter via the Contents selector, or deep-search
  the TOC by title with breadcrumbs.
- **Bookmarks** — toggle and revisit bookmarked sections.
- **Reliable resume** — reading position is stored as a Readium CFI, so you return
  to the same passage even if the book has been re-converted or re-parsed.
- **In-book search** — full-text search with result snippets, jump-to-result, regex
  mode, and a configurable result cap.
- **Book info** — author, publisher, language, dates, subjects, identifier, rights
  and description, plus a structural validation report.
- **Save as EPUB** — convert any supported format to a clean EPUB 3 file.
- **Themes** — Light, Dark, and Sepia.
- **Typography & layout** — a dedicated **Aa** panel with labelled `− value +`
  steppers for font size, line height, margin and text width, plus typeface,
  theme and one/two-page toggles and "Reset to defaults"; a **⋯** panel holds
  the long-tail actions (book info, validation, Save as EPUB, note export,
  go-to-section, fullscreen, new window).
- **Footnotes** — click a footnote reference to pop up the note inline; cross-document links navigate to the target section.
- **Dictionary & Wikipedia** — look up a word or topic without leaving the book. Offline **StarDict** dictionaries (`.ifo`/`.idx`/`.dict`) are also supported.
- **Image lightbox** — click any image in the reading view to view it full-size.
- **Nested TOC & print pages** — the TOC preserves hierarchy, and the status line shows the print page when the book has a page list.
- **Reading statistics** — section word count and session reading time in the status line.
- **Custom theme** — a "Custom" theme with user-chosen background/foreground colours.
- **LCP/DRM passphrase** — protected books can be opened by entering a passphrase.
- **OPDS catalogues** — browse and download from OPDS feeds.
- **Multiple windows** — open additional reader windows.
- **Reading progress** — progress bar, percentage, and `n/total` section indicator.
- **Fullscreen** — toggle fullscreen reading.
- **Native file dialog** — open books through a platform file picker, or pass a path on the command line.
- **State persistence** — recent books, annotations, and settings saved between sessions.
- **Fast startup** — release build uses LTO and opt-level 3.

## Requirements

- Rust 1.90 or newer (edition 2024; the MSRV is declared as `rust-version` in `Cargo.toml`)
- A supported desktop platform (Windows, macOS, Linux) with a working graphics stack

## Build

```sh
git clone https://github.com/SV-stark/eParna.git
cd eParna
cargo build --release
```

The optimized binary is produced at `target/release/eparna` (or `eparna.exe` on Windows).

## Install

Prebuilt artifacts are attached to every [GitHub Release](https://github.com/SV-stark/eParna/releases).

| Platform | Artifact | Notes |
|---|---|---|
| Windows x64 | `eparna-x86_64-pc-windows-msvc.msi` | Standard Windows installer |
| Windows x64 | `eparna-x86_64-pc-windows-msvc.zip` | Portable build, no install needed |
| macOS (Apple Silicon) | `eparna-aarch64-apple-darwin.pkg` | Double-click installer |
| macOS (Intel) | `eparna-x86_64-apple-darwin.pkg` | Double-click installer |
| Linux x64 | `eparna_0.6.0_x86_64.AppImage` | Single file; `chmod +x` and run |
| Any | `eparna-installer.sh` / `eparna-installer.ps1` | `curl \| sh` / `irm \| iex` |

Or install with the one-liner scripts:

```sh
curl -fsSL https://github.com/SV-stark/eParna/releases/latest/download/eparna-installer.sh | sh
```

```powershell
irm https://github.com/SV-stark/eParna/releases/latest/download/eparna-installer.ps1 | iex
```

Every artifact ships with a `.sha256` sidecar (plus a combined `sha256.sum`),
so downloads can be verified.

> The macOS `.pkg` and Windows `.msi` are unsigned, so Gatekeeper/SmartScreen
> will warn on first launch. Remove the quarantine flag on macOS with
> `xattr -d com.apple.quarantine /Applications/eParna.app`.

### Building releases

Releases are automated with [cargo-dist](https://axodotdev.github.io/cargo-dist/):

```sh
cargo install cargo-dist   # or: dist
dist init --yes            # configure + generate .github/workflows/release.yml
git tag -a v0.5.0 -m "..." && git push --tags
```

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds every
target and publishes the artifacts to a GitHub Release. Config lives in
[`dist-workspace.toml`](dist-workspace.toml).

> **Note on AppImage:** cargo-dist 0.32 does not support AppImage or DMG
> installers (both are still open requests upstream). Windows uses the built-in
> `msi` installer and macOS the built-in `pkg` installer; the Linux AppImage is
> produced by a dedicated `appimage` job in `release.yml` that uses
> `cargo-bundle`. That job is hand-maintained, which is why `allow-dirty = ["ci"]`
> is set in `dist-workspace.toml` — `dist generate` will not overwrite it.
>
> **Note:** cargo-bundle builds a bare AppImage — unlike tools wrapping
> `linuxdeploy`/`appimagetool`, it does **not** collect shared libraries. The
> AppImage therefore needs the usual Wayland/X11/Vulkan runtime libraries
> present on the host system. On a normal desktop distro they already are; on a
> minimal container, install them or use the `.tar.xz` artifact instead.

## Continuous integration

Every push and pull request runs the following:

| Workflow | Purpose |
|---|---|
| [`ci.yml`](.github/workflows/ci.yml) | `rustfmt --check`, `clippy -D warnings`, and a release build on Linux, Windows and macOS. Passes `--locked`, so CI can never silently re-resolve dependencies. |
| [`codeql.yml`](.github/workflows/codeql.yml) | GitHub's static analysis for Rust, on push/PR plus a weekly sweep. |
| [`deny.yml`](.github/workflows/deny.yml) | `cargo-deny`: RustSec advisories, licence policy and crates.io-only sourcing. |
| [`dependency-review.yml`](.github/workflows/dependency-review.yml) | Scans the dependency *diff* of a PR for vulnerable or licence-incompatible additions. |
| [`release.yml`](.github/workflows/release.yml) | cargo-dist release build plus the hand-written AppImage job. Triggered by `v*` tags. |
| [`.github/dependabot.yml`](.github/dependabot.yml) | Weekly `cargo` and `github-actions` update PRs, grouped into patch/minor batches. |

Because gpui is a GPUI/Zed derivative, the Linux jobs install the Wayland/X11/
Vulkan development packages it links against. If you add a workflow that
compiles the app on Linux, it needs that package list too.

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
| `Ctrl+C` | Copy the selected text |
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

## Security

### Dependency policy

[`deny.toml`](deny.toml) is enforced by CI on every push. A build fails if any
dependency:

- carries a known [RustSec](https://rustsec.org/) advisory,
- has a licence outside the permissive allowlist (MIT, Apache-2.0, BSD, ISC,
  Zlib, MPL-2.0, CDLA-Permissive-2.0, Unicode-3.0, …), or
- does not come from crates.io.

Dependabot opens an update PR when a new advisory appears, and `cargo-deny`
blocks the merge until the tree is clean again.

### Accepted upstream advisories

Three `zip` advisories are currently accepted, each with a written rationale in
[`deny.toml`](deny.toml):

| Advisory | Issue |
|---|---|
| `RUSTSEC-2026-0367` | SUID/SGID/sticky bits preserved when extracting an archive |
| `RUSTSEC-2026-0368` | Symlink targets are not confined to the destination directory |
| `RUSTSEC-2026-0369` | Unix permissions applied through those symlinks |

All three are on `ZipArchive::extract()` — the *extract an archive onto disk*
API. eParna never calls it: ebook containers are read entry-by-entry in memory
via `EpubArchive::read_bytes`, and the app never runs with elevated privileges.

The upstream fixes only exist in `zip >= 9.0.0`, which cannot be taken yet
because both copies are major-version-pinned by our dependencies
(`ebook-rs 0.17.1` → `zip 7.4.0`, `office_oxide 0.1.11` → `zip 8.6.0`). These
exceptions should be deleted as soon as an `ebook-rs` release depends on
`zip >= 9`.

### Reporting a vulnerability

Please report security issues through GitHub's private vulnerability reporting
for this repository rather than opening a public issue.

## Parity with Foliate

This project targets roughly 80% feature parity with the
[Foliate](https://github.com/johnfactotum/foliate) ebook reader. Implemented so far:

library view, recent files, filtering/sorting, TOC, bookmarks, notes, in-book
search, themes, typography and layout controls, two-page view, footnotes,
dictionary/Wikipedia lookup, OPDS, progress, fullscreen, multiple windows, state
persistence and annotation export.

Known gaps (mostly limited by the current UI stack):

- Multi-colour highlights (single colour only).
- Text-to-speech.
- True paginated (page-flip) layout and hyphenation; scrolling is used instead.
- Metadata editing (the library is a cover grid, but book metadata is read-only).
- Seekable progress bar and search jump-to-match: GPUI's HTML `TextView` has no
  programmatic scroll/reveal, so intra-section position restore and clickable
  progress are deferred until upstream support lands.
- DRM (Readium LCP). Passphrase entry is now supported (`from_bytes_with_passphrase`);
  full per-resource decryption and broader LCP test coverage are still pending.

## Project layout

```
eParna/
├── Cargo.toml
├── src/
│   └── main.rs      # application entry point, ReaderView and render logic
├── assets/
│   └── icons/       # app icons (used by the AppImage/MSI/pkg installers)
├── .github/
│   ├── dependabot.yml
│   └── workflows/   # ci, release, codeql, deny, dependency-review
├── wix/
│   └── main.wxs     # MSI definition, generated by cargo-dist
├── dist-workspace.toml
├── deny.toml        # cargo-deny policy: advisories, licences, sources
├── README.md
├── CHANGELOG.md
└── LICENSE
```

## Dependencies

| Crate     | Version | Purpose                          |
|-----------|---------|----------------------------------|
| ebook-rs  | 0.17.1  | Ebook parsing, search, TOC, covers, validation, EPUB export |
| gpui-kit  | 0.7.3   | UI framework & components        |
| rfd       | 0.15    | Native file dialogs              |
| serde     | 1       | State serialization              |
| serde_json| 1       | State (de)serialization          |
| ureq      | 3       | OPDS catalogs & online lookups   |
| base64    | 0.23    | Cover thumbnails                 |
| office_oxide | `=0.1.11` | Pin only — not called directly (see below) |

`ebook-rs` is used with the `serde`, `opds`, `pdf`, `parallel`, `mmap` and `zstd`
features enabled (PDF parsing, parallel ZIP export, memory-mapped I/O and Zstd
state caching).

The `office_oxide = "=0.1.11"` entry is a **pin, not a real dependency** — we do
not call it directly. `office_oxide` 0.1.12 added a required `defined_names`
field to `DocumentIR`, which breaks `pdf_oxide` 0.3.78's struct literal and
therefore the `pdf` feature. Dependabot is configured never to bump it
automatically. See the comment in `Cargo.toml`.

## License

Released under the [MIT License](LICENSE).

Third-party components remain under their respective licenses — see the
[ebook-rs](https://crates.io/crates/ebook-rs) and
[gpui-kit](https://crates.io/crates/gpui-kit) crates.