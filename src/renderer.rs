use crate::models::Annotation;
use crate::ui_helpers::{ThemePalette, highlight_swatch_color};
use base64::Engine as _;
use ebook_rs::{DomNode, EbookDomTree, SyntaxHighlightEngine, decode_html_entities};
use gpui_kit::base::{
    TextSelectionHandle, TextSelectionRegistration, TextSelectionRun, h_flex, v_flex,
};
use gpui_kit::*;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

pub type LinkClickCallback = Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>;
pub type ImageCache = Arc<Mutex<HashMap<String, Arc<Image>>>>;

/// Render configuration for a single reading column.
pub struct RenderConfig<'a> {
    pub id_prefix: &'a str,
    pub order_base: u64,
    pub theme: &'a str,
    pub palette: ThemePalette,
    pub base_font_size: f32,
    pub line_height: f32,
    pub font_family: &'a str,
    pub text_align: &'a str,
    pub rtl_mode: bool,
    pub column_px: f32,
    pub annotations: &'a [Annotation],
    pub active_search: Option<&'a str>,
    pub image_cache: &'a ImageCache,
    pub on_link_click: LinkClickCallback,
}

#[derive(Clone, Debug, Default)]
pub struct CssBoxStyle {
    pub display_none: bool,
    pub visibility_hidden: bool,
    pub color: Option<Hsla>,
    pub background_color: Option<Hsla>,
    pub font_size_px: Option<f32>,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
    pub font_family: Option<String>,
    pub text_align: Option<TextAlign>,
    pub underline: bool,
    pub line_through: bool,
    pub text_transform: Option<TextTransform>,
    pub text_indent_px: Option<f32>,
    pub line_height_px: Option<f32>,
    /// [top, right, bottom, left]
    pub margin: [Option<f32>; 4],
    /// [top, right, bottom, left]
    pub padding: [Option<f32>; 4],
    /// [top, right, bottom, left]
    pub border_widths: [f32; 4],
    /// [top, right, bottom, left]
    pub border_colors: [Option<Hsla>; 4],
    pub border_radius: Option<f32>,
    pub width_px: Option<f32>,
    pub max_width_px: Option<f32>,
    pub height_px: Option<f32>,
    pub rtl: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextTransform {
    Uppercase,
    Lowercase,
    Capitalize,
}

impl CssBoxStyle {
    /// Inheritable properties passed from parent container to children.
    fn inherited_only(&self) -> Self {
        Self {
            color: self.color,
            font_size_px: self.font_size_px,
            font_weight: self.font_weight,
            font_style: self.font_style,
            font_family: self.font_family.clone(),
            text_align: self.text_align,
            underline: self.underline,
            line_through: self.line_through,
            text_transform: self.text_transform,
            line_height_px: self.line_height_px,
            rtl: self.rtl,
            ..Default::default()
        }
    }
}

fn is_dark_theme(theme: &str) -> bool {
    matches!(theme, "Dark" | "OLED Black" | "Nord" | "Solarized Dark")
}

pub fn hsla_luminance(c: Hsla) -> f32 {
    let rgba: Rgba = c.into();
    0.2126 * rgba.r + 0.7152 * rgba.g + 0.0722 * rgba.b
}

pub fn is_palette_dark(theme: &str, palette: &ThemePalette) -> bool {
    is_dark_theme(theme) || (theme == "Custom" && hsla_luminance(palette.bg) < 0.45)
}

fn adapt_fg_for_theme(color: Hsla, theme: &str, palette: &ThemePalette) -> Hsla {
    if is_palette_dark(theme, palette) {
        if hsla_luminance(color) < 0.25 {
            return palette.fg;
        }
    } else if matches!(theme, "Sepia" | "Solarized" | "E-Ink" | "Custom")
        && hsla_luminance(color) < 0.15
    {
        return palette.fg;
    }
    color
}

fn adapt_bg_for_theme(color: Hsla, theme: &str, palette: &ThemePalette) -> Option<Hsla> {
    if color.a < 0.01 {
        return None;
    }
    if is_palette_dark(theme, palette) && hsla_luminance(color) > 0.80 {
        return None;
    }
    if matches!(theme, "Sepia" | "Solarized" | "E-Ink") && hsla_luminance(color) > 0.90 {
        return None;
    }
    Some(color)
}

fn adapt_border_for_theme(color: Hsla, theme: &str, palette: &ThemePalette) -> Hsla {
    if is_palette_dark(theme, palette) && hsla_luminance(color) < 0.22 {
        return palette.border;
    }
    color
}

pub fn parse_css_length(val: &str, font_size: f32, container_px: f32) -> Option<f32> {
    let v = val.trim().to_ascii_lowercase();
    if v == "0" || v == "0px" || v == "0em" || v == "0rem" || v == "0pt" {
        return Some(0.0);
    }
    if let Some(num) = v.strip_suffix("px") {
        return num.trim().parse::<f32>().ok();
    }
    if let Some(num) = v.strip_suffix("pt") {
        return num.trim().parse::<f32>().ok().map(|n| n * 1.333_333);
    }
    if let Some(num) = v.strip_suffix("rem") {
        return num.trim().parse::<f32>().ok().map(|n| n * font_size);
    }
    if let Some(num) = v.strip_suffix("em") {
        return num.trim().parse::<f32>().ok().map(|n| n * font_size);
    }
    if let Some(num) = v.strip_suffix('%') {
        return num
            .trim()
            .parse::<f32>()
            .ok()
            .map(|n| (n / 100.0) * container_px);
    }
    None
}

pub fn parse_css_color(val: &str) -> Option<Hsla> {
    let v = val.trim().to_ascii_lowercase();
    if v == "transparent" {
        return Some(transparent_black());
    }
    if let Some(hex) = v.strip_prefix('#') {
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                let packed = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                return Some(rgb(packed).into());
            }
            6 => {
                let packed = u32::from_str_radix(hex, 16).ok()?;
                return Some(rgb(packed).into());
            }
            8 => {
                let packed = u32::from_str_radix(hex, 16).ok()?;
                return Some(rgba(packed).into());
            }
            _ => return None,
        }
    }
    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some(rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)).into());
        }
    }
    if let Some(inner) = v.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 4 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            let a = (parts[3].trim().parse::<f32>().ok()?.clamp(0.0, 1.0) * 255.0).round() as u32;
            return Some(
                rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a).into(),
            );
        }
    }
    let named = match v.as_str() {
        "black" => Some(0x000000),
        "white" => Some(0xffffff),
        "red" => Some(0xff0000),
        "green" => Some(0x008000),
        "blue" => Some(0x0000ff),
        "yellow" => Some(0xffff00),
        "orange" => Some(0xffa500),
        "purple" => Some(0x800080),
        "pink" => Some(0xffc0cb),
        "brown" => Some(0xa52a2a),
        "gray" | "grey" => Some(0x808080),
        "silver" => Some(0xc0c0c0),
        "darkgray" | "darkgrey" => Some(0xa9a9a9),
        "lightgray" | "lightgrey" => Some(0xd3d3d3),
        "navy" => Some(0x000080),
        "teal" => Some(0x008080),
        "maroon" => Some(0x800000),
        "olive" => Some(0x808000),
        "cyan" | "aqua" => Some(0x00ffff),
        "magenta" | "fuchsia" => Some(0xff00ff),
        _ => None,
    };
    named.map(|c| rgb(c).into())
}

fn parse_four_lengths(val: &str, font_size: f32, container_px: f32) -> [Option<f32>; 4] {
    let parts: Vec<&str> = val.split_whitespace().collect();
    let p = |s: &str| parse_css_length(s, font_size, container_px);
    match parts.len() {
        1 => {
            let v = p(parts[0]);
            [v, v, v, v]
        }
        2 => {
            let tb = p(parts[0]);
            let lr = p(parts[1]);
            [tb, lr, tb, lr]
        }
        3 => {
            let t = p(parts[0]);
            let lr = p(parts[1]);
            let b = p(parts[2]);
            [t, lr, b, lr]
        }
        4 => [p(parts[0]), p(parts[1]), p(parts[2]), p(parts[3])],
        _ => [None; 4],
    }
}

fn parse_border_shorthand(val: &str, font_size: f32) -> (f32, Option<Hsla>) {
    let lower = val.trim().to_ascii_lowercase();
    if lower == "none" || lower == "0" {
        return (0.0, None);
    }
    let mut width = 1.0_f32;
    let mut color = None;
    for tok in val.split_whitespace() {
        let tl = tok.to_ascii_lowercase();
        if tl == "thin" {
            width = 1.0;
        } else if tl == "medium" {
            width = 2.0;
        } else if tl == "thick" {
            width = 3.0;
        } else if matches!(
            tl.as_str(),
            "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset"
        ) {
            continue;
        } else if let Some(w) = parse_css_length(tok, font_size, font_size) {
            width = w.max(0.0);
        } else if let Some(c) = parse_css_color(tok) {
            color = Some(c);
        }
    }
    (width, color)
}

