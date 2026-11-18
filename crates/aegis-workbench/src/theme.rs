//! The calm palette of the workbench (project-explorer.md section 8) as
//! light and dark token sets, and the WCAG contrast check used by the tests.

use serde::Deserialize;

const LIGHT: &str = include_str!("../themes/light.json");
const DARK: &str = include_str!("../themes/dark.json");

/// One theme: every colour as `#RRGGBB`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    /// Editors and grids.
    pub surface: String,
    /// Explorer and inspector background.
    pub sidebar: String,
    /// Headers, toolbars and the status bar.
    pub raised: String,
    /// Grid lines and separators.
    pub line: String,
    /// Field borders.
    pub line_strong: String,
    /// Text.
    pub ink: String,
    /// Secondary text.
    pub ink_muted: String,
    /// Selection, focus ring, primary button and links.
    pub accent: String,
    /// Selected row and selected tree node.
    pub accent_wash: String,
    /// Errors.
    pub error: String,
    /// Warnings.
    pub warning: String,
    /// The six system identity colours, slot 1 to 6.
    pub systems: [String; 6],
}

fn parse(text: &str) -> Tokens {
    // The files are embedded at build time; the theme test parses both.
    serde_json::from_str(text).expect("embedded theme file is valid")
}

/// The light theme.
#[must_use]
pub fn light() -> Tokens {
    parse(LIGHT)
}

/// The dark theme.
#[must_use]
pub fn dark() -> Tokens {
    parse(DARK)
}

/// `#RRGGBB` to its three bytes; `None` for anything else.
#[must_use]
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

fn luminance(rgb: [u8; 3]) -> f64 {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let [r, g, b] = rgb;
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// The WCAG 2 contrast ratio of two colours, from 1 to 21.
#[must_use]
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}
