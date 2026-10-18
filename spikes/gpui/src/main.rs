//! AegisTARA UI toolkit spike: GPUI (Zed) through gpui-kit 0.7.1.
//!
//! Key interaction: right-click an attack node and change its attack feasibility.
//! Every factor, level, label, value and rating color comes from the pack at runtime;
//! the only fixed UI words are menu verbs ("Gate type", "Delete", ...).
extern crate gpui_kit as gpui;

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use gpui_kit::base::{Align, Placement, Positioner};
use gpui_kit::component::IndexPath;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use spike_model::{Kind, Model, Pack, Placed, WINDOW_H, WINDOW_W};

actions!(spike, [OpenNodeMenu, Undo, CancelEdit]);

const BG: u32 = 0xF6F7F9;
const NODE_BORDER: u32 = 0xD0D4DC;
const EDGE: u32 = 0x9AA3B2;
const SELECTED: u32 = 0x3D6DE8;
const TEXT: u32 = 0x1F2430;
const MUTED: u32 = 0x667085;
const CHIP_BG: u32 = 0xEEF1F6;
const STATUS_H: f32 = 24.0;
/// Below this zoom, nodes drop their text and show only a rating strip (level of detail).
const LOD_ZOOM: f32 = 0.45;

fn rating_color(rgb3: [u8; 3]) -> Rgba {
    rgb(((rgb3[0] as u32) << 16) | ((rgb3[1] as u32) << 8) | rgb3[2] as u32)
}

fn level_label(label: &str, value: u32) -> String {
    format!("{label}  ({value})")
}

/// A snapshot of what a node menu needs. The keyboard path builds the menu while the
/// canvas entity is being updated, and GPUI panics on re-entrant reads of that entity,
/// so the builder never reads the canvas; it only gets this copy.
#[derive(Clone)]
struct MenuData {
    node: String,
    kind: Kind,
    is_root: bool,
    read_only: bool,
    pack: Pack,
    /// Current level index per pack factor (steps only).
    current: Vec<Option<usize>>,
}

/// The "Edit feasibility…" panel: one built-in Select per factor plus a built-in Textarea.
struct Editor {
    node: String,
    selects: Vec<Entity<SelectState<Vec<SharedString>>>>,
    rationale: Entity<TextareaState>,
    _subs: Vec<Subscription>,
}

struct Canvas {
    model: Model,
    layout: Vec<Placed>,
    index: HashMap<String, usize>,
    layout_gen: u64,
    focus: FocusHandle,
    zoom: f32,
    pan: Point<Pixels>,
    selected: Option<String>,
    /// Pan drag: mouse position and pan at mouse-down.
    drag: Option<(Point<Pixels>, Point<Pixels>)>,
    kb_menu: Option<(Entity<PopupMenu>, Point<Pixels>, Subscription)>,
    editor: Option<Editor>,
    perf: bool,
    /// Process start, until the first frame is presented (startup time).
    started: Option<Instant>,
}

/// A menu item that runs `f` on the canvas when chosen.
fn item(
    label: impl Into<SharedString>,
    disabled: bool,
    canvas: &WeakEntity<Canvas>,
    f: impl Fn(&mut Canvas, &mut Window, &mut Context<Canvas>) + 'static,
) -> PopupMenuItem {
    let canvas = canvas.clone();
    PopupMenuItem::new(label).disabled(disabled).on_click(move |_, window, cx| {
        canvas.update(cx, |c, cx| f(c, window, cx)).ok();
    })
}