fn apply_inline_css(
    style: &mut CssBoxStyle,
    css: &str,
    base_font_size: f32,
    column_px: f32,
    theme: &str,
    palette: &ThemePalette,
) {
    let current_fs = style.font_size_px.unwrap_or(base_font_size);
    for decl in css.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let Some((raw_k, raw_v)) = decl.split_once(':') else {
            continue;
        };
        let prop = raw_k.trim().to_ascii_lowercase();
        let val = raw_v.trim().trim_end_matches("!important").trim();
        if val.is_empty() {
            continue;
        }
        match prop.as_str() {
            "display" if val.eq_ignore_ascii_case("none") => style.display_none = true,
            "visibility" if val.eq_ignore_ascii_case("hidden") => style.visibility_hidden = true,
            "color" => {
                if let Some(c) = parse_css_color(val) {
                    style.color = Some(adapt_fg_for_theme(c, theme, palette));
                }
            }
            "background-color" | "background" => {
                for tok in val.split_whitespace() {
                    if let Some(c) = parse_css_color(tok) {
                        style.background_color = adapt_bg_for_theme(c, theme, palette);
                        break;
                    }
                }
            }
            "font-size" => {
                let vl = val.to_ascii_lowercase();
                let fs = match vl.as_str() {
                    "xx-small" => Some(base_font_size * 0.6),
                    "x-small" => Some(base_font_size * 0.75),
                    "small" | "smaller" => Some(current_fs * 0.85),
                    "medium" => Some(base_font_size),
                    "large" | "larger" => Some(current_fs * 1.2),
                    "x-large" => Some(base_font_size * 1.5),
                    "xx-large" => Some(base_font_size * 2.0),
                    _ => parse_css_length(val, current_fs, current_fs),
                };
                if let Some(px_val) = fs {
                    style.font_size_px = Some(px_val.clamp(8.0, 96.0));
                }
            }
            "font-weight" => {
                let vl = val.to_ascii_lowercase();
                style.font_weight = match vl.as_str() {
                    "bold" | "bolder" | "700" | "800" | "900" => Some(FontWeight::BOLD),
                    "600" => Some(FontWeight::SEMIBOLD),
                    "500" => Some(FontWeight::MEDIUM),
                    "normal" | "400" => Some(FontWeight::NORMAL),
                    "light" | "lighter" | "300" | "200" | "100" => Some(FontWeight::LIGHT),
                    _ => None,
                };
            }
            "font-style" => {
                let vl = val.to_ascii_lowercase();
                style.font_style = match vl.as_str() {
                    "italic" | "oblique" => Some(FontStyle::Italic),
                    "normal" => Some(FontStyle::Normal),
                    _ => None,
                };
            }
            "font-family" => {
                // Respect user's reader typeface choice; only keep explicit monospace fonts.
                if val.to_ascii_lowercase().contains("mono")
                    || val.to_ascii_lowercase().contains("consolas")
                    || val.to_ascii_lowercase().contains("courier")
                {
                    style.font_family = Some(resolve_font_family("monospace"));
                }
            }
            "text-align" => {
                style.text_align = match val.to_ascii_lowercase().as_str() {
                    "center" => Some(TextAlign::Center),
                    "right" | "end" => Some(TextAlign::Right),
                    "left" | "start" | "justify" => Some(TextAlign::Left),
                    _ => None,
                };
            }
            "text-decoration" | "text-decoration-line" => {
                let vl = val.to_ascii_lowercase();
                if vl.contains("underline") {
                    style.underline = true;
                }
                if vl.contains("line-through") {
                    style.line_through = true;
                }
                if vl == "none" {
                    style.underline = false;
                    style.line_through = false;
                }
            }
            "text-transform" => {
                style.text_transform = match val.to_ascii_lowercase().as_str() {
                    "uppercase" => Some(TextTransform::Uppercase),
                    "lowercase" => Some(TextTransform::Lowercase),
                    "capitalize" => Some(TextTransform::Capitalize),
                    "none" => None,
                    _ => style.text_transform,
                };
            }
            "text-indent" => {
                if let Some(ind) = parse_css_length(val, current_fs, column_px) {
                    style.text_indent_px = Some(ind.max(0.0));
                }
            }
            "line-height" => {
                if let Ok(mult) = val.parse::<f32>() {
                    style.line_height_px = Some((mult * current_fs).clamp(10.0, 120.0));
                } else if let Some(lh) = parse_css_length(val, current_fs, current_fs) {
                    style.line_height_px = Some(lh.clamp(10.0, 120.0));
                }
            }
            "margin" => {
                let m = parse_four_lengths(val, current_fs, column_px);
                for i in 0..4 {
                    if m[i].is_some() {
                        style.margin[i] = m[i];
                    }
                }
            }
            "margin-top" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.margin[0] = Some(v);
                }
            }
            "margin-right" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.margin[1] = Some(v);
                }
            }
            "margin-bottom" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.margin[2] = Some(v);
                }
            }
            "margin-left" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.margin[3] = Some(v);
                }
            }
            "padding" => {
                let p = parse_four_lengths(val, current_fs, column_px);
                for i in 0..4 {
                    if p[i].is_some() {
                        style.padding[i] = p[i];
                    }
                }
            }
            "padding-top" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.padding[0] = Some(v);
                }
            }
            "padding-right" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.padding[1] = Some(v);
                }
            }
            "padding-bottom" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.padding[2] = Some(v);
                }
            }
            "padding-left" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.padding[3] = Some(v);
                }
            }
            "border" => {
                let (w, c) = parse_border_shorthand(val, current_fs);
                let c = c.map(|col| adapt_border_for_theme(col, theme, palette));
                style.border_widths = [w, w, w, w];
                style.border_colors = [c, c, c, c];
            }
            "border-top" => {
                let (w, c) = parse_border_shorthand(val, current_fs);
                style.border_widths[0] = w;
                style.border_colors[0] = c.map(|col| adapt_border_for_theme(col, theme, palette));
            }
            "border-right" => {
                let (w, c) = parse_border_shorthand(val, current_fs);
                style.border_widths[1] = w;
                style.border_colors[1] = c.map(|col| adapt_border_for_theme(col, theme, palette));
            }
            "border-bottom" => {
                let (w, c) = parse_border_shorthand(val, current_fs);
                style.border_widths[2] = w;
                style.border_colors[2] = c.map(|col| adapt_border_for_theme(col, theme, palette));
            }
            "border-left" => {
                let (w, c) = parse_border_shorthand(val, current_fs);
                style.border_widths[3] = w;
                style.border_colors[3] = c.map(|col| adapt_border_for_theme(col, theme, palette));
            }
            "border-color" => {
                if let Some(c) = parse_css_color(val) {
                    let c = adapt_border_for_theme(c, theme, palette);
                    style.border_colors = [Some(c), Some(c), Some(c), Some(c)];
                }
            }
            "border-radius" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.border_radius = Some(v.max(0.0));
                }
            }
            "width" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.width_px = Some(v.max(0.0));
                }
            }
            "max-width" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.max_width_px = Some(v.max(0.0));
                }
            }
            "height" => {
                if let Some(v) = parse_css_length(val, current_fs, column_px) {
                    style.height_px = Some(v.max(0.0));
                }
            }
            "direction" => {
                if val.eq_ignore_ascii_case("rtl") {
                    style.rtl = Some(true);
                } else if val.eq_ignore_ascii_case("ltr") {
                    style.rtl = Some(false);
                }
            }
            _ => {}
        }
    }
}

