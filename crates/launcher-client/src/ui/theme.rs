//! Maps the six colors a server picks onto the whole component theme.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, Rgba, SharedString, hsla, px};
use relic_proto::Palette;

/// Parses `#rgb`, `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex(color: &str) -> Option<Hsla> {
    let hex = color.trim().strip_prefix('#')?;
    if !hex.is_ascii() {
        return None;
    }
    let digits: Vec<u8> = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).map(|c| c as u8).collect(),
        6 | 8 => hex.bytes().collect(),
        _ => return None,
    };
    let channel = |i: usize| {
        let pair = std::str::from_utf8(&digits[i * 2..i * 2 + 2]).ok()?;
        u8::from_str_radix(pair, 16)
            .ok()
            .map(|v| f32::from(v) / 255.)
    };
    let alpha = if digits.len() == 8 { channel(3)? } else { 1. };
    Some(
        Rgba {
            r: channel(0)?,
            g: channel(1)?,
            b: channel(2)?,
            a: alpha,
        }
        .into(),
    )
}

fn lighten(color: Hsla, by: f32) -> Hsla {
    hsla(color.h, color.s, (color.l + by).clamp(0., 1.), color.a)
}

fn alpha(color: Hsla, a: f32) -> Hsla {
    hsla(color.h, color.s, color.l, a)
}

/// Perceived brightness, 0 (black) to 1 (white).
fn luminance(color: Hsla) -> f32 {
    let rgb = Rgba::from(color);
    0.2126 * rgb.r + 0.7152 * rgb.g + 0.0722 * rgb.b
}

/// Black or white, whichever reads on top of `color`.
fn on(color: Hsla) -> Hsla {
    if luminance(color) > 0.55 {
        hsla(0., 0., 0.07, 1.)
    } else {
        hsla(0., 0., 1., 1.)
    }
}

/// The server's palette with every color resolved; anything unparsable
/// falls back to the stock value so a typo cannot blank the launcher.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    pub background: Hsla,
    pub surface: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub text: Hsla,
    pub text_dim: Hsla,
}

impl Colors {
    pub fn resolve(palette: &Palette) -> Self {
        let stock = Palette::default();
        let pick = |value: &str, fallback: &str| {
            parse_hex(value)
                .or_else(|| parse_hex(fallback))
                .unwrap_or_else(|| hsla(0., 0., 0.5, 1.))
        };
        Self {
            background: pick(&palette.background, &stock.background),
            surface: pick(&palette.surface, &stock.surface),
            border: pick(&palette.border, &stock.border),
            accent: pick(&palette.accent, &stock.accent),
            text: pick(&palette.text_primary, &stock.text_primary),
            text_dim: pick(&palette.text_secondary, &stock.text_secondary),
        }
    }

    pub fn is_dark(&self) -> bool {
        luminance(self.background) < 0.5
    }
}

/// Repaints the application in a server's colors (and font, if it has one).
pub fn apply(palette: &Palette, font: Option<SharedString>, cx: &mut App) {
    let c = Colors::resolve(palette);
    let dark = c.is_dark();
    let mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    if Theme::global(cx).mode != mode {
        Theme::change(mode, None, cx);
    }
    // Towards the text color: lighter on dark themes, darker on light ones.
    let step = if dark { 1. } else { -1. };
    let raised = lighten(c.surface, 0.04 * step);
    let danger = hsla(0., 0.72, if dark { 0.6 } else { 0.48 }, 1.);
    let success = hsla(0.39, 0.6, if dark { 0.55 } else { 0.36 }, 1.);
    let warning = hsla(0.11, 0.85, if dark { 0.6 } else { 0.42 }, 1.);

    Theme::update(cx, |theme| {
        let t = &mut theme.colors;
        t.background = c.background;
        t.foreground = c.text;
        t.border = c.border;
        t.input = c.border;
        t.ring = c.accent;
        t.caret = c.text;
        t.selection = alpha(c.accent, 0.35);

        t.primary = c.accent;
        t.primary_hover = lighten(c.accent, 0.05 * step);
        t.primary_active = lighten(c.accent, -0.04 * step);
        t.primary_foreground = on(c.accent);
        t.button_primary = t.primary;
        t.button_primary_hover = t.primary_hover;
        t.button_primary_active = t.primary_active;
        t.button_primary_foreground = t.primary_foreground;

        t.secondary = c.surface;
        t.secondary_hover = raised;
        t.secondary_active = lighten(c.surface, 0.07 * step);
        t.secondary_foreground = c.text;
        t.muted = c.surface;
        t.muted_foreground = c.text_dim;
        t.accent = raised;
        t.accent_foreground = c.text;
        t.popover = c.surface;
        t.popover_foreground = c.text;
        t.list = c.surface;
        t.list_hover = raised;
        t.list_active = alpha(c.accent, 0.16);
        t.list_active_border = c.accent;

        t.danger = danger;
        t.danger_hover = lighten(danger, 0.05);
        t.danger_active = lighten(danger, -0.05);
        t.danger_foreground = on(danger);
        t.success = success;
        t.success_foreground = on(success);
        t.warning = warning;
        t.warning_foreground = on(warning);
        t.link = c.accent;
        t.link_hover = lighten(c.accent, 0.06 * step);
        t.link_active = c.accent;

        t.progress_bar = c.accent;
        t.slider_bar = c.accent;
        t.slider_thumb = on(c.accent);
        t.switch = c.border;
        t.switch_thumb = c.text;
        t.scrollbar = alpha(c.background, 0.);
        t.scrollbar_thumb = alpha(c.text_dim, 0.35);
        t.scrollbar_thumb_hover = alpha(c.text_dim, 0.6);
        t.title_bar = c.background;
        t.title_bar_border = c.border;
        t.overlay = alpha(hsla(0., 0., 0., 1.), if dark { 0.6 } else { 0.35 });
        t.window_border = c.border;

        // Square everywhere: the launcher is built from blocks.
        theme.radius = px(0.);
        theme.radius_lg = px(0.);
        if let Some(font) = font {
            theme.font_family = font;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(color: Hsla) -> (u8, u8, u8) {
        let c = Rgba::from(color);
        let to = |v: f32| (v * 255.).round() as u8;
        (to(c.r), to(c.g), to(c.b))
    }

    #[test]
    fn hex_colors_parse() {
        assert_eq!(rgb(parse_hex("#fb7013").unwrap()), (0xfb, 0x70, 0x13));
        assert_eq!(rgb(parse_hex(" #FFF ").unwrap()), (255, 255, 255));
        assert!((parse_hex("#00000080").unwrap().a - 0.5).abs() < 0.01);
        assert!(parse_hex("fb7013").is_none());
        assert!(parse_hex("#12").is_none());
        assert!(parse_hex("#zzzzzz").is_none());
        assert!(parse_hex("#ффф").is_none());
    }

    #[test]
    fn bad_palette_entries_fall_back() {
        let palette = Palette {
            accent: "tomato".into(),
            ..Palette::default()
        };
        let colors = Colors::resolve(&palette);
        assert_eq!(
            colors.accent,
            parse_hex(&Palette::default().accent).unwrap()
        );
        assert!(colors.is_dark());
    }

    #[test]
    fn text_on_a_color_contrasts_with_it() {
        assert!(on(parse_hex("#ffe066").unwrap()).l < 0.5);
        assert!(on(parse_hex("#1f3a8a").unwrap()).l > 0.5);
    }
}