/// Builds the node menu from pack data. Used by right-click and by Shift+F10 / Menu key.
fn build_menu(
    canvas: WeakEntity<Canvas>,
    d: MenuData,
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    println!("MENU_OPEN {} read_only={}", d.node, d.read_only);
    let ro = d.read_only;
    // No header label: PopupMenu's first Down jumps to index 0 even when it is a
    // non-clickable label, which breaks keyboard navigation (gpui-component 0.7.1).
    let mut menu = menu.min_w(px(250.));
    if d.kind == Kind::Step {
        // One submenu per pack factor, one checked (radio-style) item per level.
        for (fi, factor) in d.pack.factors.iter().enumerate() {
            let (factor, current, canvas, node) = (factor.clone(), d.current[fi], canvas.clone(), d.node.clone());
            menu = menu.submenu(factor.label.clone(), window, cx, move |mut sub, _, _| {
                for (li, level) in factor.levels.iter().enumerate() {
                    let (node, fid, lid) = (node.clone(), factor.id.clone(), level.id.clone());
                    let label = level_label(&level.label, level.value);
                    let it = item(label, ro, &canvas, move |c, _, cx| c.apply(|m| m.set_factor(&node, &fid, &lid), cx));
                    sub = sub.item(it.checked(current == Some(li)));
                }
                sub
            });
        }
        let node = d.node.clone();
        menu = menu.separator().item(item("Edit feasibility…", ro, &canvas, move |c, w, cx| c.open_editor(&node, w, cx)));
    } else {
        let (kind, node, canvas2) = (d.kind, d.node.clone(), canvas.clone());
        menu = menu.submenu("Gate type", window, cx, move |mut sub, _, _| {
            for k in [Kind::And, Kind::Or] {
                let node = node.clone();
                let it = item(k.label(), ro, &canvas2, move |c, _, cx| c.apply(|m| m.set_kind(&node, k), cx));
                sub = sub.item(it.checked(kind == k));
            }
            sub
        });
        let node = d.node.clone();
        menu = menu.item(item("Add child step", ro, &canvas, move |c, _, cx| {
            c.apply(|m| m.add_child_step(&node).map(|_| ()), cx)
        }));
    }
    if !d.is_root {
        let node = d.node.clone();
        menu = menu.separator().item(item("Delete", ro, &canvas, move |c, _, cx| c.apply(|m| m.delete(&node), cx)));
    }
    menu
}

impl Canvas {
    fn new(model: Model, started: Instant, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut canvas = Self {
            model,
            layout: Vec::new(),
            index: HashMap::new(),
            layout_gen: u64::MAX,
            focus,
            zoom: 1.0,
            pan: point(px(0.), px(0.)),
            selected: None,
            drag: None,
            kb_menu: None,
            editor: None,
            perf: std::env::var("PERF").is_ok(),
            started: Some(started),
        };
        canvas.relayout();
        canvas
    }

    /// Re-run the model layout only when the model changed.
    fn relayout(&mut self) {
        if self.layout_gen == self.model.generation() {
            return;
        }
        self.layout = self.model.layout();
        self.index = self.layout.iter().enumerate().map(|(i, p)| (p.id.clone(), i)).collect();
        self.layout_gen = self.model.generation();
        if self.selected.as_ref().is_some_and(|s| !self.index.contains_key(s)) {
            self.selected = None;
        }
    }

    fn apply(&mut self, change: impl FnOnce(&mut Model) -> Result<(), String>, cx: &mut Context<Self>) {
        if let Err(e) = change(&mut self.model) {
            println!("REJECTED {e}");
        }
        self.relayout();
        cx.notify();
    }

    /// World (layout) to screen: `p * zoom + pan`.
    fn screen(&self, p: &Placed) -> Bounds<Pixels> {
        Bounds::new(
            point(px(p.x * self.zoom) + self.pan.x, px(p.y * self.zoom) + self.pan.y),
            size(px(p.w * self.zoom), px(p.h * self.zoom)),
        )
    }

    fn hit(&self, pos: Point<Pixels>) -> Option<String> {
        let x = f32::from(pos.x - self.pan.x) / self.zoom;
        let y = f32::from(pos.y - self.pan.y) / self.zoom;
        self.layout.iter().rev().find(|p| p.contains(x, y)).map(|p| p.id.clone())
    }

