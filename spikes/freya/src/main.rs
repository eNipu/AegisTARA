//! AegisTARA UI toolkit spike: Freya 0.4.3.
//! Canvas = one Skia `canvas()` element; menus = built-in Menu/SubMenu/MenuItem/RadioItem;
//! popover = overlay rect with built-in Select + Button and a small custom multi-line editor.

use std::{borrow::Cow, cell::RefCell, collections::BTreeMap, rc::Rc, time::Instant};

use freya::engine::prelude as sk;
use freya::prelude::*;
use freya::text_edit::{EditableConfig, EditableEvent, EditorLine, TextEditor, use_editable};
use freya_winit::plugins::{FreyaPlugin, PluginEvent, PluginHandle};
use spike_model::{Kind, Model, Placed, WINDOW_H, WINDOW_W};

const STATUS_H: f32 = 28.0;
const BG: (u8, u8, u8) = (0xF6, 0xF7, 0xF9);
const BORDER: (u8, u8, u8) = (0xD0, 0xD4, 0xDC);
const ACCENT: (u8, u8, u8) = (0x3D, 0x6D, 0xE8);
const EDGE: (u8, u8, u8) = (0x9A, 0xA3, 0xB2);
const TEXT: (u8, u8, u8) = (0x1F, 0x24, 0x30);
const MUTED: (u8, u8, u8) = (0x8A, 0x91, 0x9E);
const POP_W: f32 = 380.0;
const POP_H: f32 = 560.0;

/// `DEBUG_INPUT=1` prints menu and key events, for the headless tests.
fn trace(msg: impl FnOnce() -> String) {
    thread_local!(static ON: bool = std::env::var("DEBUG_INPUT").is_ok());
    if ON.with(|on| *on) {
        println!("DBG {}", msg());
    }
}

fn main() {
    if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    }
    launch(
        LaunchConfig::new().with_plugin(RendererLog).with_window(
            WindowConfig::new(app)
                .with_size(WINDOW_W as f64, WINDOW_H as f64)
                .with_title("AegisTARA spike - freya")
                .with_background(BG),
        ),
    )
}

/// Prints which graphics driver Freya picked (Vulkan, OpenGL, Metal or Software).
struct RendererLog;
impl FreyaPlugin for RendererLog {
    fn plugin_id(&self) -> &'static str {
        "renderer-log"
    }
    fn on_event(&mut self, event: &mut PluginEvent, _: PluginHandle) {
        if let PluginEvent::WindowCreated { graphics_driver, window, .. } = event {
            println!("RENDERER {graphics_driver}");
            #[cfg(target_os = "linux")]
            if std::env::var("NO_X11_FOCUS").is_err() {
                x11_focus_if_no_wm(window);
            }
        }
    }
}