fn compute_node_style<K, V, S>(
    tag: &str,
    attrs: &HashMap<K, V, S>,
    parent: &CssBoxStyle,
    cfg: &RenderConfig<'_>,
) -> CssBoxStyle
where
    K: std::borrow::Borrow<str> + Eq + std::hash::Hash,
    V: AsRef<str>,
    S: std::hash::BuildHasher,
{
    let mut style = parent.inherited_only();
    let base_fs = cfg.base_font_size;

    // Tag defaults
    match tag {
        "h1" => {
            style.font_size_px = Some(base_fs * 1.9);
            style.font_weight = Some(FontWeight::BOLD);
            style.margin[0] = Some(base_fs * 0.8);
            style.margin[2] = Some(base_fs * 0.45);
        }
        "h2" => {
            style.font_size_px = Some(base_fs * 1.55);
            style.font_weight = Some(FontWeight::BOLD);
            style.margin[0] = Some(base_fs * 0.7);
            style.margin[2] = Some(base_fs * 0.4);
        }
        "h3" => {
            style.font_size_px = Some(base_fs * 1.3);
            style.font_weight = Some(FontWeight::BOLD);
            style.margin[0] = Some(base_fs * 0.6);
            style.margin[2] = Some(base_fs * 0.35);
        }
        "h4" => {
            style.font_size_px = Some(base_fs * 1.15);
            style.font_weight = Some(FontWeight::SEMIBOLD);
            style.margin[0] = Some(base_fs * 0.5);
            style.margin[2] = Some(base_fs * 0.3);
        }
        "h5" => {
            style.font_size_px = Some(base_fs * 1.0);
            style.font_weight = Some(FontWeight::SEMIBOLD);
            style.margin[0] = Some(base_fs * 0.4);
            style.margin[2] = Some(base_fs * 0.25);
        }
        "h6" => {
            style.font_size_px = Some(base_fs * 0.9);
            style.font_weight = Some(FontWeight::SEMIBOLD);
            style.margin[0] = Some(base_fs * 0.35);
            style.margin[2] = Some(base_fs * 0.2);
        }
        "p" => {
            style.margin[2] = Some(base_fs * 0.65);
        }
        "blockquote" => {
            style.margin = [
                Some(base_fs * 0.6),
                Some(0.0),
                Some(base_fs * 0.6),
                Some(0.0),
            ];
            style.padding = [Some(4.0), Some(0.0), Some(4.0), Some(14.0)];
            style.border_widths = [0.0, 0.0, 0.0, 3.0];
            style.border_colors = [None, None, None, Some(cfg.palette.border)];
        }
        "pre" => {
            style.font_family = Some("Consolas".to_string());
            style.font_size_px = Some((base_fs * 0.88).max(11.0));
            style.background_color = Some(cfg.palette.surface);
            style.padding = [Some(10.0), Some(12.0), Some(10.0), Some(12.0)];
            style.margin[2] = Some(base_fs * 0.65);
            style.border_radius = Some(6.0);
        }
        "b" | "strong" => {
            style.font_weight = Some(FontWeight::BOLD);
        }
        "i" | "em" | "cite" | "dfn" | "var" => {
            style.font_style = Some(FontStyle::Italic);
        }
        "u" | "ins" => {
            style.underline = true;
        }
        "s" | "del" | "strike" => {
            style.line_through = true;
        }
        "code" | "kbd" | "samp" | "tt" => {
            style.font_family = Some("Consolas".to_string());
            style.background_color = Some(cfg.palette.surface);
        }
        "small" | "sub" | "sup" => {
            let cur = style.font_size_px.unwrap_or(base_fs);
            style.font_size_px = Some((cur * 0.82).max(10.0));
        }
        "big" => {
            let cur = style.font_size_px.unwrap_or(base_fs);
            style.font_size_px = Some(cur * 1.2);
        }
        "mark" => {
            style.background_color = Some(rgb(0xffd54f).into());
            style.color = Some(rgb(0x111111).into());
        }
        "figure" => {
            style.margin[0] = Some(base_fs * 0.8);
            style.margin[2] = Some(base_fs * 0.8);
            style.text_align = Some(TextAlign::Center);
        }
        "figcaption" => {
            style.font_size_px = Some((base_fs * 0.88).max(11.0));
            style.font_style = Some(FontStyle::Italic);
            style.color = Some(cfg.palette.muted);
            style.margin[0] = Some(4.0);
            style.text_align = Some(TextAlign::Center);
        }
        "dl" => {
            style.margin[0] = Some(base_fs * 0.5);
            style.margin[2] = Some(base_fs * 0.5);
        }
        "dt" => {
            style.font_weight = Some(FontWeight::BOLD);
            style.margin[0] = Some(base_fs * 0.4);
        }
        "dd" => {
            style.margin[3] = Some(20.0);
            style.margin[2] = Some(base_fs * 0.35);
        }
        "details" => {
            style.background_color = Some(cfg.palette.surface);
            style.border_widths = [1.0, 1.0, 1.0, 1.0];
            style.border_colors = [
                Some(cfg.palette.border),
                Some(cfg.palette.border),
                Some(cfg.palette.border),
                Some(cfg.palette.border),
            ];
            style.border_radius = Some(6.0);
            style.padding = [Some(8.0), Some(12.0), Some(8.0), Some(12.0)];
            style.margin[2] = Some(base_fs * 0.65);
        }
        "summary" => {
            style.font_weight = Some(FontWeight::SEMIBOLD);
            style.margin[2] = Some(4.0);
        }
        "a" if attrs.contains_key("href") => {
            style.color = Some(if is_palette_dark(cfg.theme, &cfg.palette) {
                rgb(0x8ab4f8).into()
            } else {
                rgb(0x2563eb).into()
            });
            style.underline = true;
        }
        _ => {}
    }

    if let Some(dir) = attrs.get("dir").map(AsRef::as_ref) {
        if dir.eq_ignore_ascii_case("rtl") {
            style.rtl = Some(true);
        } else if dir.eq_ignore_ascii_case("ltr") {
            style.rtl = Some(false);
        }
    }
    if let Some(align) = attrs.get("align").map(AsRef::as_ref) {
        style.text_align = match align.to_ascii_lowercase().as_str() {
            "center" => Some(TextAlign::Center),
            "right" => Some(TextAlign::Right),
            "left" | "justify" => Some(TextAlign::Left),
            _ => style.text_align,
        };
    }

    if let Some(inline_css) = attrs.get("style").map(AsRef::as_ref) {
        apply_inline_css(
            &mut style,
            inline_css,
            base_fs,
            cfg.column_px,
            cfg.theme,
            &cfg.palette,
        );
    }

    style
}

/// Apply CSS box properties (margins, paddings, borders, background, radius) to a GPUI `div`.
fn apply_box_style(mut d: Div, s: &CssBoxStyle, default_border: Hsla) -> Div {
    if let Some(v) = s.margin[0] {
        d = d.mt(px(v.max(0.0)));
    }
    if let Some(v) = s.margin[1] {
        d = d.mr(px(v.max(0.0)));
    }
    if let Some(v) = s.margin[2] {
        d = d.mb(px(v.max(0.0)));
    }
    if let Some(v) = s.margin[3] {
        d = d.ml(px(v.max(0.0)));
    }
    if let Some(v) = s.padding[0] {
        d = d.pt(px(v.max(0.0)));
    }
    if let Some(v) = s.padding[1] {
        d = d.pr(px(v.max(0.0)));
    }
    if let Some(v) = s.padding[2] {
        d = d.pb(px(v.max(0.0)));
    }
    if let Some(v) = s.padding[3] {
        d = d.pl(px(v.max(0.0)));
    }
    if s.border_widths[0] > 0.0 {
        d = d.border_t(px(s.border_widths[0]));
    }
    if s.border_widths[1] > 0.0 {
        d = d.border_r(px(s.border_widths[1]));
    }
    if s.border_widths[2] > 0.0 {
        d = d.border_b(px(s.border_widths[2]));
    }
    if s.border_widths[3] > 0.0 {
        d = d.border_l(px(s.border_widths[3]));
    }
    let border_col = s.border_colors.iter().flatten().copied().next();
    if s.border_widths.iter().any(|&w| w > 0.0) {
        d = d.border_color(border_col.unwrap_or(default_border));
    }
    if let Some(bg) = s.background_color {
        d = d.bg(bg);
    }
    if let Some(r) = s.border_radius {
        d = d.rounded(px(r));
    }
    d
}

/// Merge potentially overlapping `(Range<usize>, HighlightStyle)` spans into sorted,
/// non-overlapping spans suitable for `StyledText::with_highlights`.
pub fn merge_highlights(
    raw: Vec<(Range<usize>, HighlightStyle)>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let valid: Vec<(Range<usize>, HighlightStyle)> =
        raw.into_iter().filter(|(r, _)| r.start < r.end).collect();
    if valid.is_empty() {
        return Vec::new();
    }
    let mut boundaries: Vec<usize> = Vec::with_capacity(valid.len() * 2);
    for (r, _) in &valid {
        boundaries.push(r.start);
        boundaries.push(r.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut out: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if start >= end {
            continue;
        }
        let mut combined = HighlightStyle::default();
        let mut any = false;
        for (r, st) in &valid {
            if r.start <= start && r.end >= end {
                any = true;
                if st.color.is_some() {
                    combined.color = st.color;
                }
                if st.background_color.is_some() {
                    combined.background_color = st.background_color;
                }
                if st.font_weight.is_some() {
                    combined.font_weight = st.font_weight;
                }
                if st.font_style.is_some() {
                    combined.font_style = st.font_style;
                }
                if st.underline.is_some() {
                    combined.underline = st.underline;
                }
                if st.strikethrough.is_some() {
                    combined.strikethrough = st.strikethrough;
                }
            }
        }
        if any {
            if let Some(prev) = out.last_mut()
                && prev.0.end == start
                && prev.1 == combined
            {
                prev.0.end = end;
            } else {
                out.push((start..end, combined));
            }
        }
    }
    out
}

fn transform_text(s: &str, tr: Option<TextTransform>) -> String {
    match tr {
        None => s.to_string(),
        Some(TextTransform::Uppercase) => s.to_uppercase(),
        Some(TextTransform::Lowercase) => s.to_lowercase(),
        Some(TextTransform::Capitalize) => {
            let mut out = String::with_capacity(s.len());
            let mut cap_next = true;
            for c in s.chars() {
                if c.is_whitespace() {
                    cap_next = true;
                    out.push(c);
                } else if cap_next {
                    for uc in c.to_uppercase() {
                        out.push(uc);
                    }
                    cap_next = false;
                } else {
                    out.push(c);
                }
            }
            out
        }
    }
}

fn collapse_html_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_ws {
                out.push(' ');
                last_ws = true;
            }
        } else {
            out.push(c);
            last_ws = false;
        }
    }
    out
}

#[derive(Default)]
struct InlineFlow {
    text: String,
    spans: Vec<(Range<usize>, HighlightStyle)>,
    links: Vec<(Range<usize>, String)>,
}

