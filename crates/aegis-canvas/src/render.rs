//! From a scene and a viewport to a display list.
//!
//! Only what is visible is emitted: the spatial indexes pick the nodes and
//! edges that touch the viewport, so a frame costs O(log n + k) in the
//! number k of visible items, not in the size of the tree (A5).
//!
//! # Level of detail
//!
//! | Zoom | Drawn |
//! |---|---|
//! | 0.5 and up | everything: boxes, bars, titles, scores, kinds |
//! | 0.2 to 0.5 | boxes and rating bars, no text |
//! | below 0.2 | one filled rectangle per node |
//!
//! Text below 6 px is unreadable and costs the most to draw, so it is the
//! first thing dropped.

use aegis_model::NodeIdx;

use crate::display::{Align, DisplayList, Item};
use crate::geometry::{Point, Rect};
use crate::scene::{NodeKind, Scene};
use crate::style::{Paint, Stroke, Tone};
use crate::text::{chars_that_fit, wrap};
use crate::viewport::Viewport;
use aegis_model::GateKind;

/// Font size of titles and scores, in world pixels.
const FONT: f32 = 13.0;
/// Font size of the kind label.
const FONT_SMALL: f32 = 12.0;
/// Zoom from which text is drawn.
const TEXT_ZOOM: f32 = 0.5;
/// Zoom from which node outlines and rating bars are drawn.
const DETAIL_ZOOM: f32 = 0.2;

/// What to draw besides the tree itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderOptions {
    /// The selected node.
    pub selected: Option<NodeIdx>,
}

/// Draws the visible part of the scene.
///
/// The result is in screen coordinates, back to front: edges (ordinary
/// first, highlighted on top), then nodes in index order, then the
/// selection marks. The same inputs always give the same list (P4).
#[must_use]
pub fn render(scene: &Scene, view: &Viewport, options: &RenderOptions) -> DisplayList {
    let z = view.zoom;
    // A little slack so strokes at the edge of the screen are not cut.
    let visible = view.world_rect().inflate(4.0 / z);
    let mut items = Vec::new();

    let edge_ids = scene.edges_in(&visible);
    let width = |base: f32| (base * z).max(0.75);
    for highlighted in [false, true] {
        for &e in &edge_ids {
            if scene.is_edge_critical(e) != highlighted {
                continue;
            }
            let edge = scene.edges()[e as usize];
            let (a, b) = scene.edge_ends(edge);
            let (a, b) = (view.to_screen(a), view.to_screen(b));
            let dx = (b.x - a.x) / 2.0;
            let ghost_target = scene.style(edge.to).is_some_and(|s| s.ghost);
            let stroke = if ghost_target {
                Stroke::dashed(Paint::Accent, width(1.5))
            } else {
                match (highlighted, edge.link) {
                    (true, _) => Stroke::solid(Paint::Ink, width(3.5)),
                    (false, true) => Stroke::dashed(Paint::Graphite, width(1.25)),
                    (false, false) => Stroke::solid(Paint::Graphite, width(1.25)),
                }
            };
            items.push(Item::Curve {
                from: a,
                c1: Point::new(a.x + dx, a.y),
                c2: Point::new(b.x - dx, b.y),
                to: b,
                stroke,
            });
        }
    }

    for v in scene.nodes_in(&visible) {
        let selected = options.selected.is_some_and(|s| s.index() == v as usize);
        push_node(&mut items, scene, view, v, selected);
    }
    items_selection(&mut items, scene, view, options);
    DisplayList { items }
}

fn tone_of(scene: &Scene, v: u32) -> Tone {
    scene.style(v).map_or(Tone::Unrated, |s| s.tone)
}