/// Test-harness helper, not toolkit functionality: on X11 without a window manager (Xvfb)
/// nobody gives the window keyboard focus, and winit drops key events until it has it.
/// winit's `focus_window()` only sends `_NET_ACTIVE_WINDOW`, which needs a WM.
#[cfg(target_os = "linux")]
fn x11_focus_if_no_wm(window: &freya::winit::window::Window) {
    use freya::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::{connection::Connection, protocol::xproto::{AtomEnum, ConnectionExt, InputFocus}};
    let win = match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Xlib(x)) => x.window as u32,
        Ok(RawWindowHandle::Xcb(x)) => x.window.get(),
        _ => return,
    };
    std::thread::spawn(move || {
        let Ok((conn, screen)) = x11rb::connect(None) else { return };
        let root = conn.setup().roots[screen].root;
        let has_wm = conn
            .intern_atom(false, b"_NET_SUPPORTING_WM_CHECK")
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|a| conn.get_property(false, root, a.atom, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok())
            .is_some_and(|p| p.value_len > 0);
        // BadMatch until the window is mapped, so retry for a few seconds.
        for _ in 0..100 {
            if has_wm || conn.set_input_focus(InputFocus::PARENT, win, x11rb::CURRENT_TIME).is_ok_and(|c| c.check().is_ok()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}

#[derive(Clone, Copy, PartialEq)]
struct View {
    pan_x: f32,
    pan_y: f32,
    zoom: f32,
}

#[derive(Clone, PartialEq)]
struct MenuReq {
    node: String,
    x: f32,
    y: f32,
    seq: u64,
}

#[derive(Clone, PartialEq)]
struct Draft {
    node: String,
    levels: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
struct Drag {
    x: f64,
    y: f64,
    pan_x: f32,
    pan_y: f32,
}

/// Layout and badge cache, rebuilt only when the model generation changes.
#[derive(Default)]
struct Scene {
    generation: Option<u64>,
    placed: Vec<Placed>,
    parents: Vec<Option<usize>>,
    badges: Vec<(String, [u8; 3])>,
}

impl Scene {
    fn sync(&mut self, model: &Model) {
        if self.generation == Some(model.generation()) {
            return;
        }
        self.placed = model.layout();
        let index: std::collections::HashMap<&str, usize> =
            self.placed.iter().enumerate().map(|(i, p)| (p.id.as_str(), i)).collect();
        self.parents = self.placed.iter().map(|p| p.parent.as_deref().and_then(|id| index.get(id).copied())).collect();
        self.badges = self
            .placed
            .iter()
            .map(|p| match (model.result(&p.id), model.rating_of(&p.id)) {
                (Some(r), Some(rating)) => (format!("{} · {}", r.ap, rating.label), rating.rgb()),
                _ => ("?".into(), [128, 128, 128]),
            })
            .collect();
        self.generation = Some(model.generation());
    }

    fn hit(&self, v: View, x: f32, y: f32) -> Option<&Placed> {
        let (wx, wy) = ((x - v.pan_x) / v.zoom, (y - v.pan_y) / v.zoom);
        self.placed.iter().rev().find(|p| p.contains(wx, wy))
    }

    fn screen_rect(&self, v: View, id: &str) -> Option<(f32, f32, f32, f32)> {
        let p = self.placed.iter().find(|p| p.id == id)?;
        Some((p.x * v.zoom + v.pan_x, p.y * v.zoom + v.pan_y, p.w * v.zoom, p.h * v.zoom))
    }
}

/// All app state handles (each `State` is a cheap Copy handle).
#[derive(Clone, Copy)]
struct Ctx {
    model: State<Model>,
    view: State<View>,
    selected: State<Option<String>>,
    menu: State<Option<MenuReq>>,
    skip_close: State<bool>,
    draft: State<Option<Draft>>,
    rationale: State<String>,
}

impl Ctx {
    fn open_menu(mut self, node: String, x: f32, y: f32, from_mouse: bool) {
        let seq = self.menu.peek().as_ref().map_or(0, |m| m.seq + 1);
        self.selected.set(Some(node.clone()));
        self.skip_close.set(from_mouse);
        self.menu.set(Some(MenuReq { node, x, y, seq }));
    }
    fn close_menu(mut self) {
        self.menu.set(None);
    }
    fn open_popover(mut self, node: &str) {
        let (levels, rationale) = {
            let model = self.model.peek();
            let n = model.node(node).expect("node");
            (n.factors.clone(), n.rationale.clone())
        };
        self.rationale.set(rationale);
        self.draft.set(Some(Draft { node: node.to_string(), levels }));
        self.close_menu();
    }
    fn apply(mut self, change: impl FnOnce(&mut Model) -> Result<(), String>) {
        trace(|| "menu action".into());
        if let Err(e) = change(&mut self.model.write()) {
            println!("ERROR {e}");
        }
        self.close_menu();
    }
}

fn app() -> impl IntoElement {
    let model = use_state(|| {
        let m = Model::load_default().expect("load spike data");
        m.log_layout(0.0, 0.0);
        m.log_results();
        m
    });
    let ctx = Ctx {
        model,
        view: use_state(|| View { pan_x: 0.0, pan_y: 0.0, zoom: 1.0 }),
        selected: use_state(|| None),
        menu: use_state(|| None),
        skip_close: use_state(|| false),
        draft: use_state(|| None),
        rationale: use_state(String::new),
    };
    let mut drag = use_state(|| None::<Drag>);
    let scene = use_hook(|| Rc::new(RefCell::new(Scene::default())));
    let frame = use_hook(|| Rc::new(std::cell::Cell::new(0u64)));
    frame.set(frame.get() + 1);

    scene.borrow_mut().sync(&model.read());
    let view = *ctx.view.read();
    let selected = ctx.selected.read().clone();
    let read_only = model.read().read_only;

    // Canvas: draw callback with culling. The key forces a repaint with fresh data,
    // because Freya's RenderCallback compares equal and would otherwise keep the old closure.
    let draw_scene = scene.clone();
    let draw_sel = selected.clone();
    let perf = std::env::var("PERF").is_ok();
    let graph = canvas(RenderCallback::new(move |c: &mut CanvasContext| {
        let t0 = Instant::now();
        let drawn = draw(c, &draw_scene.borrow(), view, draw_sel.as_deref());
        if perf {
            println!("DRAW {:.2}ms nodes={drawn}", t0.elapsed().as_secs_f64() * 1000.0);
        }
    }))
    .key(frame.get())
    .width(Size::fill())
    .height(Size::flex(1.0))
    .on_mouse_down({
        let scene = scene.clone();
        move |e: Event<MouseEventData>| {
            let (x, y) = (e.element_location.x as f32, e.element_location.y as f32);
            let v = *ctx.view.peek();
            let hit = scene.borrow().hit(v, x, y).map(|p| p.id.clone());
            trace(|| format!("canvas down {:?} {x} {y} {hit:?}", e.button));
            match (e.button, hit) {
                (Some(MouseButton::Left), Some(id)) => {
                    let mut sel = ctx.selected;
                    sel.set(Some(id));
                }
                (Some(MouseButton::Left), None) => drag.set(Some(Drag {
                    x: e.global_location.x,
                    y: e.global_location.y,
                    pan_x: v.pan_x,
                    pan_y: v.pan_y,
                })),
                _ => {}
            }
        }
    })
    // Open on release (like Windows): opening on press leaves the release pending,
    // which arrives after the menu mounted and breaks the next press on a menu item.
    .on_mouse_up({
        let scene = scene.clone();
        move |e: Event<MouseEventData>| {
            if e.button != Some(MouseButton::Right) {
                return;
            }
            let v = *ctx.view.peek();
            let hit = scene.borrow().hit(v, e.element_location.x as f32, e.element_location.y as f32).map(|p| p.id.clone());
            trace(|| format!("canvas up {:?} {hit:?}", e.button));
            match hit {
                Some(id) => ctx.open_menu(id, e.global_location.x as f32, e.global_location.y as f32, false),
                None => ctx.close_menu(),
            }
        }
    })
    .on_wheel(move |e: Event<WheelEventData>| {
        let mut view = ctx.view;
        let v = *view.peek();
        let factor = if e.delta_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
        let zoom = (v.zoom * factor).clamp(0.2, 3.0);
        let (sx, sy) = (e.element_location.x as f32, e.element_location.y as f32);
        let (wx, wy) = ((sx - v.pan_x) / v.zoom, (sy - v.pan_y) / v.zoom);
        view.set(View { pan_x: sx - wx * zoom, pan_y: sy - wy * zoom, zoom });
    });

    let n_nodes = scene.borrow().placed.len();
    let status = format!(
        "{}   ·   zoom {:.0}%   ·   {} nodes   ·   {}{}",
        selected.as_deref().map(|s| format!("Selected: {s}")).unwrap_or("No selection".into()),
        view.zoom * 100.0,
        n_nodes,
        if model.read().can_undo() { "Ctrl+Z to undo" } else { "nothing to undo" },
        if read_only { "   ·   READ-ONLY" } else { "" },
    );

    let menu_el = ctx.menu.read().clone().map(|req| {
        let menu = build_menu(ctx, &model.read(), &req.node);
        let mut skip = ctx.skip_close;
        rect()
            .key(req.seq)
            .layer(Layer::Overlay)
            .position(Position::new_global().left(req.x).top(req.y))
            .child(menu.on_close(move |_| {
                trace(|| format!("menu on_close skip={}", *skip.peek()));
                if *skip.peek() {
                    skip.set(false);
                } else {
                    ctx.close_menu();
                }
            }))
    });

    let popover_el = ctx.draft.read().clone().and_then(|d| {
        let (x, y, w, _) = scene.borrow().screen_rect(view, &d.node)?;
        let root = *Platform::get().root_size.read();
        let mut px = x + w + 12.0;
        if px + POP_W > root.width - 8.0 {
            px = (x - POP_W - 12.0).max(8.0);
        }
        let py = y.min(root.height - STATUS_H - POP_H - 8.0).max(8.0);
        Some(popover(ctx, &model.read(), d, px, py))
    });

    rect()
        .expanded()
        .background(BG)
        .content(Content::flex())
        .font_size(14.0)
        .color(TEXT)
        .on_global_pointer_move(move |e: Event<PointerEventData>| {
            if let Some(d) = *drag.peek() {
                let p = e.global_location();
                let mut view = ctx.view;
                let zoom = view.peek().zoom;
                view.set(View { pan_x: d.pan_x + (p.x - d.x) as f32, pan_y: d.pan_y + (p.y - d.y) as f32, zoom });
            }
        })
        .on_global_pointer_down(move |e: Event<PointerEventData>| trace(|| format!("global down {:?}", e.button())))
        .on_global_pointer_press(move |e: Event<PointerEventData>| {
            trace(|| format!("global press {:?}", e.button()));
            if drag.peek().is_some() {
                drag.set(None);
            }
        })
        .on_global_key_down({
            let scene = scene.clone();
            move |e: Event<KeyboardEventData>| on_key(ctx, &scene, &e)
        })
        .child(graph)
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(STATUS_H))
                .padding((0.0, 12.0))
                .main_align(Alignment::center())
                .background((0xEC, 0xEE, 0xF2))
                .border(Border::new().width(1.0).fill(BORDER))
                .font_size(12.0)
                .color(MUTED)
                .child(status),
        )
        .maybe_child(menu_el)
        .maybe_child(popover_el)
}

fn on_key(ctx: Ctx, scene: &Rc<RefCell<Scene>>, e: &KeyboardEventData) {
    trace(|| format!("key {:?} {:?}", e.key, e.modifiers));
    let shift = e.modifiers.contains(Modifiers::SHIFT);
    let ctrl = e.modifiers.contains(Modifiers::CONTROL) || e.modifiers.contains(Modifiers::META);
    let menu_open = ctx.menu.peek().is_some();
    let popover_open = ctx.draft.peek().is_some();
    match &e.key {
        Key::Named(NamedKey::F10) if shift => open_for_selection(ctx, scene),
        Key::Named(NamedKey::ContextMenu) => open_for_selection(ctx, scene),
        Key::Named(NamedKey::Escape) if popover_open && !menu_open => {
            let mut draft = ctx.draft;
            draft.set(None);
        }
        Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp) if menu_open => {
            // Built-in arrows only move inside the focused group. Move focus into the menu first.
            let platform = Platform::get();
            if platform.focused_accessibility_node.peek().member_of().is_none() {
                platform.send(UserEvent::FocusAccessibilityNode(AccessibilityFocusStrategy::Forward(
                    AccessibilityFocusMovement::OutsideGroup,
                )));
                platform.send(UserEvent::FocusAccessibilityNode(AccessibilityFocusStrategy::Forward(
                    AccessibilityFocusMovement::InsideGroup,
                )));
            }
        }
        _ if ctrl && e.code == Code::KeyZ && !popover_open => {
            let mut model = ctx.model;
            if !model.write().undo() {
                println!("UNDO nothing to undo");
            }
        }
        _ => {}
    }
}

fn open_for_selection(ctx: Ctx, scene: &Rc<RefCell<Scene>>) {
    let Some(id) = ctx.selected.peek().clone() else { return };
    let v = *ctx.view.peek();
    if let Some((x, y, w, h)) = scene.borrow().screen_rect(v, &id) {
        ctx.open_menu(id, x + w * 0.3, y + h - 6.0, false);
    }
}

fn muted(text: impl Into<Cow<'static, str>>) -> Label {
    label().text(text.into()).color(MUTED)
}

fn separator() -> Element {
    rect().width(Size::fill_minimum()).height(Size::px(1.0)).margin((4.0, 0.0)).background(BORDER).into()
}

/// One menu row. Read-only rows are greyed and have no press handler (MenuItem has no `disabled`).
fn item(text: &str, enabled: bool, on_press: impl FnMut(Event<PressEventData>) + 'static) -> Element {
    let row = label().text(text.to_string()).color(if enabled { TEXT } else { MUTED });
    if enabled { MenuButton::new().on_press(on_press).child(row).into() } else { MenuButton::new().child(row).into() }
}

fn build_menu(ctx: Ctx, model: &Model, id: &str) -> Menu {
    let ro = model.read_only;
    let node = model.node(id).expect("node");
    let is_root = model.root().id == id;
    let mut menu = Menu::new().child(
        rect()
            .padding((4.0, 12.0, 6.0, 12.0))
            .font_size(12.0)
            .child(muted(format!("{} · {}{}", node.id, node.name, if ro { "  (read-only)" } else { "" }))),
    );
    if node.kind == Kind::Step {
        for f in &model.pack().factors {
            let current = model.level_of(id, &f.id).unwrap_or_default().to_string();
            let current_label = f.levels.iter().find(|l| l.id == current).map(|l| l.label.clone()).unwrap_or_default();
            let levels = f.levels.iter().map(|l| {
                let selected = l.id == current;
                let row = rect()
                    .horizontal()
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(label().text(if selected { "●" } else { "○" }))
                    .child(label().text(l.label.clone()).width(Size::px(140.0)).color(if ro { MUTED } else { TEXT }))
                    .child(muted(l.value.to_string()))
                    .on_press(|_| trace(|| "row press".into()))
                    .on_pointer_down(|_| trace(|| "row pointer down".into()))
                    .on_mouse_up(|_| trace(|| "row mouse up".into()));
                let (nid, fid, lid) = (id.to_string(), f.id.clone(), l.id.clone());
                let item = MenuItem::new().selected(selected).child(row);
                if ro {
                    item.into()
                } else {
                    item.on_press(move |_| ctx.apply(|m| m.set_factor(&nid, &fid, &lid))).into()
                }
            });
            menu = menu.child(
                SubMenu::new()
                    .label(
                        rect()
                            .horizontal()
                            .width(Size::px(280.0))
                            .child(label().text(f.label.clone()).width(Size::px(165.0)))
                            .child(muted(format!("{current_label}  ›"))),
                    )
                    .children(levels.collect::<Vec<Element>>()),
            );
        }
        let nid = id.to_string();
        menu = menu.child(separator()).child(item("Edit feasibility…", !ro, move |_| ctx.open_popover(&nid)));
    } else {
        let kinds = [Kind::And, Kind::Or].map(|k| {
            let nid = id.to_string();
            let row = rect()
                .horizontal()
                .cross_align(Alignment::center())
                .spacing(8.0)
                .child(RadioItem::new().selected(node.kind == k).size(14.0))
                .child(label().text(k.label()).color(if ro { MUTED } else { TEXT }));
            let item = MenuItem::new().selected(node.kind == k).child(row);
            if ro { item.into() } else { item.on_press(move |_| ctx.apply(|m| m.set_kind(&nid, k))).into() }
        });
        menu = menu.child(
            SubMenu::new()
                .label(
                    rect()
                        .horizontal()
                        .width(Size::px(200.0))
                        .child(label().text("Gate type").width(Size::px(110.0)))
                        .child(muted(format!("{}  ›", node.kind.label()))),
                )
                .children(kinds.into_iter().collect::<Vec<Element>>()),
        );
        let nid = id.to_string();
        menu = menu.child(item("Add child step", !ro, move |_| ctx.apply(|m| m.add_child_step(&nid).map(|_| ()))));
    }
    if !is_root {
        let nid = id.to_string();
        menu = menu.child(separator()).child(item("Delete", !ro, move |_| ctx.apply(|m| m.delete(&nid))));
    }
    menu
}

fn popover(ctx: Ctx, model: &Model, d: Draft, x: f32, y: f32) -> Element {
    let pack = model.pack();
    let (ap, ri) = model.preview(&d.levels);
    let rating = &pack.ratings[ri];
    let [r, g, b] = rating.rgb();
    let name = model.node(&d.node).map(|n| n.name.clone()).unwrap_or_default();
    let rows = pack.factors.iter().map(|f| {
        let cur = d.levels.get(&f.id).cloned().unwrap_or_default();
        let cur_text = f.levels.iter().find(|l| l.id == cur).map(|l| format!("{} ({})", l.label, l.value)).unwrap_or("—".into());
        let items = f.levels.iter().map(|l| {
            let (fid, lid) = (f.id.clone(), l.id.clone());
            let mut draft = ctx.draft;
            MenuItem::new()
                .selected(l.id == cur)
                .on_press(move |_| {
                    if let Some(d) = draft.write().as_mut() {
                        d.levels.insert(fid.clone(), lid.clone());
                    }
                })
                .child(format!("{} ({})", l.label, l.value))
                .into()
        });
        rect()
            .horizontal()
            .width(Size::fill())
            .cross_align(Alignment::center())
            .child(label().text(f.label.clone()).width(Size::px(150.0)))
            .child(Select::new().selected_item(cur_text).children(items.collect::<Vec<Element>>()))
            .into()
    });
    let node_id = d.node.clone();
    let levels = d.levels.clone();
    let mut draft = ctx.draft;
    rect()
        .layer(Layer::Overlay)
        .position(Position::new_global().left(x).top(y))
        .width(Size::px(POP_W))
        .padding(14.0)
        .spacing(8.0)
        .background(Color::WHITE)
        .corner_radius(10.0)
        .border(Border::new().width(1.0).fill(BORDER))
        .shadow((0.0, 6.0, 18.0, 0.0, (0, 0, 0, 40)))
        .child(label().text(format!("Edit feasibility: {name}")).font_size(15.0).font_weight(FontWeight::BOLD))
        .children(rows.collect::<Vec<Element>>())
        .child(label().text("Rationale"))
        .child(RationaleEditor { value: ctx.rationale })
        .child(
            rect()
                .horizontal()
                .cross_align(Alignment::center())
                .spacing(8.0)
                .child(muted("Preview"))
                .child(
                    rect()
                        .padding((3.0, 10.0))
                        .corner_radius(99.0)
                        .background((r, g, b))
                        .color(Color::WHITE)
                        .font_weight(FontWeight::BOLD)
                        .child(format!("{ap} · {}", rating.label)),
                ),
        )
        .child(
            rect()
                .horizontal()
                .width(Size::fill())
                .main_align(Alignment::end())
                .spacing(8.0)
                .child(Button::new().on_press(move |_| draft.set(None)).child("Cancel"))
                .child(Button::new().filled().on_press(move |_| {
                    let text = ctx.rationale.peek().clone();
                    ctx.apply(|m| m.set_factors(&node_id, levels.clone(), text));
                    draft.set(None);
                }).child("OK")),
        )
        .into()
}

/// Multi-line text field. Freya's `Input` is single-line (max_lines(1), Enter submits),
/// so this wires `use_editable` to a `paragraph` like `Input` does internally.
#[derive(PartialEq)]
struct RationaleEditor {
    value: State<String>,
}

impl Component for RationaleEditor {
    fn render(&self) -> impl IntoElement {
        let mut value = self.value;
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let holder = use_state(ParagraphHolder::default);
        let mut editable = use_editable(|| value.peek().clone(), EditableConfig::new);
        let mut dragging = use_state(|| false);

        let on_key_down = move |e: Event<KeyboardEventData>| {
            if matches!(e.key, Key::Named(NamedKey::Escape | NamedKey::Tab)) {
                return;
            }
            e.stop_propagation();
            editable.process_event(EditableEvent::KeyDown { key: &e.key, modifiers: e.modifiers });
            let text = editable.editor().peek().committed_text();
            value.set(text);
        };
        let on_key_up = move |e: Event<KeyboardEventData>| {
            editable.process_event(EditableEvent::KeyUp { key: &e.key });
        };
        let on_down = move |e: Event<MouseEventData>| {
            a11y_id.request_focus();
            dragging.set(true);
            editable.process_event(EditableEvent::Down {
                location: e.element_location,
                editor_line: EditorLine::SingleParagraph,
                holder: &holder.read(),
            });
        };
        let on_move = move |e: Event<MouseEventData>| {
            if *dragging.peek() {
                editable.process_event(EditableEvent::Move {
                    location: e.element_location,
                    editor_line: EditorLine::SingleParagraph,
                    holder: &holder.read(),
                });
            }
        };
        let on_up = move |_: Event<MouseEventData>| {
            dragging.set(false);
            editable.process_event(EditableEvent::Release);
        };

        let editor = editable.editor().read();
        let focused = focus().is_focused();
        let cursor = focused.then(|| editor.cursor_pos());
        let selection = if focused { editor.get_visible_selection(EditorLine::SingleParagraph) } else { None };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::MultilineTextInput)
            .width(Size::fill())
            .height(Size::px(96.0))
            .padding(8.0)
            .corner_radius(6.0)
            .background(if focused { Color::WHITE } else { Color::from_rgb(0xFA, 0xFB, 0xFC) })
            .border(Border::new().width(if focused { 2.0 } else { 1.0 }).fill(if focused { ACCENT } else { BORDER }))
            .on_key_down(on_key_down)
            .on_key_up(on_key_up)
            .child(
                paragraph()
                    .holder(holder.read().clone())
                    .width(Size::fill())
                    .cursor_index(cursor)
                    .cursor_color(TEXT)
                    .highlights(selection.map(|h| vec![h]))
                    .on_mouse_down(on_down)
                    .on_mouse_move(on_move)
                    .on_mouse_up(on_up)
                    .span(editor.rope().to_string()),
            )
    }
}