impl InlineFlow {
    fn push_text(&mut self, raw: &str, style: &CssBoxStyle, preserve_ws: bool, link: Option<&str>) {
        let decoded = decode_html_entities(raw);
        let normalized = if preserve_ws {
            decoded
        } else {
            let collapsed = collapse_html_whitespace(&decoded);
            if self.text.is_empty() || self.text.ends_with(' ') || self.text.ends_with('\n') {
                collapsed.trim_start().to_string()
            } else {
                collapsed
            }
        };
        let transformed = transform_text(&normalized, style.text_transform);
        if transformed.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(&transformed);
        let end = self.text.len();

        let mut hs = HighlightStyle::default();
        let mut has_hs = false;
        if let Some(c) = style.color {
            hs.color = Some(c);
            has_hs = true;
        }
        if let Some(bg) = style.background_color {
            hs.background_color = Some(bg);
            has_hs = true;
        }
        if let Some(w) = style.font_weight {
            hs.font_weight = Some(w);
            has_hs = true;
        }
        if let Some(fs) = style.font_style {
            hs.font_style = Some(fs);
            has_hs = true;
        }
        if style.underline {
            hs.underline = Some(UnderlineStyle {
                thickness: px(1.0),
                color: style.color,
                wavy: false,
            });
            has_hs = true;
        }
        if style.line_through {
            hs.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.0),
                color: style.color,
            });
            has_hs = true;
        }
        if has_hs {
            self.spans.push((start..end, hs));
        }
        if let Some(href) = link {
            self.links.push((start..end, href.to_string()));
        }
    }
}

fn extract_lang_from_attrs<K, V, S>(attrs: &HashMap<K, V, S>) -> Option<String>
where
    K: std::borrow::Borrow<str> + Eq + std::hash::Hash,
    V: AsRef<str>,
    S: std::hash::BuildHasher,
{
    let class = attrs.get("class")?.as_ref();
    for token in class.split_whitespace() {
        if let Some(lang) = token
            .strip_prefix("language-")
            .or_else(|| token.strip_prefix("lang-"))
            && !lang.is_empty()
        {
            return Some(lang.to_string());
        }
    }
    None
}

fn collect_code_text(nodes: &[DomNode], out: &mut String, lang: &mut Option<String>) -> bool {
    for node in nodes {
        match node {
            DomNode::Comment(_) => {}
            DomNode::Text(t) => out.push_str(&decode_html_entities(t)),
            DomNode::Element {
                tag_name,
                attributes,
                children,
            } => {
                let tag = tag_name.to_ascii_lowercase();
                if tag == "br" {
                    out.push('\n');
                    continue;
                }
                if !matches!(tag.as_str(), "code" | "span" | "kbd" | "samp" | "tt") {
                    return false;
                }
                if lang.is_none() {
                    *lang = extract_lang_from_attrs(attributes);
                }
                if !collect_code_text(children, out, lang) {
                    return false;
                }
            }
        }
    }
    true
}

fn extract_plain_code_block<K, V, S>(
    nodes: &[DomNode],
    pre_attrs: &HashMap<K, V, S>,
) -> Option<(String, String)>
where
    K: std::borrow::Borrow<str> + Eq + std::hash::Hash,
    V: AsRef<str>,
    S: std::hash::BuildHasher,
{
    let mut code = String::new();
    let mut lang = extract_lang_from_attrs(pre_attrs);
    if !collect_code_text(nodes, &mut code, &mut lang) {
        return None;
    }
    Some((code, lang.unwrap_or_else(|| "text".to_string())))
}

fn syntax_node_color(
    kind: &str,
    dark: bool,
) -> Option<(Hsla, Option<FontWeight>, Option<FontStyle>)> {
    match kind {
        "comment" => Some((
            if dark {
                rgb(0x7f8c8d).into()
            } else {
                rgb(0x6a737d).into()
            },
            None,
            Some(FontStyle::Italic),
        )),
        "function_definition" => Some((
            if dark {
                rgb(0x79b8ff).into()
            } else {
                rgb(0x005cc5).into()
            },
            Some(FontWeight::SEMIBOLD),
            None,
        )),
        "type_definition" => Some((
            if dark {
                rgb(0xffab70).into()
            } else {
                rgb(0xb35900).into()
            },
            Some(FontWeight::SEMIBOLD),
            None,
        )),
        _ => None,
    }
}

fn find_ci_ranges(haystack: &str, needle: &str) -> Vec<Range<usize>> {
    let trimmed = needle.trim();
    if trimmed.is_empty() || haystack.is_empty() {
        return Vec::new();
    }
    let h_lower = haystack.to_lowercase();
    let n_lower = trimmed.to_lowercase();
    if h_lower.len() == haystack.len() {
        let mut out = Vec::new();
        let mut pos = 0;
        while let Some(idx) = h_lower[pos..].find(&n_lower) {
            let start = pos + idx;
            let end = start + n_lower.len();
            if haystack.is_char_boundary(start) && haystack.is_char_boundary(end) {
                out.push(start..end);
            }
            pos = end.max(pos + 1);
        }
        return out;
    }

    // Unicode-safe mapping when case folding changes UTF-8 byte lengths (e.g. 'İ', 'ß')
    let mut lower_buf = String::with_capacity(haystack.len());
    let mut byte_map: Vec<(usize, usize)> = Vec::new();
    for (orig_start, ch) in haystack.char_indices() {
        let orig_end = orig_start + ch.len_utf8();
        for lc in ch.to_lowercase() {
            let lc_len = lc.len_utf8();
            lower_buf.push(lc);
            for _ in 0..lc_len {
                byte_map.push((orig_start, orig_end));
            }
        }
    }
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(idx) = lower_buf[pos..].find(&n_lower) {
        let l_start = pos + idx;
        let l_end = l_start + n_lower.len();
        if l_start < byte_map.len() && l_end > 0 && l_end <= byte_map.len() {
            let orig_start = byte_map[l_start].0;
            let orig_end = byte_map[l_end - 1].1;
            if orig_start < orig_end
                && haystack.is_char_boundary(orig_start)
                && haystack.is_char_boundary(orig_end)
            {
                out.push(orig_start..orig_end);
            }
        }
        pos = l_end.max(pos + 1);
    }
    out
}

/// Decode a `data:image/...;base64,...` URI into a cached `Arc<gpui::Image>`.
pub fn decode_data_uri_image(src: &str, cache: &ImageCache) -> Option<Arc<Image>> {
    if let Ok(guard) = cache.lock()
        && let Some(cached) = guard.get(src)
    {
        return Some(cached.clone());
    }
    let rest = src.strip_prefix("data:")?;
    let (header, data) = rest.split_once(',')?;
    if !header.to_ascii_lowercase().contains(";base64") {
        return None;
    }
    let mime = header.split(';').next().unwrap_or("").trim();
    let format = ImageFormat::from_mime_type(mime)?;
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned)
        .ok()?;
    let image = Arc::new(Image::from_bytes(format, bytes));
    if let Ok(mut guard) = cache.lock() {
        if guard.len() > 256 {
            guard.clear();
        }
        guard.insert(src.to_string(), image.clone());
    }
    Some(image)
}

/// Selectable, link-aware GPUI text block that participates in window-wide `TextSelection`.
struct EbookTextBlock {
    id: ElementId,
    text: SharedString,
    styled_text: StyledText,
    document_order: u64,
    text_style: TextStyleRefinement,
    links: Arc<Vec<(Range<usize>, String)>>,
    on_link_click: LinkClickCallback,
}

struct RetainedBlockState {
    handle: TextSelectionHandle,
    _refresh: Subscription,
    mouse_down_pos: std::rc::Rc<std::cell::Cell<Option<Point<Pixels>>>>,
}

