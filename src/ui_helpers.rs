use crate::library::truncate;
use base64::Engine as _;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    progress::Progress,
    text::TextView,
};
use gpui_kit::*;

#[derive(Clone, Copy)]
pub struct ThemePalette {
    pub bg: Hsla,
    pub fg: Hsla,
    pub surface: Hsla,
    pub border: Hsla,
    pub muted: Hsla,
}

pub const THEME_PRESETS: &[(&str, &str)] = &[
    ("Light", "Light"),
    ("Sepia", "Sepia"),
    ("Dark", "Dark"),
    ("OLED Black", "OLED"),
    ("Nord", "Nord"),
    ("Solarized", "Solarized"),
    ("Solarized Dark", "Sol. Dark"),
    ("E-Ink", "E-Ink"),
    ("Custom", "Custom"),
];

pub fn highlight_swatch_color(color: &str) -> Hsla {
    match color {
        "green" => rgb(0x4caf50).into(),
        "blue" => rgb(0x2196f3).into(),
        "pink" => rgb(0xe91e63).into(),
        "purple" => rgb(0x9c27b0).into(),
        _ => rgb(0xffd54f).into(),
    }
}

pub fn highlight_dot(
    id: String,
    color: &str,
    selected: bool,
    ring_color: Hsla,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let swatch = highlight_swatch_color(color);
    let mut outer = div()
        .id(id)
        .w(px(22.0))
        .h(px(22.0))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .border_2()
        .border_color(if selected {
            ring_color
        } else {
            rgba(0x00000000).into()
        })
        .on_click(on_click);
    outer = outer.child(div().w(px(14.0)).h(px(14.0)).rounded_full().bg(swatch));
    outer.into_any_element()
}

pub fn theme_swatch(
    id: String,
    short_label: &str,
    swatch_palette: ThemePalette,
    selected: bool,
    accent_border: Hsla,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    v_flex()
        .id(id)
        .w(px(98.0))
        .px_2()
        .py_1()
        .gap_1()
        .items_center()
        .rounded_md()
        .bg(swatch_palette.bg)
        .text_color(swatch_palette.fg)
        .border_2()
        .border_color(if selected {
            accent_border
        } else {
            swatch_palette.border
        })
        .on_click(on_click)
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Aa"),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(swatch_palette.muted)
                .child(short_label.to_string()),
        )
        .into_any_element()
}

pub fn is_dark_theme(theme: &str) -> bool {
    matches!(theme, "Dark" | "OLED Black" | "Nord" | "Solarized Dark")
}

pub fn parse_hex_color(s: &str) -> Option<Hsla> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)).into())
}

pub fn theme_palette_for(theme: &str, custom_bg: &str, custom_fg: &str) -> ThemePalette {
    match theme {
        "Dark" => ThemePalette {
            bg: rgb(0x1e1e1e).into(),
            fg: rgb(0xe0e0e0).into(),
            surface: rgb(0x262626).into(),
            border: rgb(0x3a3a3a).into(),
            muted: rgb(0x9ca3af).into(),
        },
        "OLED Black" => ThemePalette {
            bg: rgb(0x000000).into(),
            fg: rgb(0xd4d4d4).into(),
            surface: rgb(0x121212).into(),
            border: rgb(0x262626).into(),
            muted: rgb(0x888888).into(),
        },
        "Sepia" => ThemePalette {
            bg: rgb(0xf4ecd8).into(),
            fg: rgb(0x5b4636).into(),
            surface: rgb(0xebe0c6).into(),
            border: rgb(0xd5c6a8).into(),
            muted: rgb(0x826c58).into(),
        },
        "Nord" => ThemePalette {
            bg: rgb(0x2e3440).into(),
            fg: rgb(0xeceff4).into(),
            surface: rgb(0x3b4252).into(),
            border: rgb(0x4c566a).into(),
            muted: rgb(0xa3b1c6).into(),
        },
        "Solarized" => ThemePalette {
            bg: rgb(0xfdf6e3).into(),
            fg: rgb(0x657b83).into(),
            surface: rgb(0xeee8d5).into(),
            border: rgb(0xd8d0b8).into(),
            muted: rgb(0x839496).into(),
        },
        "Solarized Dark" => ThemePalette {
            bg: rgb(0x002b36).into(),
            fg: rgb(0x839496).into(),
            surface: rgb(0x073642).into(),
            border: rgb(0x1b4b57).into(),
            muted: rgb(0x657b83).into(),
        },
        "E-Ink" => ThemePalette {
            bg: rgb(0xffffff).into(),
            fg: rgb(0x000000).into(),
            surface: rgb(0xf5f5f5).into(),
            border: rgb(0x222222).into(),
            muted: rgb(0x444444).into(),
        },
        "Custom" => {
            let bg = parse_hex_color(custom_bg).unwrap_or(rgb(0xffffff).into());
            let fg = parse_hex_color(custom_fg).unwrap_or(rgb(0x202020).into());
            ThemePalette {
                bg,
                fg,
                surface: bg,
                border: rgb(0x888888).into(),
                muted: rgb(0x777777).into(),
            }
        }
        _ => ThemePalette {
            bg: rgb(0xffffff).into(),
            fg: rgb(0x202020).into(),
            surface: rgb(0xf7f7f8).into(),
            border: rgb(0xe2e2e6).into(),
            muted: rgb(0x6b7280).into(),
        },
    }
}