fn push_node(items: &mut Vec<Item>, scene: &Scene, view: &Viewport, v: u32, selected: bool) {
    let z = view.zoom;
    let Some(idx) = scene.node_idx(v) else { return };
    let Some(world) = scene.node_rect(idx) else {
        return;
    };
    let top_left = view.to_screen(world.origin);
    let rect = Rect::new(top_left.x, top_left.y, world.size.w * z, world.size.h * z);
    let style = scene.style(v);
    let critical = style.is_some_and(|s| s.critical);
    let ghost = style.is_some_and(|s| s.ghost);
    let tone = tone_of(scene, v);

    if z < DETAIL_ZOOM {
        items.push(Item::Rect {
            rect,
            fill: Some(if ghost {
                Paint::AccentWash
            } else {
                Paint::Tone(tone)
            }),
            stroke: None,
            radius: 0.0,
            node: Some(v),
        });
        return;
    }

    let outline = if ghost {
        Stroke::dashed(Paint::Accent, if selected { 2.25 } else { 1.5 })
    } else if selected {
        Stroke::solid(Paint::Accent, 2.0)
    } else if critical {
        Stroke::solid(Paint::Ink, 2.5 * z.min(1.0))
    } else {
        Stroke::solid(Paint::Rule, 1.0)
    };
    items.push(Item::Rect {
        rect,
        fill: Some(if ghost {
            Paint::AccentWash
        } else {
            Paint::Panel
        }),
        stroke: Some(outline),
        radius: 2.0 * z,
        node: Some(v),
    });
    items.push(Item::Rect {
        rect: Rect::new(rect.origin.x, rect.origin.y, 6.0 * z, rect.size.h),
        fill: Some(if ghost {
            Paint::Accent
        } else {
            Paint::Tone(tone)
        }),
        stroke: None,
        radius: 0.0,
        node: None,
    });

    if z < TEXT_ZOOM {
        return;
    }
    let text_left = rect.origin.x + 16.0 * z;
    let max_chars = chars_that_fit(world.size.w - 26.0, FONT);
    for (i, line) in wrap(scene.label(v), max_chars, 2).into_iter().enumerate() {
        items.push(Item::Text {
            at: Point::new(text_left, rect.origin.y + (17.0 + 15.6 * i as f32) * z),
            text: line,
            size: FONT * z,
            bold: false,
            paint: if ghost { Paint::Accent } else { Paint::Ink },
            align: Align::Start,
        });
    }
    let base = rect.origin.y + (world.size.h - 8.0) * z;
    if let Some(score) = style.and_then(|s| s.score.as_deref()) {
        items.push(Item::Text {
            at: Point::new(text_left, base),
            text: score.to_owned(),
            size: FONT * z,
            bold: true,
            paint: if ghost { Paint::Accent } else { Paint::Ink },
            align: Align::Start,
        });
    }
    items.push(Item::Text {
        at: Point::new(rect.right() - 10.0 * z, base),
        text: if ghost {
            format!("ghost · {}", kind_label(scene.kind(v)))
        } else {
            kind_label(scene.kind(v)).to_owned()
        },
        size: FONT_SMALL * z,
        bold: ghost,
        paint: if ghost {
            Paint::Accent
        } else {
            Paint::Graphite
        },
        align: Align::End,
    });
}

fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Step => "step",
        NodeKind::Gate(GateKind::And) => "AND",
        NodeKind::Gate(GateKind::Or) => "OR",
        NodeKind::Gate(GateKind::SeqAnd) => "SEQ",
    }
}

/// Crop-mark style selection ticks at the four corners: readable without
/// color, like the mockup.
fn items_selection(items: &mut Vec<Item>, scene: &Scene, view: &Viewport, options: &RenderOptions) {
    let Some(sel) = options.selected else { return };
    let Some(world) = scene.node_rect(sel) else {
        return;
    };
    if !view
        .world_rect()
        .inflate(10.0 / view.zoom)
        .intersects(&world)
        || view.zoom < DETAIL_ZOOM
    {
        return;
    }
    let z = view.zoom;
    let tl = view.to_screen(world.origin);
    let (w, h) = (world.size.w * z, world.size.h * z);
    let tick = 10.0;
    let off = 7.0;
    let stroke = Stroke::solid(Paint::Accent, 2.0);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let cx = if sx < 0.0 { tl.x - off } else { tl.x + w + off };
        let cy = if sy < 0.0 { tl.y - off } else { tl.y + h + off };
        items.push(Item::Polyline {
            points: vec![
                Point::new(cx - sx * tick, cy),
                Point::new(cx, cy),
                Point::new(cx, cy - sy * tick),
            ],
            stroke,
        });
    }
}