impl Element for EbookTextBlock {
    type RequestLayoutState = (
        TextSelectionHandle,
        std::rc::Rc<std::cell::Cell<Option<Point<Pixels>>>>,
    );
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let (handle, mouse_down_pos) = window.with_element_state(
            global_id.expect("EbookTextBlock must have a stable element id"),
            |retained: Option<RetainedBlockState>, window| {
                let retained = retained.unwrap_or_else(|| {
                    let handle = TextSelectionHandle::new(self.text.clone(), cx);
                    let refresh = handle.refresh_window_on_change(window, cx);
                    RetainedBlockState {
                        handle,
                        _refresh: refresh,
                        mouse_down_pos: std::rc::Rc::new(std::cell::Cell::new(None)),
                    }
                });
                (
                    (retained.handle.clone(), retained.mouse_down_pos.clone()),
                    retained,
                )
            },
        );
        let style = self.text_style.clone();
        let (layout_id, ()) = window.with_text_style(Some(style), |window| {
            self.styled_text
                .request_layout(global_id, inspector_id, window, cx)
        });
        (layout_id, (handle, mouse_down_pos))
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (handle, _): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled_text
            .prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let registration = TextSelectionRegistration::new(hitbox.clone(), bounds)
            .with_document_order(self.document_order)
            .with_text_bounds(vec![bounds])
            .with_rendered_element(handle, window, cx);
        handle.register(registration, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (handle, mouse_down_pos): &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.styled_text.layout().clone();

        let cursor = if !self.links.is_empty()
            && hitbox.is_hovered(window)
            && let Ok(idx) = layout.index_for_position(window.mouse_position())
            && self.links.iter().any(|(r, _)| r.contains(&idx))
        {
            CursorStyle::PointingHand
        } else {
            CursorStyle::IBeam
        };
        window.set_cursor_style(cursor, hitbox);

        let projection = handle.update_runs(
            &[
                TextSelectionRun::new(self.text.clone(), layout.clone(), bounds)
                    .with_document_order(self.document_order),
            ],
            cx,
        );
        let layout_bounds = layout.bounds();
        let sel_color = gpui_kit::base::Theme::global(cx).tokens.colors.selection;
        for range in projection.ranges().iter().flatten().cloned() {
            let (Some(start), Some(end)) = (
                layout.position_for_index(range.start),
                layout.position_for_index(range.end),
            ) else {
                continue;
            };
            let lh = layout.line_height();
            if start.y == end.y {
                window.paint_quad(PaintQuad {
                    bounds: Bounds::from_corners(start, Point::new(end.x, end.y + lh)),
                    background: sel_color.into(),
                    corner_radii: Corners::default(),
                    border_widths: Edges::default(),
                    border_color: transparent_black(),
                    border_style: BorderStyle::default(),
                });
            } else {
                let b1 =
                    Bounds::from_corners(start, Point::new(layout_bounds.right(), start.y + lh));
                window.paint_quad(PaintQuad {
                    bounds: b1,
                    background: sel_color.into(),
                    corner_radii: Corners::default(),
                    border_widths: Edges::default(),
                    border_color: transparent_black(),
                    border_style: BorderStyle::default(),
                });
                if end.y > start.y + lh {
                    let b2 = Bounds::from_corners(
                        Point::new(layout_bounds.left(), start.y + lh),
                        Point::new(layout_bounds.right(), end.y),
                    );
                    window.paint_quad(PaintQuad {
                        bounds: b2,
                        background: sel_color.into(),
                        corner_radii: Corners::default(),
                        border_widths: Edges::default(),
                        border_color: transparent_black(),
                        border_style: BorderStyle::default(),
                    });
                }
                let b3 = Bounds::from_corners(
                    Point::new(layout_bounds.left(), end.y),
                    Point::new(end.x, end.y + lh),
                );
                window.paint_quad(PaintQuad {
                    bounds: b3,
                    background: sel_color.into(),
                    corner_radii: Corners::default(),
                    border_widths: Edges::default(),
                    border_color: transparent_black(),
                    border_style: BorderStyle::default(),
                });
            }
        }

        self.styled_text.paint(
            global_id,
            inspector_id,
            bounds,
            &mut (),
            &mut (),
            window,
            cx,
        );

        if !self.links.is_empty() {
            let links = self.links.clone();
            let on_click = self.on_link_click.clone();
            let layout_for_click = layout;
            let hb = hitbox.clone();

            window.on_mouse_event({
                let mouse_down_pos = mouse_down_pos.clone();
                let hb = hb.clone();
                move |event: &MouseDownEvent, phase, window, _cx| {
                    if phase == DispatchPhase::Bubble
                        && event.button == MouseButton::Left
                        && hb.is_hovered(window)
                    {
                        mouse_down_pos.set(Some(event.position));
                    }
                }
            });

            window.on_mouse_event({
                let mouse_down_pos = mouse_down_pos.clone();
                move |event: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble
                        && event.button == MouseButton::Left
                        && let Some(down_pos) = mouse_down_pos.take()
                    {
                        let dx: f32 = (event.position.x - down_pos.x).into();
                        let dy: f32 = (event.position.y - down_pos.y).into();
                        if dx * dx + dy * dy <= 25.0
                            && hb.is_hovered(window)
                            && let Ok(idx) = layout_for_click.index_for_position(event.position)
                            && let Some((_, href)) = links.iter().find(|(r, _)| r.contains(&idx))
                        {
                            on_click(href, window, cx);
                        }
                    }
                }
            });
        }
    }
}

struct DomWalker<'a> {
    cfg: &'a RenderConfig<'a>,
    block_seq: u64,
}

impl<'a> DomWalker<'a> {
    fn next_id(&mut self, suffix: &str) -> ElementId {
        self.block_seq += 1;
        ElementId::Name(format!("{}-{}-{}", self.cfg.id_prefix, suffix, self.block_seq).into())
    }

