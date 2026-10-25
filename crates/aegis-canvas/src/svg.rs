//! The SVG adapter: a display list as an SVG document.
//!
//! The same list the screen draws becomes the diagram in the PDF report, so
//! the two cannot disagree. The output is deterministic (P4): numbers are
//! printed with two decimals, attributes are in a fixed order, and text is
//! XML-escaped, so identical input gives identical bytes and the result can
//! be signed, hashed, and snapshot tested.
//!
//! The adapter resolves [`Paint`] tokens through an [`SvgTheme`]. The light
//! palette is the "drafting film" look of the design mockup; the dark one is
//! its night variant.

use std::fmt::Write as _;

use crate::display::{Align, DisplayList, Item};
use crate::geometry::{Point, Size};
use crate::render::{RenderOptions, render};
use crate::scene::Scene;
use crate::style::{Paint, Stroke, Tone};
use crate::viewport::Viewport;

/// A palette for the SVG adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgTheme {
    /// Dark text on a light sheet.
    Light,
    /// Light text on a dark sheet.
    Dark,
}

impl SvgTheme {
    /// The color for a token, as `#rrggbb`.
    #[must_use]
    pub fn hex(self, paint: Paint) -> &'static str {
        let light = self == SvgTheme::Light;
        match paint {
            Paint::Sheet => {
                if light {
                    "#E9EDF0"
                } else {
                    "#151B21"
                }
            }
            Paint::Panel => {
                if light {
                    "#FFFFFF"
                } else {
                    "#1E262E"
                }
            }
            Paint::Ink => {
                if light {
                    "#1D2733"
                } else {
                    "#E4EAF0"
                }
            }
            Paint::Graphite => {
                if light {
                    "#5B6875"
                } else {
                    "#9AA8B5"
                }
            }
            Paint::Rule => {
                if light {
                    "#C3CBD3"
                } else {
                    "#35424E"
                }
            }
            Paint::Wash => {
                if light {
                    "#F3F5F7"
                } else {
                    "#252F38"
                }
            }
            Paint::Accent => {
                if light {
                    "#5A3FC0"
                } else {
                    "#A894F2"
                }
            }
            Paint::AccentWash => {
                if light {
                    "#ECE8FA"
                } else {
                    "#2B2547"
                }
            }
            // Okabe-Ito based, safe for the common kinds of color blindness.
            Paint::Tone(Tone::High) => "#D55E00",
            Paint::Tone(Tone::Medium) => "#E69F00",
            Paint::Tone(Tone::Low) => "#56B4E9",
            Paint::Tone(Tone::VeryLow) => "#009E73",
            Paint::Tone(Tone::Unrated) => {
                if light {
                    "#C3CBD3"
                } else {
                    "#35424E"
                }
            }
        }
    }
}

/// Font stack of the report. Falls back to whatever the viewer has.
const FONT_FAMILY: &str = "Inter, 'Segoe UI', Helvetica, Arial, sans-serif";

/// Prints a number with at most two decimals and no `-0`.
fn num(v: f32) -> String {
    let mut s = format!("{v:.2}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_owned() } else { s }
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
}

fn stroke_attrs(theme: SvgTheme, s: &Stroke, out: &mut String) {
    let _ = write!(
        out,
        " stroke=\"{}\" stroke-width=\"{}\"",
        theme.hex(s.paint),
        num(s.width)
    );
    if s.dashed {
        let _ = write!(
            out,
            " stroke-dasharray=\"{} {}\"",
            num(s.width * 4.0),
            num(s.width * 3.0)
        );
    }
}

fn point(p: Point) -> String {
    format!("{} {}", num(p.x), num(p.y))
}

/// How to write the document.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgOptions {
    /// The palette.
    pub theme: SvgTheme,
    /// Paint the sheet color behind everything (off for transparent
    /// embedding).
    pub background: bool,
    /// Width of the document as displayed; the drawing scales to it through
    /// the `viewBox`. `None` uses the drawing's own width.
    pub display_width: Option<f32>,
}

