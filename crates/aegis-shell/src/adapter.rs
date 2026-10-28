//! The Slint adapter: a display list as Slint draw items.
//!
//! The canvas engine emits plain shapes in screen coordinates. This module
//! turns each into a [`DrawItem`] the Slint canvas draws as it is. Paints
//! become integer tokens that `ui/theme.slint` resolves to colors, so a
//! theme switch needs no work here. Slint paths cannot be dashed, so a dashed
//! curve is flattened to a polyline and cut into dashes here.

use aegis_canvas::{Align, DisplayList, Item, Paint, Point, Stroke, Tone};

use crate::DrawItem;

/// The paint token numbers shared with `ui/theme.slint`.
#[must_use]
pub fn token(paint: Paint) -> i32 {
    match paint {
        Paint::Sheet => 0,
        Paint::Panel => 1,
        Paint::Ink => 2,
        Paint::Graphite => 3,
        Paint::Rule => 4,
        Paint::Wash => 5,
        Paint::Accent => 6,
        Paint::AccentWash => 7,
        Paint::Tone(Tone::Unrated) => 8,
        Paint::Tone(Tone::VeryLow) => 9,
        Paint::Tone(Tone::Low) => 10,
        Paint::Tone(Tone::Medium) => 11,
        Paint::Tone(Tone::High) => 12,
    }
}

/// The rating bucket number the lists use: 0 not rated, 1 very low ... 4 high.
#[must_use]
pub fn bucket(tone: Tone) -> i32 {
    match tone {
        Tone::Unrated => 0,
        Tone::VeryLow => 1,
        Tone::Low => 2,
        Tone::Medium => 3,
        Tone::High => 4,
    }
}

const KIND_RECT: i32 = 0;
const KIND_PATH: i32 = 1;
const KIND_TEXT: i32 = 2;

/// Converts a display list, coalescing consecutive paths that share a stroke
/// so the software path rasterizer runs once per stroke layer instead of once
/// per edge. Time O(items), plus O(segments) per dashed curve.
#[must_use]
pub fn draw_items(list: &DisplayList) -> Vec<DrawItem> {
    let mut out: Vec<DrawItem> = Vec::with_capacity(list.items.len());
    for item in &list.items {
        let next = draw_item(item);
        if next.kind == KIND_PATH
            && let Some(prev) = out.last_mut()
            && prev.kind == KIND_PATH
            && prev.stroke == next.stroke
            && (prev.stroke_width - next.stroke_width).abs() < 1e-4
        {
            let mut merged = String::with_capacity(prev.commands.len() + 1 + next.commands.len());
            merged.push_str(prev.commands.as_str());
            if !merged.is_empty() && !next.commands.is_empty() {
                merged.push(' ');
            }
            merged.push_str(next.commands.as_str());
            prev.commands = merged.into();
            continue;
        }
        out.push(next);
    }
    out
}

/// Splits a display list into `(paths, rects, texts)` with same-stroke paths
/// coalesced, avoiding dummy hidden components in Slint `for` loops.
#[must_use]
pub fn draw_layers(list: &DisplayList) -> (Vec<DrawItem>, Vec<DrawItem>, Vec<DrawItem>) {
    let mut paths: Vec<DrawItem> = Vec::with_capacity(4);
    let mut rects: Vec<DrawItem> = Vec::with_capacity(list.items.len() / 2);
    let mut texts: Vec<DrawItem> = Vec::with_capacity(list.items.len() / 2);
    for item in &list.items {
        let next = draw_item(item);
        match next.kind {
            KIND_PATH => {
                if let Some(prev) = paths.last_mut()
                    && prev.stroke == next.stroke
                    && (prev.stroke_width - next.stroke_width).abs() < 1e-4
                {
                    let mut merged =
                        String::with_capacity(prev.commands.len() + 1 + next.commands.len());
                    merged.push_str(prev.commands.as_str());
                    if !merged.is_empty() && !next.commands.is_empty() {
                        merged.push(' ');
                    }
                    merged.push_str(next.commands.as_str());
                    prev.commands = merged.into();
                } else {
                    paths.push(next);
                }
            }
            KIND_RECT => rects.push(next),
            KIND_TEXT => texts.push(next),
            _ => {}
        }
    }
    (paths, rects, texts)
}

