//! Resolves canvas paint tokens to GPUI colours through the light SVG theme.

use aegis_canvas::{Paint, SvgTheme};

/// `#rrggbb` to its three bytes; `None` for anything else.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// The light-theme colour of a paint token; black if the hex is malformed.
pub fn color(paint: Paint) -> gpui::Rgba {
    let [r, g, b] = parse_hex(SvgTheme::Light.hex(paint)).unwrap_or([0, 0, 0]);
    gpui::rgb(u32::from_be_bytes([0, r, g, b]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_canvas::Tone;

    #[test]
    fn parses_hex() {
        assert_eq!(parse_hex("#E9EDF0"), Some([0xE9, 0xED, 0xF0]));
        assert_eq!(parse_hex("E9EDF0"), None);
        assert_eq!(parse_hex("#E9EDF"), None);
        assert_eq!(parse_hex("#E9EDFG"), None);
    }

    #[test]
    fn every_paint_has_a_colour() {
        let tones = [
            Tone::Unrated,
            Tone::VeryLow,
            Tone::Low,
            Tone::Medium,
            Tone::High,
        ];
        let paints = [
            Paint::Sheet,
            Paint::Panel,
            Paint::Ink,
            Paint::Graphite,
            Paint::Rule,
            Paint::Wash,
            Paint::Accent,
            Paint::AccentWash,
        ]
        .into_iter()
        .chain(tones.map(Paint::Tone));
        for p in paints {
            assert!(parse_hex(SvgTheme::Light.hex(p)).is_some(), "{p:?}");
        }
    }
}
