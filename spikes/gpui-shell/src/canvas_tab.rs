//! The `Attack tree` tab: paints the `aegis-canvas` display list with GPUI.

use aegis_canvas::{
    Align, DisplayList, Item, LayoutConfig, NodeKind, NodeStyle, RenderOptions, Scene, Size,
    Stroke, Viewport, render,
};
use aegis_model::{AttackTree, NodeIdx};
use gpui::{
    App, BorderStyle, Bounds, Context, Entity, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _,
    PathBuilder, Pixels, Point, Render, ScrollEvent, Styled as _, TextAlign, TextRun, Window,
    canvas, div, point, px, quad, transparent_black,
};

use crate::palette::color;
use crate::selection::Selection;

/// The `Attack tree` panel.
pub struct AttackTreeTab {
    focus_handle: FocusHandle,
    scene: Scene,
    viewport: Viewport,
    selected: Option<NodeIdx>,
    drag_from: Option<Point<Pixels>>,
    /// The canvas element's top-left corner in the window, from the last prepaint.
    origin: Point<Pixels>,
    selection: Entity<Selection>,
}

impl AttackTreeTab {
    pub fn new(tree: &AttackTree, selection: Entity<Selection>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            scene: Scene::from_tree(tree, LayoutConfig::default(), |_| NodeStyle::default()),
            viewport: Viewport::new(Size::new(800.0, 600.0)),
            selected: None,
            drag_from: None,
            origin: Point::default(),
            selection,
        }
    }

    /// A window position relative to the canvas, in canvas coordinates.
    fn local(&self, p: Point<Pixels>) -> aegis_canvas::Point {
        let d = p - self.origin;
        aegis_canvas::Point::new(d.x.into(), d.y.into())
    }

    fn mouse_down(&mut self, ev: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.drag_from = Some(ev.position);
        let world = self.viewport.to_world(self.local(ev.position));
        self.selected = self.scene.hit_test(world);
        if let Some(idx) = self.selected {
            let v = idx.index() as u32;
            let kind = match self.scene.kind(v) {
                NodeKind::Gate(_) => "gate",
                NodeKind::Step => "attack step",
            };
            let label = self.scene.label(v).to_string();
            let detail = format!("node {}", idx.index());
            self.selection.update(cx, |selection, cx| {
                *selection = Selection {
                    kind,
                    label,
                    detail,
                };
                cx.notify();
            });
        }
        cx.notify();
    }

    fn mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(from) = self.drag_from.filter(|_| ev.dragging()) else {
            return;
        };
        let d = ev.position - from;
        self.viewport.pan_by_screen(d.x.into(), d.y.into());
        self.drag_from = Some(ev.position);
        cx.notify();
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag_from = None;
    }

    fn scroll(&mut self, ev: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let dy = f32::from(ev.delta.pixel_delta(px(16.)).y);
        if dy == 0.0 {
            return;
        }
        let factor = if dy > 0.0 { 1.1 } else { 1.0 / 1.1 };
        self.viewport.zoom_at(self.local(ev.position), factor);
        cx.notify();
    }
}

dock_panel!(AttackTreeTab, "AttackTree", "Attack tree");

impl Render for AttackTreeTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let prepaint = move |bounds: Bounds<Pixels>, _: &mut Window, cx: &mut App| {
            this.update(cx, |tab, _| {
                tab.origin = bounds.origin;
                tab.viewport.size = Size::new(bounds.size.width.into(), bounds.size.height.into());
                let selected = tab.selected;
                render(&tab.scene, &tab.viewport, &RenderOptions { selected })
            })
        };
        let paint =
            |bounds: Bounds<Pixels>, list: DisplayList, window: &mut Window, cx: &mut App| {
                paint_list(&list, bounds.origin, window, cx);
            };
        div()
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll(cx.listener(Self::scroll))
            .child(canvas(prepaint, paint).size_full())
    }
}

/// A canvas point moved to the window, at `origin`.
fn at(origin: Point<Pixels>, p: aegis_canvas::Point) -> Point<Pixels> {
    origin + point(px(p.x), px(p.y))
}

/// A stroke path builder, dashed when the stroke is.
fn stroke_builder(stroke: &Stroke) -> PathBuilder {
    let builder = PathBuilder::stroke(px(stroke.width));
    if stroke.dashed {
        builder.dash_array(&[px(4.0), px(3.0)])
    } else {
        builder
    }
}

/// Paints every item of `list` in order, offset by `origin`.
fn paint_list(list: &DisplayList, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
    let font = window.text_style().font();
    for item in &list.items {
        match item {
            Item::Rect {
                rect,
                fill,
                stroke,
                radius,
                ..
            } => {
                let bounds = Bounds::new(
                    at(origin, rect.origin),
                    gpui::size(px(rect.size.w), px(rect.size.h)),
                );
                let background = fill.map_or(transparent_black(), |p| color(p).into());
                let (width, border, style) = match stroke {
                    Some(s) => (
                        s.width,
                        color(s.paint).into(),
                        if s.dashed {
                            BorderStyle::Dashed
                        } else {
                            BorderStyle::Solid
                        },
                    ),
                    None => (0.0, transparent_black(), BorderStyle::Solid),
                };
                window.paint_quad(quad(
                    bounds,
                    px(*radius),
                    background,
                    px(width),
                    border,
                    style,
                ));
            }
            Item::Curve {
                from,
                c1,
                c2,
                to,
                stroke,
            } => {
                let mut builder = stroke_builder(stroke);
                builder.move_to(at(origin, *from));
                builder.cubic_bezier_to(at(origin, *to), at(origin, *c1), at(origin, *c2));
                if let Ok(path) = builder.build() {
                    window.paint_path(path, color(stroke.paint));
                }
            }
            Item::Polyline { points, stroke } => {
                let mut builder = stroke_builder(stroke);
                let mut points = points.iter();
                if let Some(first) = points.next() {
                    builder.move_to(at(origin, *first));
                }
                for p in points {
                    builder.line_to(at(origin, *p));
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, color(stroke.paint));
                }
            }
            Item::Text {
                at: anchor,
                text,
                size,
                bold,
                paint,
                align,
            } => {
                let mut font = font.clone();
                if *bold {
                    font.weight = FontWeight::BOLD;
                }
                let run = TextRun {
                    len: text.len(),
                    font,
                    color: color(*paint).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(
                    text.replace('\n', " ").into(),
                    px(*size),
                    &[run],
                    None,
                );
                let mut top = at(origin, *anchor);
                top.y -= px(0.8 * size);
                if *align == Align::End {
                    top.x -= line.width;
                }
                // A failed glyph raster only loses this label; keep painting.
                let _ = line.paint(top, px(*size), TextAlign::Left, None, window, cx);
            }
        }
    }
}