fn blank() -> DrawItem {
    DrawItem {
        kind: 0,
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
        fill: -1,
        stroke: -1,
        stroke_width: 0.0,
        radius: 0.0,
        commands: "".into(),
        text: "".into(),
        size: 0.0,
        bold: false,
        align_end: false,
    }
}

fn draw_item(item: &Item) -> DrawItem {
    match item {
        Item::Rect {
            rect,
            fill,
            stroke,
            radius,
            ..
        } => DrawItem {
            kind: KIND_RECT,
            x: rect.origin.x,
            y: rect.origin.y,
            w: rect.size.w,
            h: rect.size.h,
            fill: fill.map_or(-1, token),
            stroke: stroke.map_or(-1, |s| token(s.paint)),
            stroke_width: stroke.map_or(0.0, |s| s.width),
            radius: *radius,
            ..blank()
        },
        Item::Curve {
            from,
            c1,
            c2,
            to,
            stroke,
        } => path(&curve_commands(*from, *c1, *c2, *to, stroke), stroke),
        Item::Polyline { points, stroke } => path(&polyline_commands(points), stroke),
        Item::Text {
            at,
            text,
            size,
            bold,
            paint,
            align,
        } => DrawItem {
            kind: KIND_TEXT,
            x: at.x,
            y: at.y,
            fill: token(*paint),
            text: text.as_str().into(),
            size: *size,
            bold: *bold,
            align_end: *align == Align::End,
            ..blank()
        },
    }
}

fn path(commands: &str, stroke: &Stroke) -> DrawItem {
    DrawItem {
        kind: KIND_PATH,
        stroke: token(stroke.paint),
        stroke_width: stroke.width,
        commands: commands.into(),
        ..blank()
    }
}

fn polyline_commands(points: &[Point]) -> String {
    let mut out = String::new();
    for (i, p) in points.iter().enumerate() {
        out.push_str(if i == 0 { "M " } else { " L " });
        out.push_str(&format!("{:.2} {:.2}", p.x, p.y));
    }
    out
}

/// A cubic Bézier, solid as one `C` command, dashed as many short segments.
fn curve_commands(a: Point, c1: Point, c2: Point, b: Point, stroke: &Stroke) -> String {
    if !stroke.dashed {
        return format!(
            "M {:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
            a.x, a.y, c1.x, c1.y, c2.x, c2.y, b.x, b.y
        );
    }
    const STEPS: usize = 32;
    let at = |t: f32| {
        let u = 1.0 - t;
        let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        Point::new(
            b0 * a.x + b1 * c1.x + b2 * c2.x + b3 * b.x,
            b0 * a.y + b1 * c1.y + b2 * c2.y + b3 * b.y,
        )
    };
    let points: Vec<Point> = (0..=STEPS).map(|i| at(i as f32 / STEPS as f32)).collect();
    dash(&points, stroke.width * 4.0, stroke.width * 3.0)
}

