# eParna website

This folder holds the static [GitHub Pages](https://pages.github.com/) site for eParna,
inspired by the clean single-page design of <https://huacnlee.github.io/cargo/>.

The entire site is a single self-contained `index.html` (inline CSS + JS) plus the app
icon in `assets/`. There is no build step.

## Enable Pages

In the repository settings, under **Pages → Build and deployment**:

- **Source:** Deploy from a branch
- **Branch:** `main`
- **Folder:** `/docs` (root)

Save, and the site will be served at `https://SV-stark.github.io/eParna/`.

The `.nojekyll` file prevents Jekyll from processing the folder, so files beginning with
an underscore (and assets) are served as-is.

## Editing

- Content, styling and behaviour all live in `index.html`.
- Theme tokens (Midnight / Dusk / Paper) are CSS custom properties at the top of the
  `<style>` block; add a new `data-theme` there and an `<option>` in the theme selector.
- The install commands are in the "install" tab panels; keep them aligned with the
  one-liners in the root `README.md`.