    fn flush_flow(
        &mut self,
        flow: &mut InlineFlow,
        style: &CssBoxStyle,
        out: &mut Vec<AnyElement>,
        preserve_ws: bool,
    ) {
        let text = if preserve_ws {
            flow.text.clone()
        } else {
            flow.text.trim_end().to_string()
        };
        if text.is_empty() {
            flow.text.clear();
            flow.spans.clear();
            flow.links.clear();
            return;
        }

        // Apply persistent annotations & active search highlights
        for ann in self.cfg.annotations {
            if ann.is_highlight() && !ann.quote.trim().is_empty() {
                let bg = highlight_swatch_color(&ann.color);
                for r in find_ci_ranges(&text, &ann.quote) {
                    flow.spans.push((
                        r,
                        HighlightStyle {
                            background_color: Some(bg),
                            color: Some(rgb(0x111111).into()),
                            ..Default::default()
                        },
                    ));
                }
            }
        }
        if let Some(q) = self.cfg.active_search {
            for r in find_ci_ranges(&text, q) {
                flow.spans.push((
                    r,
                    HighlightStyle {
                        background_color: Some(rgb(0xff9800).into()),
                        color: Some(rgb(0x000000).into()),
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                ));
            }
        }

        // Apply text-indent visually as leading em-spaces if specified
        let (final_text, offset_shift) = if let Some(ind) = style.text_indent_px
            && ind >= 4.0
        {
            let spaces = ((ind / self.cfg.base_font_size.max(8.0)).round() as usize).clamp(1, 8);
            let prefix = "\u{2003}".repeat(spaces);
            let shift = prefix.len();
            (format!("{prefix}{text}"), shift)
        } else {
            (text, 0)
        };

        let mut shifted_spans = std::mem::take(&mut flow.spans);
        let mut shifted_links = std::mem::take(&mut flow.links);
        if offset_shift > 0 {
            for (r, _) in &mut shifted_spans {
                r.start += offset_shift;
                r.end += offset_shift;
            }
            for (r, _) in &mut shifted_links {
                r.start += offset_shift;
                r.end += offset_shift;
            }
        }
        let merged = merge_highlights(shifted_spans);

        let fs = style.font_size_px.unwrap_or(self.cfg.base_font_size);
        let lh = fs * self.cfg.line_height;
        let align = match self.cfg.text_align.to_ascii_lowercase().as_str() {
            "left" => TextAlign::Left,
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            _ => {
                if let Some(a) = style.text_align {
                    a
                } else if style.rtl.unwrap_or(self.cfg.rtl_mode) {
                    TextAlign::Right
                } else {
                    TextAlign::Left
                }
            }
        };
        let resolved_family =
            resolve_font_family(style.font_family.as_deref().unwrap_or(self.cfg.font_family));
        let mut ts = TextStyleRefinement {
            font_size: Some(px(fs).into()),
            line_height: Some(px(lh).into()),
            font_family: Some(resolved_family.into()),
            color: Some(style.color.unwrap_or(self.cfg.palette.fg)),
            font_weight: style.font_weight,
            font_style: style.font_style,
            text_align: Some(align),
            ..Default::default()
        };
        if style.underline {
            ts.underline = Some(UnderlineStyle {
                thickness: px(1.0),
                color: style.color,
                wavy: false,
            });
        }
        if style.line_through {
            ts.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.0),
                color: style.color,
            });
        }

        let shared: SharedString = final_text.into();
        let styled_text = if merged.is_empty() {
            StyledText::new(shared.clone())
        } else {
            StyledText::new(shared.clone()).with_highlights(merged)
        };
        let order = self.cfg.order_base + self.block_seq + 1;
        let id = self.next_id("txt");
        let block = EbookTextBlock {
            id,
            text: shared,
            styled_text,
            document_order: order,
            text_style: ts,
            links: Arc::new(shifted_links),
            on_link_click: self.cfg.on_link_click.clone(),
        };
        out.push(block.into_any_element());

        flow.text.clear();
    }

    fn render_image_el(
        &mut self,
        src: &str,
        style: &CssBoxStyle,
        align: Option<TextAlign>,
    ) -> Option<AnyElement> {
        let image = decode_data_uri_image(src, self.cfg.image_cache)?;
        let max_w = style
            .max_width_px
            .or(style.width_px)
            .unwrap_or(self.cfg.column_px)
            .min(self.cfg.column_px)
            .max(32.0);
        let mut im = img(ImageSource::Image(image))
            .max_w(px(max_w))
            .object_fit(ObjectFit::ScaleDown);
        if let Some(w) = style.width_px {
            im = im.w(px(w.min(self.cfg.column_px)));
        }
        if let Some(h) = style.height_px {
            im = im.h(px(h));
        }

        let id = self.next_id("img");
        let href = format!("img:{src}");
        let on_click = self.cfg.on_link_click.clone();
        let mut wrapper = h_flex().w_full().my_2();
        wrapper = match align {
            Some(TextAlign::Center) => wrapper.justify_center(),
            Some(TextAlign::Right) => wrapper.justify_end(),
            _ => wrapper.justify_center(),
        };
        let clickable = div()
            .id(id)
            .cursor_pointer()
            .on_click(move |_, window, cx| on_click(&href, window, cx))
            .child(im);
        Some(wrapper.child(clickable).into_any_element())
    }

    fn render_nodes_into_container(
        &mut self,
        nodes: &[DomNode],
        parent_style: &CssBoxStyle,
        preserve_ws: bool,
        link: Option<&str>,
    ) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = Vec::new();
        let mut flow = InlineFlow::default();

        for node in nodes {
            match node {
                DomNode::Comment(_) => {}
                DomNode::Text(t) => {
                    flow.push_text(t, parent_style, preserve_ws, link);
                }
                DomNode::Element {
                    tag_name,
                    attributes,
                    children,
                } => {
                    let tag = tag_name.to_ascii_lowercase();
                    if matches!(
                        tag.as_str(),
                        "head" | "style" | "script" | "title" | "meta" | "link" | "noscript"
                    ) {
                        continue;
                    }
                    let node_style = compute_node_style(&tag, attributes, parent_style, self.cfg);
                    if node_style.display_none || node_style.visibility_hidden {
                        continue;
                    }

                    match tag.as_str() {
                        "br" => {
                            flow.text.push('\n');
                        }
                        "hr" => {
                            self.flush_flow(&mut flow, parent_style, &mut out, preserve_ws);
                            let h = node_style.height_px.unwrap_or(1.0).max(1.0);
                            let bg = node_style
                                .border_colors
                                .iter()
                                .flatten()
                                .copied()
                                .next()
                                .or(node_style.background_color)
                                .unwrap_or(self.cfg.palette.border);
                            out.push(div().w_full().h(px(h)).my_3().bg(bg).into_any_element());
                        }
                        "img" => {
                            if let Some(src) = attributes.get("src") {
                                self.flush_flow(&mut flow, parent_style, &mut out, preserve_ws);
                                if let Some(el) =
                                    self.render_image_el(src, &node_style, node_style.text_align)
                                {
                                    out.push(el);
                                }
                            }
                        }
                        "svg" => {
                            if let Some(src) = find_svg_image_href(children) {
                                self.flush_flow(&mut flow, parent_style, &mut out, preserve_ws);
                                if let Some(el) =
                                    self.render_image_el(&src, &node_style, node_style.text_align)
                                {
                                    out.push(el);
                                }
                            }
                        }
                        "a" => {
                            let href = attributes.get("href").map(|s| s.as_str()).or(link);
                            self.collect_inline_children(
                                children,
                                &node_style,
                                preserve_ws,
                                href,
                                &mut flow,
                                &mut out,
                            );
                        }
                        _ if is_inline_tag(&tag) => {
                            self.collect_inline_children(
                                children,
                                &node_style,
                                preserve_ws,
                                link,
                                &mut flow,
                                &mut out,
                            );
                        }
                        "ul" | "ol" => {
                            self.flush_flow(&mut flow, parent_style, &mut out, preserve_ws);
                            let ordered = tag == "ol";
                            let start = attributes
                                .get("start")
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(1);
                            let mut list_box = v_flex().w_full().pl(px(18.0)).my_1().gap_1();
                            list_box =
                                apply_box_style(list_box, &node_style, self.cfg.palette.border);
                            let mut idx = 0;
                            for child in children {
                                if let DomNode::Element {
                                    tag_name: ctag,
                                    attributes: cattrs,
                                    children: cchildren,
                                } = child
                                    && ctag.eq_ignore_ascii_case("li")
                                {
                                    let li_style =
                                        compute_node_style("li", cattrs, &node_style, self.cfg);
                                    let marker = if ordered {
                                        format!("{}.", start + idx)
                                    } else {
                                        "•".to_string()
                                    };
                                    idx += 1;
                                    let li_children = self.render_nodes_into_container(
                                        cchildren, &li_style, false, link,
                                    );
                                    let fs =
                                        li_style.font_size_px.unwrap_or(self.cfg.base_font_size);
                                    let mut row = h_flex().w_full().items_start().gap_2();
                                    row = apply_box_style(row, &li_style, self.cfg.palette.border);
                                    row = row
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .text_size(px(fs))
                                                .text_color(
                                                    li_style.color.unwrap_or(self.cfg.palette.fg),
                                                )
                                                .child(marker),
                                        )
                                        .child(
                                            v_flex().flex_1().min_w(px(0.0)).children(li_children),
                                        );
                                    list_box = list_box.child(row);
                                }
                            }
                            out.push(list_box.into_any_element());
                        }
                        "table" => {
                            self.flush_flow(&mut flow, parent_style, &mut out, preserve_ws);
                            out.push(self.render_table(children, &node_style));
                        }
                        _ => {
                            // Block element: html, body, div, section, p, h1..h6, blockquote, pre, etc.
                            self.flush_flow(&mut flow, parent_style, &mut out, preserve_ws);
                            let mut child_els = if tag == "pre"
                                && link.is_none()
                                && let Some((code, lang)) =
                                    extract_plain_code_block(children, attributes)
                                && !code.trim().is_empty()
                            {
                                let nodes = SyntaxHighlightEngine::parse_code(&code, &lang);
                                let dark = is_palette_dark(self.cfg.theme, &self.cfg.palette);
                                let mut code_flow = InlineFlow {
                                    text: code.clone(),
                                    ..Default::default()
                                };
                                for node in nodes {
                                    if node.start_byte < node.end_byte
                                        && node.end_byte <= code.len()
                                        && code.is_char_boundary(node.start_byte)
                                        && code.is_char_boundary(node.end_byte)
                                        && let Some((color, font_weight, font_style)) =
                                            syntax_node_color(&node.kind, dark)
                                    {
                                        code_flow.spans.push((
                                            node.start_byte..node.end_byte,
                                            HighlightStyle {
                                                color: Some(color),
                                                font_weight,
                                                font_style,
                                                ..Default::default()
                                            },
                                        ));
                                    }
                                }
                                let mut pre_els = Vec::new();
                                self.flush_flow(&mut code_flow, &node_style, &mut pre_els, true);
                                pre_els
                            } else {
                                let is_pre = tag == "pre" || preserve_ws;
                                self.render_nodes_into_container(
                                    children,
                                    &node_style,
                                    is_pre,
                                    link,
                                )
                            };
                            if !child_els.is_empty()
                                || node_style.border_widths.iter().any(|&w| w > 0.0)
                                || node_style.background_color.is_some()
                            {
                                let mut block_box = v_flex().w_full();
                                block_box = apply_box_style(
                                    block_box,
                                    &node_style,
                                    self.cfg.palette.border,
                                );
                                block_box = block_box.children(child_els.drain(..));
                                out.push(block_box.into_any_element());
                            }
                        }
                    }
                }
            }
        }

        self.flush_flow(&mut flow, parent_style, &mut out, preserve_ws);
        out
    }

    fn collect_inline_children(
        &mut self,
        nodes: &[DomNode],
        style: &CssBoxStyle,
        preserve_ws: bool,
        link: Option<&str>,
        flow: &mut InlineFlow,
        out: &mut Vec<AnyElement>,
    ) {
        for node in nodes {
            match node {
                DomNode::Comment(_) => {}
                DomNode::Text(t) => flow.push_text(t, style, preserve_ws, link),
                DomNode::Element {
                    tag_name,
                    attributes,
                    children,
                } => {
                    let tag = tag_name.to_ascii_lowercase();
                    if matches!(
                        tag.as_str(),
                        "head" | "style" | "script" | "title" | "meta" | "link" | "noscript"
                    ) {
                        continue;
                    }
                    let cstyle = compute_node_style(&tag, attributes, style, self.cfg);
                    if cstyle.display_none || cstyle.visibility_hidden {
                        continue;
                    }
                    if tag == "br" {
                        flow.text.push('\n');
                    } else if tag == "img" {
                        if let Some(src) = attributes.get("src") {
                            self.flush_flow(flow, style, out, preserve_ws);
                            if let Some(el) = self.render_image_el(src, &cstyle, cstyle.text_align)
                            {
                                out.push(el);
                            }
                        }
                    } else if tag == "svg" {
                        if let Some(src) = find_svg_image_href(children) {
                            self.flush_flow(flow, style, out, preserve_ws);
                            if let Some(el) = self.render_image_el(&src, &cstyle, cstyle.text_align)
                            {
                                out.push(el);
                            }
                        }
                    } else {
                        let clink = if tag == "a" {
                            attributes.get("href").map(|s| s.as_str()).or(link)
                        } else {
                            link
                        };
                        self.collect_inline_children(
                            children,
                            &cstyle,
                            preserve_ws,
                            clink,
                            flow,
                            out,
                        );
                    }
                }
            }
        }
    }

    fn render_table(&mut self, nodes: &[DomNode], table_style: &CssBoxStyle) -> AnyElement {
        let mut rows: Vec<(bool, &Vec<DomNode>)> = Vec::new();
        collect_table_rows(nodes, false, &mut rows);

        let mut tbl = v_flex()
            .w_full()
            .my_2()
            .border_1()
            .border_color(self.cfg.palette.border);
        tbl = apply_box_style(tbl, table_style, self.cfg.palette.border);

        for (in_thead, row_cells) in rows {
            let mut row_el = h_flex().w_full().items_stretch();
            for cell in row_cells {
                if let DomNode::Element {
                    tag_name,
                    attributes,
                    children,
                } = cell
                {
                    let ctag = tag_name.to_ascii_lowercase();
                    if ctag != "td" && ctag != "th" {
                        continue;
                    }
                    let mut cstyle = compute_node_style(&ctag, attributes, table_style, self.cfg);
                    if ctag == "th" || in_thead {
                        cstyle.font_weight = Some(FontWeight::BOLD);
                        if cstyle.background_color.is_none() {
                            cstyle.background_color = Some(self.cfg.palette.surface);
                        }
                    }
                    let cell_children =
                        self.render_nodes_into_container(children, &cstyle, false, None);
                    let mut cell_box = v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .p_1p5()
                        .border_r_1()
                        .border_b_1()
                        .border_color(self.cfg.palette.border);
                    cell_box = apply_box_style(cell_box, &cstyle, self.cfg.palette.border);
                    cell_box = cell_box.children(cell_children);
                    row_el = row_el.child(cell_box);
                }
            }
            tbl = tbl.child(row_el);
        }
        tbl.into_any_element()
    }
}