impl SvgOptions {
    /// Light theme with a background, at the drawing's own size.
    #[must_use]
    pub const fn light() -> Self {
        SvgOptions {
            theme: SvgTheme::Light,
            background: true,
            display_width: None,
        }
    }
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self::light()
    }
}

/// Writes a display list as a complete SVG document. `size` is the extent
/// of the coordinates the list was rendered for.
///
/// Time O(items), space O(output).
#[must_use]
pub fn to_svg(list: &DisplayList, size: Size, options: &SvgOptions) -> String {
    let theme = options.theme;
    let mut out = String::with_capacity(256 + list.len() * 120);
    let display_w = options.display_width.unwrap_or(size.w);
    let display_h = if size.w > 0.0 {
        size.h * display_w / size.w
    } else {
        size.h
    };
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" \
         font-family=\"{FONT_FAMILY}\">",
        num(display_w),
        num(display_h),
        num(size.w),
        num(size.h)
    );
    if options.background {
        let _ = writeln!(
            out,
            "<rect width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            num(size.w),
            num(size.h),
            theme.hex(Paint::Sheet)
        );
    }
    for item in &list.items {
        match item {
            Item::Rect {
                rect,
                fill,
                stroke,
                radius,
                ..
            } => {
                let _ = write!(
                    out,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                    num(rect.origin.x),
                    num(rect.origin.y),
                    num(rect.size.w),
                    num(rect.size.h)
                );
                if *radius > 0.0 {
                    let _ = write!(out, " rx=\"{}\"", num(*radius));
                }
                let _ = write!(out, " fill=\"{}\"", fill.map_or("none", |f| theme.hex(f)));
                if let Some(s) = stroke {
                    stroke_attrs(theme, s, &mut out);
                }
                out.push_str("/>\n");
            }
            Item::Curve {
                from,
                c1,
                c2,
                to,
                stroke,
            } => {
                let _ = write!(
                    out,
                    "<path d=\"M {} C {}, {}, {}\" fill=\"none\"",
                    point(*from),
                    point(*c1),
                    point(*c2),
                    point(*to)
                );
                stroke_attrs(theme, stroke, &mut out);
                out.push_str("/>\n");
            }
            Item::Polyline { points, stroke } => {
                let pts: Vec<String> = points.iter().map(|p| point(*p)).collect();
                let _ = write!(out, "<polyline points=\"{}\" fill=\"none\"", pts.join(" "));
                stroke_attrs(theme, stroke, &mut out);
                out.push_str("/>\n");
            }
            Item::Text {
                at,
                text,
                size,
                bold,
                paint,
                align,
            } => {
                let _ = write!(
                    out,
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{}\"",
                    num(at.x),
                    num(at.y),
                    num(*size),
                    theme.hex(*paint)
                );
                if *bold {
                    out.push_str(" font-weight=\"700\"");
                }
                if *align == Align::End {
                    out.push_str(" text-anchor=\"end\"");
                }
                out.push('>');
                escape(text, &mut out);
                out.push_str("</text>\n");
            }
        }
    }
    out.push_str("</svg>\n");
    out
}

/// The whole tree as one SVG, for reports. Nothing is culled and text is
/// always drawn; the diagram is scaled to `options.display_width` (or kept at
/// its natural size) through the `viewBox`.
///
/// Time O(n) in the number of nodes.
#[must_use]
pub fn scene_to_svg(scene: &Scene, options: &SvgOptions, render_options: &RenderOptions) -> String {
    const MARGIN: f32 = 16.0;
    let bounds = scene.bounds();
    let mut full = Viewport::new(Size::new(
        bounds.size.w + 2.0 * MARGIN,
        bounds.size.h + 2.0 * MARGIN,
    ));
    full.origin = Point::new(bounds.origin.x - MARGIN, bounds.origin.y - MARGIN);
    let list = render(scene, &full, render_options);
    to_svg(&list, full.size, options)
}