    fn menu_data(&self, id: &str) -> Option<MenuData> {
        let node = self.model.node(id)?;
        let pack = self.model.pack().clone();
        let current = pack
            .factors
            .iter()
            .map(|f| self.model.level_of(id, &f.id).and_then(|l| f.levels.iter().position(|x| x.id == l)))
            .collect();
        Some(MenuData {
            node: id.to_string(),
            kind: node.kind,
            is_root: id == self.model.root().id,
            read_only: self.model.read_only,
            pack,
            current,
        })
    }

    /// Shift+F10 / Menu key: GPUI's ContextMenuExt only reacts to the right button,
    /// so the keyboard path builds the same PopupMenu and shows it in an anchored overlay.
    fn open_menu_kb(&mut self, _: &OpenNodeMenu, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            println!("KBMENU no selection");
            return;
        };
        let Some(data) = self.menu_data(&id) else { return };
        let weak = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |m, w, cx| build_menu(weak, data, m, w, cx));
        menu.read(cx).focus_handle(cx).focus(window, cx);
        let sub = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.kb_menu = None;
            this.focus.focus(window, cx);
            cx.notify();
        });
        let b = self.screen(&self.layout[self.index[&id]]);
        self.kb_menu = Some((menu, point(b.left() + px(24.), b.bottom() - px(8.)), sub));
        println!("KBMENU_OPEN {id}");
        cx.notify();
    }

    fn open_editor(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(node) = self.model.node(id).cloned() else { return };
        let mut subs = Vec::new();
        let factors = self.model.pack().factors.clone();
        let selects = factors
            .iter()
            .map(|f| {
                let items: Vec<SharedString> =
                    f.levels.iter().map(|l| level_label(&l.label, l.value).into()).collect();
                let current = node.factors.get(&f.id).and_then(|lid| f.levels.iter().position(|l| &l.id == lid));
                let state = cx.new(|cx| SelectState::new(items, current.map(IndexPath::new), window, cx));
                // Live preview: re-render when a level is picked.
                subs.push(cx.subscribe_in(&state, window, |this, _, _: &SelectEvent<Vec<SharedString>>, _, cx| {
                    if let Some(ed) = &this.editor {
                        let (ap, r) = this.model.preview(&this.editor_levels(ed, cx));
                        println!("PREVIEW {} {} {}", ed.node, ap, this.model.pack().ratings[r].label);
                    }
                    cx.notify();
                }));
                state
            })
            .collect();
        let rationale = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 6)
                .placeholder("Why these values?")
                .default_value(node.rationale.clone())
        });
        self.editor = Some(Editor { node: id.to_string(), selects, rationale, _subs: subs });
        self.selected = Some(id.to_string());
        println!("EDITOR_OPEN {id}");
        cx.notify();
    }

    /// Factor levels currently chosen in the editor's Select widgets.
    fn editor_levels(&self, ed: &Editor, cx: &App) -> BTreeMap<String, String> {
        let factors = &self.model.pack().factors;
        factors
            .iter()
            .zip(&ed.selects)
            .filter_map(|(f, s)| {
                let row = s.read(cx).selected_index(cx)?.row;
                Some((f.id.clone(), f.levels.get(row)?.id.clone()))
            })
            .collect()
    }

    fn commit_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ed) = self.editor.take() else { return };
        let levels = self.editor_levels(&ed, cx);
        let text = ed.rationale.read(cx).value().to_string();
        println!("EDITOR_OK {} rationale={text:?}", ed.node);
        self.apply(|m| m.set_factors(&ed.node, levels, text), cx);
        // Focus was inside the removed panel; without this Ctrl+Z stops reaching the canvas.
        self.focus.focus(window, cx);
    }

    fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.take().is_some() {
            println!("EDITOR_CANCEL");
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if !self.model.undo() {
            println!("UNDO nothing to undo");
        }
        self.relayout();
        cx.notify();
    }

    fn on_wheel(&mut self, ev: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let dy = f32::from(ev.delta.pixel_delta(px(16.)).y);
        if dy == 0. {
            return;
        }
        let old = self.zoom;
        self.zoom = (self.zoom * if dy > 0. { 1.1 } else { 1. / 1.1 }).clamp(0.05, 4.);
        // Zoom around the cursor: the world point under the cursor stays put.
        let k = self.zoom / old;
        self.pan = point(
            ev.position.x - (ev.position.x - self.pan.x) * k,
            ev.position.y - (ev.position.y - self.pan.y) * k,
        );
        println!("ZOOM {:.3}", self.zoom);
        cx.notify();
    }

    fn render_node(&self, i: usize, weak: &WeakEntity<Self>) -> impl IntoElement + use<> {
        let p = &self.layout[i];
        let b = self.screen(p);
        let z = self.zoom;
        let selected = self.selected.as_deref() == Some(p.id.as_str());
        let ap = self.model.result(&p.id).map_or(0, |r| r.ap);
        let rating = self.model.rating_of(&p.id).expect("every node has a result");
        let color = rating_color(rating.rgb());
        let badge = format!("{ap} · {}", rating.label);
        let (id, weak) = (p.id.clone(), weak.clone());
        div()
            .id(ElementId::Name(p.id.clone().into()))
            .absolute()
            .left(b.origin.x)
            .top(b.origin.y)
            .w(b.size.width)
            .h(b.size.height)
            .bg(rgb(0xFFFFFF))
            .rounded(px(8. * z))
            .map(|d| if selected { d.border_2().border_color(rgb(SELECTED)) } else { d.border_1().border_color(rgb(NODE_BORDER)) })
            .overflow_hidden()
            .role(Role::TreeItem)
            .aria_label(format!("{} {}: {badge}", p.kind.label(), p.name))
            .aria_level(p.depth + 1)
            .aria_selected(selected)
            .map(|d| {
                if z < LOD_ZOOM {
                    // Level of detail: no text when zoomed far out, just the rating color.
                    d.child(div().absolute().left_0().right_0().bottom_0().h(px((10. * z).max(2.))).bg(color))
                } else {
                    d.flex()
                        .flex_col()
                        .justify_between()
                        .px(px(12. * z))
                        .py(px(9. * z))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6. * z))
                                .when(p.kind.is_gate(), |d| {
                                    d.child(
                                        div()
                                            .px(px(5. * z))
                                            .rounded(px(4. * z))
                                            .bg(rgb(CHIP_BG))
                                            .border_1()
                                            .border_color(rgb(NODE_BORDER))
                                            .text_size(px(10. * z))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0x4A5468))
                                            .child(p.kind.label()),
                                    )
                                })
                                .child(
                                    div().flex_1().min_w_0().truncate().text_size(px(14. * z)).text_color(rgb(TEXT)).child(p.name.clone()),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .px(px(8. * z))
                                        .rounded_full()
                                        .bg(color)
                                        .text_color(white())
                                        .text_size(px(12. * z))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(badge),
                                )
                                .child(div().text_size(px(11. * z)).text_color(rgb(MUTED)).child(p.id.clone())),
                        )
                }
            })
            // Built-in right-click menu. The builder runs on every open, so it reads fresh model data.
            .context_menu(move |menu, window, cx| {
                match weak.upgrade().and_then(|c| c.read(cx).menu_data(&id)) {
                    Some(data) => build_menu(weak.clone(), data, menu, window, cx),
                    None => menu,
                }
            })
    }

    fn render_editor(&self, ed: &Editor, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let p = &self.layout[*self.index.get(&ed.node)?];
        let anchor = self.screen(p);
        let pack = self.model.pack();
        let (ap, ri) = self.model.preview(&self.editor_levels(ed, cx));
        let rating = &pack.ratings[ri];
        let muted = |s: SharedString| div().text_size(px(12.)).text_color(rgb(MUTED)).child(s);
        let panel = div()
            .id("editor")
            .occlude()
            .key_context("Editor")
            .w(px(340.))
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_color(rgb(NODE_BORDER))
            .rounded(px(10.))
            .shadow_lg()
            .text_size(px(13.))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("Edit feasibility"))
                    .child(
                        div()
                            .id("preview")
                            .px_2()
                            .rounded_full()
                            .bg(rating_color(rating.rgb()))
                            .text_color(white())
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("{ap} · {}", rating.label)),
                    ),
            )
            .child(muted(format!("{} · {}", p.id, p.name).into()))
            .children(pack.factors.iter().zip(&ed.selects).map(|(f, s)| {
                div().flex().flex_col().gap_1().child(muted(f.label.clone().into())).child(Select::new(s).small())
            }))
            .child(muted("Rationale".into()))
            .child(Textarea::new(&ed.rationale))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .pt_1()
                    .child(Button::new("cancel").label("Cancel").small().on_click(cx.listener(|this, _, w, cx| this.close_editor(w, cx))))
                    .child(Button::new("ok").primary().label("OK").small().on_click(cx.listener(|this, _, w, cx| this.commit_editor(w, cx)))),
            );
        // Positioner places the panel beside the node, flips to the other side if there is
        // no room, and clamps to the window. deferred() draws it above the canvas.
        Some(
            deferred(
                Positioner::side(anchor).placement(Placement::Right).align(Align::Start).offset(px(8.)).margin(px(8.)).child(panel),
            )
            .with_priority(1),
        )
    }
}