fn collect_table_rows<'a>(
    nodes: &'a [DomNode],
    in_thead: bool,
    out: &mut Vec<(bool, &'a Vec<DomNode>)>,
) {
    for node in nodes {
        if let DomNode::Element {
            tag_name, children, ..
        } = node
        {
            match tag_name.to_ascii_lowercase().as_str() {
                "tr" => out.push((in_thead, children)),
                "thead" => collect_table_rows(children, true, out),
                "tbody" | "tfoot" => collect_table_rows(children, false, out),
                _ => collect_table_rows(children, in_thead, out),
            }
        }
    }
}

fn is_inline_tag(tag: &str) -> bool {
    matches!(
        tag,
        "span"
            | "b"
            | "strong"
            | "i"
            | "em"
            | "u"
            | "ins"
            | "s"
            | "del"
            | "strike"
            | "code"
            | "kbd"
            | "samp"
            | "tt"
            | "small"
            | "big"
            | "sub"
            | "sup"
            | "mark"
            | "abbr"
            | "cite"
            | "dfn"
            | "var"
            | "q"
            | "time"
            | "bdi"
            | "bdo"
            | "font"
            | "label"
    )
}

fn find_svg_image_href(nodes: &[DomNode]) -> Option<String> {
    for n in nodes {
        if let DomNode::Element {
            tag_name,
            attributes,
            children,
        } = n
        {
            if tag_name.eq_ignore_ascii_case("image")
                && let Some(v) = attributes
                    .get("href")
                    .or_else(|| attributes.get("xlink:href"))
            {
                return Some(v.to_string());
            }
            if let Some(found) = find_svg_image_href(children) {
                return Some(found);
            }
        }
    }
    None
}

/// Map generic CSS font family keywords to real platform system fonts so GPUI
/// DirectWrite / CoreText / Fontconfig renders distinct typefaces.
pub fn resolve_font_family(name: &str) -> String {
    match name.trim().to_ascii_lowercase().as_str() {
        "sans-serif" | "system-ui" | "" => {
            if cfg!(target_os = "windows") {
                "Segoe UI".to_string()
            } else if cfg!(target_os = "macos") {
                "Helvetica Neue".to_string()
            } else {
                "DejaVu Sans".to_string()
            }
        }
        "serif" => {
            if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
                "Georgia".to_string()
            } else {
                "DejaVu Serif".to_string()
            }
        }
        "monospace" => {
            if cfg!(target_os = "windows") {
                "Consolas".to_string()
            } else if cfg!(target_os = "macos") {
                "Menlo".to_string()
            } else {
                "DejaVu Sans Mono".to_string()
            }
        }
        other => {
            let _ = other;
            name.trim().to_string()
        }
    }
}

/// Render an `EbookDomTree` into a full-fidelity GPUI element tree.
pub fn render_ebook_dom(dom: &EbookDomTree, cfg: &RenderConfig<'_>) -> AnyElement {
    let root_style = CssBoxStyle {
        color: Some(cfg.palette.fg),
        font_size_px: Some(cfg.base_font_size),
        font_family: None,
        line_height_px: Some(cfg.base_font_size * cfg.line_height),
        rtl: Some(cfg.rtl_mode),
        ..Default::default()
    };
    let mut walker = DomWalker { cfg, block_seq: 0 };
    let children = walker.render_nodes_into_container(&dom.root_nodes, &root_style, false, None);
    v_flex().w_full().children(children).into_any_element()
}

pub type SeekCallback = Arc<dyn Fn(f32, &mut Window, &mut App) + Send + Sync>;

/// Interactive reading progress scrubber with chapter boundary ticks,
/// hover preview pill, and click/drag seeking.
pub struct ProgressScrubber {
    pub id: ElementId,
    pub progress: f32,
    pub ticks: Vec<f32>,
    pub track_color: Hsla,
    pub fill_color: Hsla,
    pub tick_color: Hsla,
    pub thumb_color: Hsla,
    pub preview_bg: Hsla,
    pub preview_fg: Hsla,
    pub preview_border: Hsla,
    pub format_preview: Arc<dyn Fn(f32) -> String + Send + Sync>,
    pub on_seek: SeekCallback,
}

#[derive(Default)]
struct ScrubberState {
    dragging: bool,
    hover_pct: Option<f32>,
}

impl Element for ProgressScrubber {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = px(16.0).into();
        style.flex_shrink = 0.0;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = window.use_state(cx, |_, _| ScrubberState::default());
        let (is_dragging, hover_pct) = {
            let s = state.read(cx);
            (s.dragging, s.hover_pct)
        };
        let hovered = hitbox.is_hovered(window) || is_dragging;
        let track_h = if hovered { px(7.0) } else { px(5.0) };
        let track_y = bounds.top() + (bounds.size.height - track_h) * 0.5;
        let track_bounds = Bounds::new(
            Point::new(bounds.left(), track_y),
            size(bounds.size.width, track_h),
        );
        window.paint_quad(PaintQuad {
            bounds: track_bounds,
            background: self.track_color.into(),
            corner_radii: Corners::all(px(3.5)),
            border_widths: Edges::default(),
            border_color: transparent_black(),
            border_style: BorderStyle::default(),
        });

        let frac = (self.progress / 100.0).clamp(0.0, 1.0);
        let fill_w = bounds.size.width * frac;
        if fill_w > px(0.5) {
            window.paint_quad(PaintQuad {
                bounds: Bounds::new(Point::new(bounds.left(), track_y), size(fill_w, track_h)),
                background: self.fill_color.into(),
                corner_radii: Corners::all(px(3.5)),
                border_widths: Edges::default(),
                border_color: transparent_black(),
                border_style: BorderStyle::default(),
            });
        }

        for &tick in &self.ticks {
            if tick > 0.01 && tick < 0.99 {
                let tx = bounds.left() + bounds.size.width * tick;
                window.paint_quad(PaintQuad {
                    bounds: Bounds::new(Point::new(tx - px(0.75), track_y), size(px(1.5), track_h)),
                    background: self.tick_color.into(),
                    corner_radii: Corners::default(),
                    border_widths: Edges::default(),
                    border_color: transparent_black(),
                    border_style: BorderStyle::default(),
                });
            }
        }

        let thumb_d = if hovered { px(13.0) } else { px(9.0) };
        let thumb_x =
            (bounds.left() + fill_w - thumb_d * 0.5).clamp(bounds.left(), bounds.right() - thumb_d);
        let thumb_y = bounds.top() + (bounds.size.height - thumb_d) * 0.5;
        window.paint_quad(PaintQuad {
            bounds: Bounds::new(Point::new(thumb_x, thumb_y), size(thumb_d, thumb_d)),
            background: self.thumb_color.into(),
            corner_radii: Corners::all(thumb_d * 0.5),
            border_widths: Edges::default(),
            border_color: transparent_black(),
            border_style: BorderStyle::default(),
        });

        if hovered && let Some(hp) = hover_pct {
            let label = (self.format_preview)(hp);
            let font = window.text_style().font();
            let run = TextRun {
                len: label.len(),
                font,
                color: self.preview_fg,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .shape_line(label.into(), px(11.0), &[run], None);
            let pad_x = px(8.0);
            let pad_y = px(4.0);
            let pill_w = shaped.width + pad_x * 2.0;
            let pill_h = px(22.0);
            let hx = bounds.left() + bounds.size.width * (hp / 100.0).clamp(0.0, 1.0);
            let pill_x = (hx - pill_w * 0.5)
                .clamp(bounds.left(), (bounds.right() - pill_w).max(bounds.left()));
            let pill_y = bounds.top() - pill_h - px(6.0);
            window.paint_quad(PaintQuad {
                bounds: Bounds::new(Point::new(pill_x, pill_y), size(pill_w, pill_h)),
                background: self.preview_bg.into(),
                corner_radii: Corners::all(px(5.0)),
                border_widths: Edges::all(px(1.0)),
                border_color: self.preview_border,
                border_style: BorderStyle::default(),
            });
            let _ = shaped.paint(
                Point::new(pill_x + pad_x, pill_y + pad_y),
                px(14.0),
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }

        let pct_at = move |pos: Point<Pixels>| -> f32 {
            let w: f32 = bounds.size.width.into();
            if w <= 1.0 {
                return 0.0;
            }
            let rel: f32 = (pos.x - bounds.left()).into();
            ((rel / w) * 100.0).clamp(0.0, 100.0)
        };

        window.on_mouse_event({
            let state = state.clone();
            let hb = hitbox.clone();
            move |ev: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let inside = hb.is_hovered(window);
                    let dragging = state.read(cx).dragging;
                    if inside || dragging {
                        let p = pct_at(ev.position);
                        state.update(cx, |s, cx| {
                            if s.hover_pct != Some(p) {
                                s.hover_pct = Some(p);
                                cx.notify();
                            }
                        });
                    } else if state.read(cx).hover_pct.is_some() {
                        state.update(cx, |s, cx| {
                            s.hover_pct = None;
                            cx.notify();
                        });
                    }
                }
            }
        });

