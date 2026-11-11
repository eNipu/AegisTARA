//! The `Attack tree` tab: paints the `aegis-canvas` display list with GPUI.

use aegis_canvas::{
    Align, DisplayList, Item, LayoutConfig, NodeKind, NodeStyle, RenderOptions, Scene, Size,
    Stroke, Viewport, render,
};
use std::io::Write as _;
use std::time::{Duration, Instant};

use aegis_model::{AttackTree, NodeIdx};
use gpui::{
    App, BorderStyle, Bounds, Context, Entity, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _,
    PathBuilder, Pixels, Point, Render, ScrollEvent, Styled as _, TextAlign, TextRun, Window,
    canvas, div, point, px, quad, transparent_black,
};

use crate::args::Args;
use crate::measure;
use crate::palette::color;
use crate::selection::Selection;

/// State of a `--measure` run.
struct Measure {
    started: Instant,
    frames: usize,
    nodes: usize,
    canvas_ms: Vec<f64>,
    interval_ms: Vec<f64>,
    last_paint: Option<Instant>,
    prepaint_start: Instant,
    prepaint_ms: f64,
    centred: bool,
    direction: f32,
    items: usize,
    done: bool,
}

impl Measure {
    /// Prints the results and marks the run done.
    fn finish(&mut self) {
        let mut out = String::new();
        for (name, samples) in [
            ("FRAME_INTERVAL_MS", &mut self.interval_ms),
            ("CANVAS_MS", &mut self.canvas_ms),
        ] {
            if let Some(s) = measure::stats(samples) {
                out += &format!("{}\n", measure::line(name, &s));
            }
        }
        out += &format!("ITEMS {}\nNODES {}\nMEASURE_DONE\n", self.items, self.nodes);
        print!("{out}");
        let _ = std::io::stdout().flush();
        self.done = true;
    }
}

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
    measure: Option<Measure>,
}

impl AttackTreeTab {
    pub fn new(
        tree: &AttackTree,
        selection: Entity<Selection>,
        started: Instant,
        args: &Args,
        cx: &mut Context<Self>,
    ) -> Self {
        let measure = args.measure.then(|| Measure {
            started,
            frames: args.frames,
            nodes: args.nodes,
            canvas_ms: Vec::new(),
            interval_ms: Vec::new(),
            last_paint: None,
            prepaint_start: started,
            prepaint_ms: 0.0,
            centred: false,
            direction: -1.0,
            items: 0,
            done: false,
        });
        Self {
            focus_handle: cx.focus_handle(),
            scene: Scene::from_tree(tree, LayoutConfig::default(), |_| NodeStyle::default()),
            viewport: Viewport::new(Size::new(800.0, 600.0)),
            selected: None,
            drag_from: None,
            origin: Point::default(),
            selection,
            measure,
        }
    }

    /// A window position relative to the canvas, in canvas coordinates.
    fn local(&self, p: Point<Pixels>) -> aegis_canvas::Point {
        let d = p - self.origin;
        aegis_canvas::Point::new(d.x.into(), d.y.into())
    }

    /// Measure mode: record the frame, pan, and ask for the next frame.
    fn after_paint(&mut self, items: usize, begin: Instant, window: &mut Window) {
        let Some(m) = self.measure.as_mut().filter(|m| !m.done) else {
            return;
        };
        m.items = items;
        m.canvas_ms
            .push(m.prepaint_ms + begin.elapsed().as_secs_f64() * 1000.0);
        if m.canvas_ms.len() == 1 {
            println!(
                "FIRST_FRAME_MS {:.1}",
                m.started.elapsed().as_secs_f64() * 1000.0
            );
        }
        if m.canvas_ms.len() >= m.frames {
            m.finish();
            return;
        }
        if m.canvas_ms.len() % 100 == 0 {
            m.direction = -m.direction;
        }
        self.viewport.pan_by_screen(8.0 * m.direction, 0.0);
        window.request_animation_frame();
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
                if let Some(m) = tab.measure.as_mut() {
                    m.prepaint_start = Instant::now();
                    if !m.centred {
                        m.centred = true;
                        let (v, s) = (tab.viewport.size, tab.scene.bounds().size);
                        tab.viewport.origin.x = s.w / 2.0 - v.w / 2.0;
                        tab.viewport.origin.y = s.h / 2.0 - v.h / 2.0;
                    }
                }
                let selected = tab.selected;
                let list = render(&tab.scene, &tab.viewport, &RenderOptions { selected });
                if let Some(m) = tab.measure.as_mut() {
                    m.prepaint_ms = m.prepaint_start.elapsed().as_secs_f64() * 1000.0;
                }
                list
            })
        };
        let this = cx.entity();
        let paint =
            move |bounds: Bounds<Pixels>, list: DisplayList, window: &mut Window, cx: &mut App| {
                let begin = Instant::now();
                this.update(cx, |tab, _| {
                    if let Some(m) = tab.measure.as_mut()
                        && let Some(prev) = m.last_paint.replace(begin)
                    {
                        m.interval_ms
                            .push(begin.duration_since(prev).as_secs_f64() * 1000.0);
                    }
                });
                paint_list(&list, bounds.origin, window, cx);
                this.update(cx, |tab, _| {
                    tab.after_paint(list.items.len(), begin, window)
                });
                if this.read(cx).measure.as_ref().is_some_and(|m| m.done) {
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(Duration::from_secs(3)).await;
                        cx.update(|cx| cx.quit());
                    })
                    .detach();
                }
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