/// Cuts a polyline into dashes of `on` pixels separated by `off` pixels.
/// Time O(points).
fn dash(points: &[Point], on: f32, off: f32) -> String {
    let mut out = String::new();
    let (mut drawing, mut left) = (true, on.max(0.5));
    let mut started = false;
    for pair in points.windows(2) {
        let (mut p, q) = (pair[0], pair[1]);
        let mut seg = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt();
        while seg > 0.0 {
            let step = seg.min(left);
            let t = step / seg;
            let next = Point::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
            if drawing {
                if !started {
                    out.push_str(&format!("M {:.2} {:.2} ", p.x, p.y));
                    started = true;
                }
                out.push_str(&format!("L {:.2} {:.2} ", next.x, next.y));
            }
            seg -= step;
            left -= step;
            p = next;
            if left <= 0.0 {
                drawing = !drawing;
                started = false;
                left = if drawing { on.max(0.5) } else { off.max(0.5) };
            }
        }
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use aegis_canvas::{Rect, Stroke};

    use super::*;

    #[test]
    fn rectangles_keep_geometry_and_tokens() {
        let list = DisplayList {
            items: vec![Item::Rect {
                rect: Rect::new(1.0, 2.0, 30.0, 40.0),
                fill: Some(Paint::Panel),
                stroke: Some(Stroke::solid(Paint::Accent, 2.0)),
                radius: 3.0,
                node: Some(7),
            }],
        };
        let items = draw_items(&list);
        assert_eq!(items.len(), 1);
        let r = &items[0];
        assert_eq!(
            (r.kind, r.x, r.y, r.w, r.h),
            (KIND_RECT, 1.0, 2.0, 30.0, 40.0)
        );
        assert_eq!(
            (r.fill, r.stroke, r.stroke_width, r.radius),
            (1, 6, 2.0, 3.0)
        );
    }

    #[test]
    fn text_keeps_its_baseline_and_alignment() {
        let list = DisplayList {
            items: vec![Item::Text {
                at: Point::new(10.0, 20.0),
                text: "Hello".into(),
                size: 13.0,
                bold: true,
                paint: Paint::Ink,
                align: Align::End,
            }],
        };
        let t = &draw_items(&list)[0];
        assert_eq!((t.kind, t.x, t.y, t.size), (KIND_TEXT, 10.0, 20.0, 13.0));
        assert!(t.bold && t.align_end);
        assert_eq!(t.text.as_str(), "Hello");
        assert_eq!(t.fill, 2);
    }

    #[test]
    fn solid_curves_are_one_command_and_dashed_ones_are_cut() {
        let (a, c1, c2, b) = (
            Point::new(0.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 100.0),
            Point::new(100.0, 100.0),
        );
        let solid = curve_commands(a, c1, c2, b, &Stroke::solid(Paint::Graphite, 1.25));
        assert!(solid.starts_with("M 0.00 0.00 C "));
        let dashed = curve_commands(a, c1, c2, b, &Stroke::dashed(Paint::Graphite, 1.25));
        let pieces = dashed.matches('M').count();
        assert!(pieces > 5, "many dashes: {pieces}");
        assert!(!dashed.contains('C'));
        // Dashes cover about 4/7 of the length.
        assert!(dashed.matches('L').count() >= pieces);
    }

    #[test]
    fn dashing_handles_degenerate_input() {
        assert_eq!(dash(&[], 4.0, 3.0), "");
        assert_eq!(dash(&[Point::new(1.0, 1.0)], 4.0, 3.0), "");
        let same = [Point::new(1.0, 1.0), Point::new(1.0, 1.0)];
        assert_eq!(dash(&same, 4.0, 3.0), "");
        let line = [Point::new(0.0, 0.0), Point::new(14.0, 0.0)];
        // 4 on, 3 off, 4 on, 3 off.
        assert_eq!(dash(&line, 4.0, 3.0).matches('M').count(), 2);
    }

    #[test]
    fn tokens_are_distinct_and_buckets_match_tones() {
        let paints = [
            Paint::Sheet,
            Paint::Panel,
            Paint::Ink,
            Paint::Graphite,
            Paint::Rule,
            Paint::Wash,
            Paint::Accent,
            Paint::AccentWash,
            Paint::Tone(Tone::Unrated),
            Paint::Tone(Tone::VeryLow),
            Paint::Tone(Tone::Low),
            Paint::Tone(Tone::Medium),
            Paint::Tone(Tone::High),
        ];
        let mut seen: Vec<i32> = paints.iter().map(|p| token(*p)).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..13).collect::<Vec<_>>());
        assert_eq!(bucket(Tone::High), 4);
        assert_eq!(bucket(Tone::Unrated), 0);
    }
}