        window.on_mouse_event({
            let state = state.clone();
            let hb = hitbox.clone();
            move |ev: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && ev.button == MouseButton::Left
                    && hb.is_hovered(window)
                {
                    let p = pct_at(ev.position);
                    state.update(cx, |s, cx| {
                        s.dragging = true;
                        s.hover_pct = Some(p);
                        cx.notify();
                    });
                }
            }
        });

        window.on_mouse_event({
            let state = state;
            let on_seek = self.on_seek.clone();
            move |ev: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left {
                    let was_dragging = state.update(cx, |s, cx| {
                        let d = s.dragging;
                        s.dragging = false;
                        if d {
                            cx.notify();
                        }
                        d
                    });
                    if was_dragging {
                        let p = pct_at(ev.position);
                        on_seek(p, window, cx);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn test_parse_css_length_units() {
        assert_eq!(parse_css_length("12px", 16.0, 600.0), Some(12.0));
        assert_eq!(parse_css_length("1.5em", 16.0, 600.0), Some(24.0));
        assert_eq!(parse_css_length("2rem", 16.0, 600.0), Some(32.0));
        assert_eq!(parse_css_length("50%", 16.0, 600.0), Some(300.0));
        assert_eq!(parse_css_length("0", 16.0, 600.0), Some(0.0));
        assert!((parse_css_length("12pt", 16.0, 600.0).unwrap() - 16.0).abs() < 0.05);
    }

    #[core::prelude::v1::test]
    fn test_parse_css_color_formats() {
        assert!(parse_css_color("#222").is_some());
        assert!(parse_css_color("#1a2b3c").is_some());
        assert!(parse_css_color("rgb(10, 20, 30)").is_some());
        assert!(parse_css_color("rgba(10, 20, 30, 0.5)").is_some());
        assert!(parse_css_color("navy").is_some());
        assert_eq!(parse_css_color("transparent"), Some(transparent_black()));
    }

    #[core::prelude::v1::test]
    fn test_merge_highlights_splits_overlapping_ranges() {
        let h1 = HighlightStyle {
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        };
        let h2 = HighlightStyle {
            background_color: Some(rgb(0xff9800).into()),
            ..Default::default()
        };
        let merged = merge_highlights(vec![(0..10, h1), (4..8, h2)]);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].0, 0..4);
        assert_eq!(merged[0].1.font_weight, Some(FontWeight::BOLD));
        assert_eq!(merged[0].1.background_color, None);
        assert_eq!(merged[1].0, 4..8);
        assert_eq!(merged[1].1.font_weight, Some(FontWeight::BOLD));
        assert!(merged[1].1.background_color.is_some());
        assert_eq!(merged[2].0, 8..10);
    }

    #[core::prelude::v1::test]
    fn test_chapter_header_css_resolution() {
        let html = r#"<html><head><style>
            .chapter-header { border-top: 1.5px solid #222; border-bottom: 1.5px solid #222; padding: 12px 0; }
            .chapter-num { text-align: right; font-size: 0.85em; }
        </style></head><body>
            <div class="chapter-header"><p class="chapter-num">CHAPTER 1</p></div>
        </body></html>"#;
        let inlined = ebook_rs::inline_stylesheet_rules(html);
        let dom = EbookDomTree::parse(&inlined);
        let palette = crate::ui_helpers::theme_palette_for("Light", "", "");
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let cfg = RenderConfig {
            id_prefix: "test",
            order_base: 0,
            theme: "Light",
            palette,
            base_font_size: 16.0,
            line_height: 1.6,
            font_family: "Georgia",
            text_align: "Default",
            rtl_mode: false,
            column_px: 640.0,
            annotations: &[],
            active_search: None,
            image_cache: &cache,
            on_link_click: Arc::new(|_, _, _| {}),
        };
        let parent = CssBoxStyle::default();

        fn find_el<'a>(nodes: &'a [DomNode], cls: &str) -> Option<&'a DomNode> {
            for n in nodes {
                if let DomNode::Element {
                    attributes,
                    children,
                    ..
                } = n
                {
                    if attributes.get("class").map(|c| c.as_str()) == Some(cls) {
                        return Some(n);
                    }
                    if let Some(f) = find_el(children, cls) {
                        return Some(f);
                    }
                }
            }
            None
        }

        let Some(DomNode::Element {
            tag_name,
            attributes,
            ..
        }) = find_el(&dom.root_nodes, "chapter-header")
        else {
            panic!("chapter-header not found");
        };
        let hdr_style =
            compute_node_style(&tag_name.to_ascii_lowercase(), attributes, &parent, &cfg);
        assert!((hdr_style.border_widths[0] - 1.5).abs() < 0.01);
        assert!((hdr_style.border_widths[2] - 1.5).abs() < 0.01);
        assert_eq!(hdr_style.padding[0], Some(12.0));
        assert_eq!(hdr_style.padding[2], Some(12.0));

        let Some(DomNode::Element {
            tag_name: ptag,
            attributes: pattrs,
            ..
        }) = find_el(&dom.root_nodes, "chapter-num")
        else {
            panic!("chapter-num not found");
        };
        let num_style = compute_node_style(&ptag.to_ascii_lowercase(), pattrs, &hdr_style, &cfg);
        assert_eq!(num_style.text_align, Some(TextAlign::Right));
        assert!((num_style.font_size_px.unwrap() - 13.6).abs() < 0.1);
    }

    #[core::prelude::v1::test]
    fn test_extract_plain_code_block_and_syntax_highlight() {
        let html = r#"<pre><code class="language-rust">// greeting
fn main() {
    struct Point { x: i32 }
}</code></pre>"#;
        let dom = EbookDomTree::parse(html);
        let Some(DomNode::Element {
            tag_name,
            attributes,
            children,
        }) = dom.root_nodes.first()
        else {
            panic!("expected <pre> element");
        };
        assert_eq!(tag_name.to_ascii_lowercase(), "pre");
        let (code, lang) =
            extract_plain_code_block(children, attributes).expect("plain code block");
        assert_eq!(lang, "rust");
        let nodes = SyntaxHighlightEngine::parse_code(&code, &lang);
        assert!(nodes.iter().any(|n| n.kind == "comment"));
        assert!(nodes.iter().any(|n| n.kind == "function_definition"));
        assert!(nodes.iter().any(|n| n.kind == "type_definition"));
    }

    #[core::prelude::v1::test]
    fn test_find_ci_ranges_handles_ascii_and_unicode_case_folding() {
        let ascii = find_ci_ranges("Hello World, hello!", "hello");
        assert_eq!(ascii, vec![0..5, 13..18]);

        // 'İ' lowercases to "i\u{307}" (3 bytes vs 2 bytes), changing byte length.
        let text = "Visit İstanbul and Straße today";
        let istanbul = find_ci_ranges(text, "İstanbul");
        assert_eq!(istanbul.len(), 1);
        assert_eq!(&text[istanbul[0].clone()], "İstanbul");
        let strasse = find_ci_ranges(text, "straße");
        assert_eq!(strasse.len(), 1);
        assert_eq!(&text[strasse[0].clone()], "Straße");
    }

    #[core::prelude::v1::test]
    fn test_semantic_html_styles_figure_dl_details() {
        let html = r#"<html><body>
            <figure><figcaption>Fig 1</figcaption></figure>
            <dl><dt>Term</dt><dd>Def</dd></dl>
            <details><summary>More</summary><p>Body</p></details>
        </body></html>"#;
        let dom = EbookDomTree::parse(html);
        let palette = crate::ui_helpers::theme_palette_for("Light", "", "");
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let cfg = RenderConfig {
            id_prefix: "sem",
            order_base: 0,
            theme: "Light",
            palette,
            base_font_size: 16.0,
            line_height: 1.6,
            font_family: "Georgia",
            text_align: "Default",
            rtl_mode: false,
            column_px: 640.0,
            annotations: &[],
            active_search: None,
            image_cache: &cache,
            on_link_click: Arc::new(|_, _, _| {}),
        };
        let parent = CssBoxStyle::default();

        fn find_tag<'a>(nodes: &'a [DomNode], wanted: &str) -> Option<&'a DomNode> {
            for n in nodes {
                if let DomNode::Element {
                    tag_name, children, ..
                } = n
                {
                    if tag_name.eq_ignore_ascii_case(wanted) {
                        return Some(n);
                    }
                    if let Some(f) = find_tag(children, wanted) {
                        return Some(f);
                    }
                }
            }
            None
        }

        let DomNode::Element { attributes, .. } = find_tag(&dom.root_nodes, "figcaption").unwrap()
        else {
            unreachable!()
        };
        let figcap = compute_node_style("figcaption", attributes, &parent, &cfg);
        assert_eq!(figcap.font_style, Some(FontStyle::Italic));
        assert_eq!(figcap.text_align, Some(TextAlign::Center));

        let DomNode::Element { attributes, .. } = find_tag(&dom.root_nodes, "dd").unwrap() else {
            unreachable!()
        };
        let dd = compute_node_style("dd", attributes, &parent, &cfg);
        assert_eq!(dd.margin[3], Some(20.0));

        let DomNode::Element { attributes, .. } = find_tag(&dom.root_nodes, "details").unwrap()
        else {
            unreachable!()
        };
        let details = compute_node_style("details", attributes, &parent, &cfg);
        assert_eq!(details.border_radius, Some(6.0));
        assert_eq!(details.border_widths, [1.0, 1.0, 1.0, 1.0]);
    }
}