// ---------- Skia drawing ----------

fn sk_rgb((r, g, b): (u8, u8, u8)) -> sk::SkColor {
    sk::SkColor::from_rgb(r, g, b)
}

fn text(c: &CanvasContext, s: &str, size: f32, color: sk::SkColor, bold: bool, max_lines: usize, width: f32) -> sk::SkParagraph {
    let mut ts = sk::TextStyle::new();
    // An empty family list trips a debug UB check in skia-safe, so always pass a fallback.
    let mut families: Vec<&str> = c.text_style_state.font_families.iter().map(|f| f.as_ref()).collect();
    families.extend(["Noto Sans", "DejaVu Sans", "sans-serif"]);
    ts.set_font_size(size).set_color(color).set_font_families(&families);
    if bold {
        ts.set_font_style(sk::FontStyle::bold());
    }
    let mut ps = sk::ParagraphStyle::new();
    ps.set_text_style(&ts).set_max_lines(max_lines).set_ellipsis("…");
    let mut b = sk::ParagraphBuilder::new(&ps, c.font_collection.clone());
    b.add_text(s);
    let mut p = b.build();
    p.layout(width);
    p
}

/// Draws visible edges and nodes. Returns the number of nodes drawn.
fn draw(c: &mut CanvasContext, scene: &Scene, v: View, selected: Option<&str>) -> usize {
    let canvas = c.canvas;
    canvas.save();
    canvas.translate((v.pan_x, v.pan_y));
    canvas.scale((v.zoom, v.zoom));
    // Visible world rectangle, for culling.
    let (vx0, vy0) = (-v.pan_x / v.zoom, -v.pan_y / v.zoom);
    let (vx1, vy1) = (vx0 + c.size.width / v.zoom, vy0 + c.size.height / v.zoom);
    let visible = |x0: f32, y0: f32, x1: f32, y1: f32| x1 >= vx0 && x0 <= vx1 && y1 >= vy0 && y0 <= vy1;

    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Stroke).set_stroke_width(1.5).set_color(sk_rgb(EDGE));
    for (i, p) in scene.placed.iter().enumerate() {
        let Some(pi) = scene.parents[i] else { continue };
        let q = &scene.placed[pi];
        let (x0, y0, x1, y1) = (q.x + q.w, q.y + q.h / 2.0, p.x, p.y + p.h / 2.0);
        if !visible(x0, y0.min(y1), x1, y0.max(y1)) {
            continue;
        }
        let mx = (x0 + x1) / 2.0;
        let mut pb = sk::PathBuilder::new();
        pb.move_to((x0, y0)).cubic_to((mx, y0), (mx, y1), (x1, y1));
        canvas.draw_path(&pb.detach(), &paint);
    }

    let mut drawn = 0;
    for (i, p) in scene.placed.iter().enumerate() {
        if !visible(p.x, p.y, p.x + p.w, p.y + p.h) {
            continue;
        }
        drawn += 1;
        let r = sk::SkRRect::new_rect_xy(sk::SkRect::from_xywh(p.x, p.y, p.w, p.h), 8.0, 8.0);
        paint.set_style(sk::PaintStyle::Fill).set_color(sk::SkColor::WHITE);
        canvas.draw_rrect(r, &paint);
        let is_sel = selected == Some(p.id.as_str());
        let bw = if is_sel { 2.0 } else { 1.0 };
        paint.set_style(sk::PaintStyle::Stroke).set_stroke_width(bw).set_color(sk_rgb(if is_sel { ACCENT } else { BORDER }));
        canvas.draw_rrect(r.with_inset((bw / 2.0, bw / 2.0)), &paint);

        let chip_w = if p.kind.is_gate() { 44.0 } else { 0.0 };
        let title = text(c, &p.name, 14.0, sk_rgb(TEXT), false, 2, p.w - 24.0 - chip_w);
        title.paint(canvas, (p.x + 12.0, p.y + 8.0));
        if p.kind.is_gate() {
            let chip = sk::SkRRect::new_rect_xy(sk::SkRect::from_xywh(p.x + p.w - 50.0, p.y + 9.0, 38.0, 18.0), 9.0, 9.0);
            paint.set_style(sk::PaintStyle::Fill).set_color(sk::SkColor::from_rgb(0xEE, 0xF1, 0xF6));
            canvas.draw_rrect(chip, &paint);
            let t = text(c, p.kind.label(), 10.0, sk_rgb((0x4A, 0x52, 0x60)), true, 1, 100.0);
            t.paint(canvas, (p.x + p.w - 31.0 - t.longest_line() / 2.0, p.y + 18.0 - t.height() / 2.0));
        }
        let (badge, [br, bg, bb]) = &scene.badges[i];
        let t = text(c, badge, 12.0, sk::SkColor::WHITE, true, 1, 400.0);
        let pill = sk::SkRRect::new_rect_xy(sk::SkRect::from_xywh(p.x + 12.0, p.y + p.h - 28.0, t.longest_line() + 18.0, 20.0), 10.0, 10.0);
        paint.set_style(sk::PaintStyle::Fill).set_color(sk::SkColor::from_rgb(*br, *bg, *bb));
        canvas.draw_rrect(pill, &paint);
        t.paint(canvas, (p.x + 21.0, p.y + p.h - 18.0 - t.height() / 2.0));
    }
    canvas.restore();
    drawn
}