pub fn section_heading(text: &str) -> AnyElement {
    div()
        .text_size(px(15.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_string())
        .into_any_element()
}

pub fn app_icon_data_uri() -> &'static str {
    static URI: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URI.get_or_init(|| {
        let bytes = include_bytes!("../assets/icons/64x64.png");
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        format!("data:image/png;base64,{b64}")
    })
}

pub fn cover_art(
    id: String,
    cover: Option<String>,
    title: &str,
    format: &str,
    palette: ThemePalette,
    w: f32,
    h: f32,
) -> AnyElement {
    match cover {
        Some(uri) => div()
            .w(px(w))
            .h(px(h))
            .rounded_sm()
            .overflow_hidden()
            .child(TextView::html(
                id,
                format!("<img src=\"{uri}\" width=\"{w}\" height=\"{h}\"/>"),
            ))
            .into_any_element(),
        None => v_flex()
            .w(px(w))
            .h(px(h))
            .p_2()
            .justify_between()
            .rounded_sm()
            .bg(palette.surface)
            .border_1()
            .border_color(palette.border)
            .child(
                div()
                    .text_size(px(if w < 100.0 { 10.0 } else { 12.0 }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(truncate(title, if w < 100.0 { 22 } else { 42 })),
            )
            .child(div().text_size(px(10.0)).text_color(palette.muted).child(
                if format.is_empty() {
                    "EBOOK".to_string()
                } else {
                    format.to_uppercase()
                },
            ))
            .into_any_element(),
    }
}

pub struct BookTileMeta {
    pub id: String,
    pub cover: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub format: String,
    pub pct: f32,
}

pub fn grid_tile(
    meta: BookTileMeta,
    palette: ThemePalette,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    actions: AnyElement,
) -> AnyElement {
    let BookTileMeta {
        id,
        cover,
        title,
        subtitle,
        format,
        pct,
    } = meta;
    v_flex()
        .w(px(164.0))
        .gap_2()
        .p_2()
        .rounded_md()
        .bg(palette.surface)
        .border_1()
        .border_color(palette.border)
        .hover(|s| s.border_color(palette.muted))
        .child(
            v_flex()
                .id(format!("{id}-body"))
                .gap_2()
                .on_click(on_open)
                .child(cover_art(
                    format!("{id}-c"),
                    cover,
                    &title,
                    &format,
                    palette,
                    144.0,
                    196.0,
                ))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(truncate(&title, 32)),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(palette.muted)
                        .child(truncate(&subtitle, 36)),
                )
                .child(Progress::new(format!("{id}-p")).value(pct)),
        )
        .child(actions)
        .into_any_element()
}

pub fn wide_tile(
    meta: BookTileMeta,
    palette: ThemePalette,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    actions: AnyElement,
) -> AnyElement {
    let BookTileMeta {
        id,
        cover,
        title,
        subtitle,
        format,
        pct,
    } = meta;
    h_flex()
        .w(px(348.0))
        .gap_3()
        .p_2()
        .rounded_md()
        .bg(palette.surface)
        .border_1()
        .border_color(palette.border)
        .hover(|s| s.border_color(palette.muted))
        .child(
            div()
                .id(format!("{id}-cov"))
                .on_click(on_open)
                .child(cover_art(
                    format!("{id}-c"),
                    cover,
                    &title,
                    &format,
                    palette,
                    72.0,
                    100.0,
                )),
        )
        .child(
            v_flex()
                .flex_1()
                .gap_1()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(truncate(&title, 40)),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(palette.muted)
                        .child(truncate(&subtitle, 48)),
                )
                .child(Progress::new(format!("{id}-p")).value(pct))
                .child(actions),
        )
        .into_any_element()
}

pub fn cycle_row(
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

pub fn setting_row(label: &str, value: String, minus: AnyElement, plus: AnyElement) -> AnyElement {
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