impl Render for Canvas {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t0 = Instant::now();
        self.relayout();
        let vp = window.viewport_size();
        let view = Bounds::new(point(px(0.), px(0.)), size(vp.width, vp.height - px(STATUS_H)));
        // Culling: only nodes and edges that touch the view are built.
        let visible: Vec<usize> = (0..self.layout.len()).filter(|&i| self.screen(&self.layout[i]).intersects(&view)).collect();
        let edges: Vec<(Point<Pixels>, Point<Pixels>)> = self
            .layout
            .iter()
            .filter_map(|c| {
                let (pb, cb) = (self.screen(&self.layout[*self.index.get(c.parent.as_ref()?)?]), self.screen(c));
                let (a, b) = (point(pb.right(), pb.center().y), point(cb.left(), cb.center().y));
                let hull = Bounds::from_corners(
                    point(a.x.min(b.x), a.y.min(b.y) - px(1.)),
                    point(a.x.max(b.x), a.y.max(b.y) + px(1.)),
                );
                hull.intersects(&view).then_some((a, b))
            })
            .collect();
        if let Some(t) = self.started.take() {
            window.on_next_frame(move |_, _| println!("FIRST_FRAME {:.2}s", t.elapsed().as_secs_f32()));
        }
        if std::env::var("A11Y_DUMP").is_ok() {
            // F13 evidence: AccessKit state and the last tree GPUI sent (only built while an AT is connected).
            window.on_next_frame(|w, _| {
                let tree = w.debug_a11y_tree_json().unwrap_or_default();
                println!("A11Y enabled={} active={} tree_bytes={}", w.is_a11y_enabled(), w.is_a11y_active(), tree.len());
                if !tree.is_empty() && std::env::var("A11Y_DUMP").as_deref() == Ok("full") {
                    println!("A11Y_TREE {tree}");
                }
            });
        }
        if self.perf {
            let (nv, ne) = (visible.len(), edges.len());
            window.on_next_frame(move |_, _| println!("FRAME visible={nv} edges={ne} {:?}", t0.elapsed()));
        }
        let stroke = px((1.25 * self.zoom).clamp(0.6, 2.5));
        let weak = cx.entity().downgrade();
        let canvas_area = div()
            .id("canvas")
            .absolute()
            .left_0()
            .top_0()
            .w(view.size.width)
            .h(view.size.height)
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    match this.hit(ev.position) {
                        Some(id) => {
                            println!("SELECT {id}");
                            this.selected = Some(id);
                        }
                        None => this.drag = Some((ev.position, this.pan)),
                    }
                    this.focus.focus(window, cx);
                    cx.notify();
                }),
            )
            // Right-click also selects; the built-in context menu handles the same event for the menu.
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    if let Some(id) = this.hit(ev.position) {
                        this.selected = Some(id);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if let (Some((start, pan0)), Some(MouseButton::Left)) = (this.drag, ev.pressed_button) {
                    this.pan = point(pan0.x + ev.position.x - start.x, pan0.y + ev.position.y - start.y);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    if this.drag.take().is_some() {
                        println!("PAN {:.0} {:.0}", f32::from(this.pan.x), f32::from(this.pan.y));
                    }
                }),
            )
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            // Edges: one canvas element, smooth cubic curves from parent right to child left.
            .child(
                canvas(
                    move |_, _, _| edges,
                    move |_, edges, window, _| {
                        for (a, b) in edges {
                            let mid = (a.x + b.x) / 2.;
                            let mut path = PathBuilder::stroke(stroke);
                            path.move_to(a);
                            path.cubic_bezier_to(b, point(mid, a.y), point(mid, b.y));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, rgb(EDGE));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(visible.iter().map(|&i| self.render_node(i, &weak)));
        let status = format!(
            "GPUI · gpui-kit 0.7.1    Zoom {:.0}%    Nodes {} shown / {}    Selected: {}{}{}",
            self.zoom * 100.,
            visible.len(),
            self.layout.len(),
            self.selected.as_deref().unwrap_or("none"),
            if self.model.read_only { "    READ-ONLY" } else { "" },
            if self.model.can_undo() { "    Ctrl+Z to undo" } else { "" },
        );
        let editor = self.editor.as_ref().and_then(|ed| self.render_editor(ed, cx));
        div()
            .id("root")
            .size_full()
            .relative()
            .bg(rgb(BG))
            .track_focus(&self.focus)
            .key_context("Canvas")
            .on_action(cx.listener(Self::open_menu_kb))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(|this, _: &CancelEdit, w, cx| this.close_editor(w, cx)))
            .child(canvas_area)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .w_full()
                    .h(px(STATUS_H))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(0xFFFFFF))
                    .border_t_1()
                    .border_color(rgb(NODE_BORDER))
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(status)
                    .child("Right-click a node · Shift+F10 / Menu key on the selected node · Wheel zooms · Drag pans"),
            )
            .when_some(self.kb_menu.as_ref(), |d, (menu, pos, _)| {
                d.child(deferred(anchored().position(*pos).snap_to_window_with_margin(px(8.)).child(menu.clone())).with_priority(1))
            })
            .children(editor)
    }
}

fn main() {
    let started = Instant::now();
    env_logger::init();
    let model = Model::load_default().expect("load spike data");
    model.log_layout(0.0, 0.0);
    model.log_results();
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Light, None, cx);
        cx.bind_keys([
            KeyBinding::new("shift-f10", OpenNodeMenu, Some("Canvas")),
            KeyBinding::new("menu", OpenNodeMenu, Some("Canvas")),
            KeyBinding::new("ctrl-z", Undo, Some("Canvas")),
            KeyBinding::new("cmd-z", Undo, Some("Canvas")),
            KeyBinding::new("escape", CancelEdit, Some("Canvas")),
        ]);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(WINDOW_W), px(WINDOW_H)),
            ))),
            titlebar: Some(TitlebarOptions { title: Some("AegisTARA spike - GPUI".into()), ..Default::default() }),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Canvas::new(model, started, window, cx)))
            .expect("open window");
        println!("WINDOW_OPENED");
    });
}
