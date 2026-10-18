//! AegisTARA UI toolkit spike with iced 0.14 (see ../SPEC.md).
//!
//! - The attack tree is drawn on a built-in `canvas` (custom drawing, pan, zoom, hit-testing, culling).
//! - iced has no context menu, submenu or popover widget. The menu and the popover are extra layers
//!   in a built-in `stack`, placed with `pin` and made clickable with `mouse_area` and `opaque`.
//! - The popover content is built-in: `pick_list` per factor, `text_editor`, `button`.
//! - Every factor, level, label, value and rating color comes from the pack at runtime.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::OnceLock;
use std::time::Instant;

use iced::alignment::Vertical;
use iced::keyboard::{self, Key, key::Named};
use iced::mouse;
use iced::widget::canvas::{self, Cache, Frame, Geometry, Path, Stroke};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, mouse_area, opaque, pick_list, pin, responsive, row, space, stack, text, text_editor,
};
use iced::{
    Border, Center, Color, Element, Event, Fill, Font, Point, Rectangle, Renderer, Shadow, Size, Subscription, Task,
    Theme, Vector, event, window,
};
use spike_model::{Kind, Model, Placed, WINDOW_H, WINDOW_W};

static START: OnceLock<Instant> = OnceLock::new();

// Shared visual style from the SPEC.
const BG: u32 = 0xF6F7F9;
const NODE_BORDER: u32 = 0xD0D4DC;
const EDGE: u32 = 0x9AA3B2;
const SELECTED: u32 = 0x3D6DE8;
// Spike-local UI colors (not data).
const INK: u32 = 0x1F2430;
const MUTED: u32 = 0x6B7280;
const DISABLED: u32 = 0xA9AFBA;
const HILITE: u32 = 0xE6EDFD;
const CHIP: u32 = 0xEEF1F6;

const STATUS_H: f32 = 24.0;
const MENU_W: f32 = 340.0;
const SUB_W: f32 = 230.0;
const ITEM_H: f32 = 28.0;
const HEADER_H: f32 = 26.0;
const SEP_H: f32 = 9.0;
const PAD: f32 = 4.0;
const POP_W: f32 = 400.0;
const POP_H: f32 = 416.0; // measured from the screenshot

fn hex(c: u32) -> Color {
    Color::from_rgb8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn rating_color(m: &Model, idx: usize) -> Color {
    let [r, g, b] = m.pack().ratings[idx].rgb();
    Color::from_rgb8(r, g, b)
}

/// Commands that change the model or open the editor. Menu items carry them.
#[derive(Debug, Clone)]
enum Cmd {
    SetFactor(String, String, String),
    SetKind(String, Kind),
    AddChild(String),
    Delete(String),
    Edit(String),
}

#[derive(Debug, Clone)]
enum Message {
    Select(Option<String>),
    Pan(Vector),
    PanEnd,
    Zoom(f32, Point),
    OpenMenu(String, Point),
    CloseMenu,
    HoverItem(usize),
    HoverSub(usize),
    ClickItem(usize),
    Run(Cmd),
    PopLevel(usize, usize),
    PopText(text_editor::Action),
    PopOk,
    PopCancel,
    Key(keyboard::Event),
    MouseMoved,
    WindowOpened(window::Id),
    XWindow(u64),
    Renderer(String),
}

/// One menu row, generated from the model and the pack each time the menu is drawn.
struct Item {
    label: String,
    detail: String,
    enabled: bool,
    sep_before: bool,
    action: Action,
}

enum Action {
    Sub(Vec<SubItem>),
    Cmd(Cmd),
}

struct SubItem {
    label: String,
    detail: String,
    checked: bool,
    enabled: bool,
    cmd: Cmd,
}

struct Menu {
    node: String,
    at: Point,
    focus: Option<usize>,
    sub: Option<usize>,
    /// Some while the keyboard is inside the open submenu.
    sub_focus: Option<usize>,
    /// Opened by keyboard and the mouse has not moved since: ignore hover from a resting cursor.
    kb_hold: bool,
}

struct Popover {
    node: String,
    levels: BTreeMap<String, String>,
    rationale: text_editor::Content,
}

/// A pick_list option: one level of one factor, labelled from the pack.
#[derive(Debug, Clone, PartialEq)]
struct LevelOpt {
    factor: usize,
    level: usize,
    text: String,
}

impl fmt::Display for LevelOpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

struct App {
    model: Model,
    layout: Vec<Placed>,
    index: HashMap<String, usize>,
    generation: u64,
    cache: Cache,
    zoom: f32,
    pan: Vector,
    selected: Option<String>,
    menu: Option<Menu>,
    popover: Option<Popover>,
    options: Vec<Vec<LevelOpt>>,
    renderer: String,
    widths: RefCell<HashMap<(String, u32), f32>>,
    first_draw: Cell<bool>,
}

impl App {
    fn new() -> Self {
        let model = Model::load_default().unwrap_or_else(|e| panic!("cannot load spike data: {e}"));
        model.log_layout(0.0, 0.0);
        model.log_results();
        let options = model
            .pack()
            .factors
            .iter()
            .enumerate()
            .map(|(fi, f)| {
                let opts = f.levels.iter().enumerate();
                opts.map(|(li, l)| LevelOpt { factor: fi, level: li, text: format!("{} ({})", l.label, l.value) })
                    .collect()
            })
            .collect();
        let mut app = App {
            model,
            layout: Vec::new(),
            index: HashMap::new(),
            generation: u64::MAX,
            cache: Cache::new(),
            zoom: 1.0,
            pan: Vector::ZERO,
            selected: None,
            menu: None,
            popover: None,
            options,
            renderer: String::new(),
            widths: RefCell::new(HashMap::new()),
            first_draw: Cell::new(true),
        };
        app.sync_layout();
        if app.model.read_only {
            println!("READONLY on");
        }
        app
    }

    fn sync_layout(&mut self) {
        if self.generation == self.model.generation() {
            return;
        }
        self.generation = self.model.generation();
        self.layout = self.model.layout();
        self.index = self.layout.iter().enumerate().map(|(i, p)| (p.id.clone(), i)).collect();
        if self.selected.as_ref().is_some_and(|s| !self.index.contains_key(s)) {
            self.selected = None;
        }
        if self.popover.as_ref().is_some_and(|p| !self.index.contains_key(&p.node)) {
            self.popover = None;
        }
    }

    fn to_world(&self, p: Point) -> Point {
        Point::new((p.x - self.pan.x) / self.zoom, (p.y - self.pan.y) / self.zoom)
    }

    fn hit(&self, p: Point) -> Option<String> {
        let w = self.to_world(p);
        self.layout.iter().rev().find(|n| n.contains(w.x, w.y)).map(|n| n.id.clone())
    }

    fn screen_rect(&self, id: &str) -> Option<Rectangle> {
        let n = &self.layout[*self.index.get(id)?];
        let pos = Point::new(n.x * self.zoom + self.pan.x, n.y * self.zoom + self.pan.y);
        Some(Rectangle::new(pos, Size::new(n.w * self.zoom, n.h * self.zoom)))
    }

    /// Width of a single line of canvas text, cached (the canvas API has no measuring call).
    fn text_width(&self, s: &str, size: f32) -> f32 {
        use iced_core::text::Paragraph as _;
        let key = (s.to_string(), size.to_bits());
        if let Some(w) = self.widths.borrow().get(&key) {
            return *w;
        }
        let paragraph = iced_graphics::text::Paragraph::with_text(iced_core::text::Text {
            content: s,
            bounds: Size::INFINITE,
            size: size.into(),
            line_height: Default::default(),
            font: Font::DEFAULT,
            align_x: Default::default(),
            align_y: Vertical::Top,
            shaping: Default::default(),
            wrapping: Wrapping::None,
        });
        let w = paragraph.min_width();
        self.widths.borrow_mut().insert(key, w);
        w
    }

    // ----- menu model (data-driven) -----

    fn menu_items(&self, id: &str) -> Vec<Item> {
        let ro = self.model.read_only;
        let Some(node) = self.model.node(id) else { return Vec::new() };
        let mut items = Vec::new();
        if node.kind == Kind::Step {
            for f in &self.model.pack().factors {
                let cur = self.model.level_of(id, &f.id);
                let subs = f
                    .levels
                    .iter()
                    .map(|l| SubItem {
                        label: l.label.clone(),
                        detail: l.value.to_string(),
                        checked: cur == Some(l.id.as_str()),
                        enabled: !ro,
                        cmd: Cmd::SetFactor(id.into(), f.id.clone(), l.id.clone()),
                    })
                    .collect();
                let detail = f.levels.iter().find(|l| cur == Some(l.id.as_str())).map(|l| l.label.clone());
                items.push(Item {
                    label: f.label.clone(),
                    detail: detail.unwrap_or_default(),
                    enabled: true,
                    sep_before: false,
                    action: Action::Sub(subs),
                });
            }
            items.push(Item {
                label: "Edit feasibility…".into(),
                detail: String::new(),
                enabled: !ro,
                sep_before: true,
                action: Action::Cmd(Cmd::Edit(id.into())),
            });
        } else {
            let subs = [Kind::And, Kind::Or]
                .into_iter()
                .map(|k| SubItem {
                    label: k.label().into(),
                    detail: String::new(),
                    checked: node.kind == k,
                    enabled: !ro,
                    cmd: Cmd::SetKind(id.into(), k),
                })
                .collect();
            items.push(Item {
                label: "Gate type".into(),
                detail: node.kind.label().into(),
                enabled: true,
                sep_before: false,
                action: Action::Sub(subs),
            });
            items.push(Item {
                label: "Add child step".into(),
                detail: String::new(),
                enabled: !ro,
                sep_before: false,
                action: Action::Cmd(Cmd::AddChild(id.into())),
            });
        }
        if id != self.model.root().id {
            items.push(Item {
                label: "Delete".into(),
                detail: String::new(),
                enabled: !ro,
                sep_before: node.kind.is_gate(),
                action: Action::Cmd(Cmd::Delete(id.into())),
            });
        }
        items
    }

    // ----- update -----

    fn update(&mut self, msg: Message) -> Task<Message> {
        let before = (self.model.generation(), self.zoom, self.pan, self.selected.clone());
        match msg {
            Message::Select(id) => {
                if let Some(id) = &id {
                    println!("SELECT {id}");
                }
                self.selected = id;
            }
            Message::Pan(d) => {
                self.menu = None;
                self.pan = self.pan + d;
            }
            Message::PanEnd => println!("PAN {:.0} {:.0}", self.pan.x, self.pan.y),
            Message::Zoom(steps, p) => {
                self.menu = None;
                let world = self.to_world(p);
                self.zoom = (self.zoom * 1.1f32.powf(steps)).clamp(0.1, 4.0);
                self.pan = Vector::new(p.x - world.x * self.zoom, p.y - world.y * self.zoom);
                println!("ZOOM {:.3} pan {:.0} {:.0}", self.zoom, self.pan.x, self.pan.y);
            }
            Message::OpenMenu(id, at) => self.open_menu(id, at, false),
            Message::CloseMenu => self.menu = None,
            Message::HoverItem(i) => {
                let has_sub = self
                    .menu
                    .as_ref()
                    .map(|m| self.menu_items(&m.node))
                    .is_some_and(|items| items.get(i).is_some_and(|it| matches!(it.action, Action::Sub(_))));
                if let Some(m) = self.menu.as_mut().filter(|m| !m.kb_hold) {
                    if m.focus != Some(i) || m.sub_focus.is_some() {
                        m.focus = Some(i);
                        m.sub = has_sub.then_some(i);
                        m.sub_focus = None;
                    }
                }
            }
            Message::HoverSub(j) => {
                if let Some(m) = self.menu.as_mut().filter(|m| !m.kb_hold) {
                    m.sub_focus = Some(j);
                }
            }
            Message::MouseMoved => {
                if let Some(m) = &mut self.menu {
                    m.kb_hold = false;
                }
            }
            Message::ClickItem(i) => self.activate(i, false),
            Message::Run(cmd) => self.run(cmd),
            Message::PopLevel(fi, li) => {
                if let Some(p) = &mut self.popover {
                    let f = &self.model.pack().factors[fi];
                    p.levels.insert(f.id.clone(), f.levels[li].id.clone());
                    let (ap, r) = self.model.preview(&p.levels);
                    println!("PREVIEW {} {} {}", p.node, ap, self.model.pack().ratings[r].label);
                }
            }
            Message::PopText(action) => {
                if let Some(p) = &mut self.popover {
                    p.rationale.perform(action);
                }
            }
            Message::PopOk => {
                if let Some(p) = self.popover.take() {
                    let text = p.rationale.text();
                    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
                    println!("POPOVER_OK {} rationale={text:?}", p.node);
                    if let Err(e) = self.model.set_factors(&p.node, p.levels, text) {
                        println!("REJECTED {e}");
                    }
                }
            }
            Message::PopCancel => {
                self.popover = None;
                println!("POPOVER_CANCEL");
            }
            Message::Key(ev) => self.on_key(ev),
            Message::WindowOpened(id) => {
                let info = iced::system::information();
                let renderer =
                    info.map(|i| Message::Renderer(format!("{} ({})", i.graphics_backend, i.graphics_adapter)));
                return Task::batch([window::raw_id::<Message>(id).map(Message::XWindow), renderer]);
            }
            Message::Renderer(r) => {
                println!("RENDERER {r}");
                self.renderer = r;
            }
            Message::XWindow(_raw) => {
                #[cfg(target_os = "linux")]
                focus_without_wm(_raw);
            }
        }
        self.sync_layout();
        if before != (self.model.generation(), self.zoom, self.pan, self.selected.clone()) {
            self.cache.clear();
        }
        Task::none()
    }

    fn open_menu(&mut self, id: String, at: Point, keyboard: bool) {
        self.popover = None;
        println!("{} {id} {:.0} {:.0}", if keyboard { "KBMENU_OPEN" } else { "MENU_OPEN" }, at.x, at.y);
        self.selected = Some(id.clone());
        let focus = keyboard.then_some(0);
        self.menu = Some(Menu { node: id, at, focus, sub: None, sub_focus: None, kb_hold: keyboard });
    }

    /// Activates main item `i`: opens its submenu or runs its command.
    fn activate(&mut self, i: usize, keyboard: bool) {
        let Some(m) = &self.menu else { return };
        let items = self.menu_items(&m.node);
        let Some(item) = items.get(i) else { return };
        match &item.action {
            Action::Sub(subs) => {
                let checked = subs.iter().position(|s| s.checked).unwrap_or(0);
                let m = self.menu.as_mut().unwrap();
                m.focus = Some(i);
                m.sub = Some(i);
                m.sub_focus = keyboard.then_some(checked);
            }
            Action::Cmd(cmd) if item.enabled => self.run(cmd.clone()),
            Action::Cmd(_) => {}
        }
    }

    fn run(&mut self, cmd: Cmd) {
        self.menu = None;
        let result = match cmd {
            Cmd::SetFactor(n, f, l) => self.model.set_factor(&n, &f, &l),
            Cmd::SetKind(n, k) => self.model.set_kind(&n, k),
            Cmd::AddChild(n) => self.model.add_child_step(&n).map(|id| self.selected = Some(id)),
            Cmd::Delete(n) => self.model.delete(&n),
            Cmd::Edit(n) => {
                let Some(node) = self.model.node(&n) else { return };
                let rationale = text_editor::Content::with_text(&node.rationale);
                self.popover = Some(Popover { node: n.clone(), levels: node.factors.clone(), rationale });
                println!("POPOVER_OPEN {n}");
                Ok(())
            }
        };
        if let Err(e) = result {
            println!("REJECTED {e}");
        }
    }

    fn on_key(&mut self, ev: keyboard::Event) {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = ev else { return };
        match key.as_ref() {
            Key::Named(Named::F10) if modifiers.shift() => self.open_menu_kb(),
            Key::Named(Named::ContextMenu) => self.open_menu_kb(),
            Key::Character(c) if modifiers.command() && c.eq_ignore_ascii_case("z") => {
                // While the editor is open, Ctrl+Z must not undo the model behind it.
                if self.popover.is_none() {
                    self.menu = None;
                    if !self.model.undo() {
                        println!("UNDO nothing");
                    }
                }
            }
            Key::Named(named) if self.menu.is_some() => self.menu_key(named),
            Key::Named(Named::Escape) if self.popover.is_some() => {
                self.popover = None;
                println!("POPOVER_CANCEL");
            }
            _ => {}
        }
    }

    fn open_menu_kb(&mut self) {
        let Some(id) = self.selected.clone() else { return };
        let Some(r) = self.screen_rect(&id) else { return };
        self.open_menu(id, Point::new(r.x + 24.0, r.y + r.height - 4.0), true);
    }

    fn menu_key(&mut self, key: Named) {
        let Some(m) = &self.menu else { return };
        let items = self.menu_items(&m.node);
        if items.is_empty() {
            return;
        }
        let step = |i: usize, len: usize, down: bool| if down { (i + 1) % len } else { (i + len - 1) % len };
        let in_sub = m.sub.zip(m.sub_focus).and_then(|(si, sf)| match &items[si].action {
            Action::Sub(subs) => Some((subs, sf)),
            Action::Cmd(_) => None,
        });
        let m = self.menu.as_mut().unwrap();
        match (key, in_sub) {
            (Named::ArrowDown | Named::ArrowUp, Some((subs, sf))) => {
                m.sub_focus = Some(step(sf, subs.len(), key == Named::ArrowDown));
            }
            (Named::ArrowDown | Named::ArrowUp, None) => {
                let down = key == Named::ArrowDown;
                m.focus = Some(m.focus.map_or(if down { 0 } else { items.len() - 1 }, |i| step(i, items.len(), down)));
                m.sub = None;
            }
            (Named::Enter | Named::Space, Some((subs, sf))) => {
                if subs[sf].enabled {
                    let cmd = subs[sf].cmd.clone();
                    self.run(cmd);
                }
            }
            (Named::ArrowRight | Named::Enter | Named::Space, None) => {
                if let Some(i) = m.focus {
                    let is_sub = matches!(items[i].action, Action::Sub(_));
                    if is_sub || key != Named::ArrowRight {
                        self.activate(i, true);
                    }
                }
            }
            (Named::ArrowLeft | Named::Escape, Some(_)) => {
                m.sub = None;
                m.sub_focus = None;
            }
            (Named::Escape, None) => self.menu = None,
            _ => {}
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let keys = Subscription::batch([
            keyboard::listen().map(Message::Key),
            window::open_events().map(Message::WindowOpened),
        ]);
        let moves = event::listen_with(|e, _, _| match e {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => Some(Message::MouseMoved),
            _ => None,
        });
        if self.menu.as_ref().is_some_and(|m| m.kb_hold) { Subscription::batch([keys, moves]) } else { keys }
    }

    // ----- view -----

    fn view(&self) -> Element<'_, Message> {
        column![responsive(move |size| self.layers(size)), self.status_bar()].into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let sel = self.selected.as_ref().and_then(|id| self.model.node(id));
        let mut parts = vec![sel.map_or("No selection".into(), |n| format!("{} · {}", n.id, n.name))];
        parts.push(format!("Zoom {:.0}%", self.zoom * 100.0));
        parts.push(format!("{} nodes", self.layout.len()));
        if self.model.read_only {
            parts.push("READ-ONLY".into());
        }
        parts.push("Right-click or Shift+F10: menu · Ctrl+Z: undo".into());
        parts.push(format!("Renderer: {}", self.renderer));
        container(text(parts.join("   |   ")).size(12).color(hex(MUTED)).wrapping(Wrapping::None))
            .padding([0, 12])
            .height(STATUS_H)
            .width(Fill)
            .align_y(Center)
            .clip(true)
            .style(|_| container::Style {
                background: Some(Color::WHITE.into()),
                border: Border { width: 1.0, color: hex(NODE_BORDER), radius: 0.0.into() },
                ..Default::default()
            })
            .into()
    }

    fn layers(&self, size: Size) -> Element<'_, Message> {
        let canvas = canvas::Canvas::new(TreeCanvas { app: self }).width(Fill).height(Fill);
        let mut layers: Vec<Element<'_, Message>> = vec![canvas.into()];
        if let Some(m) = &self.menu {
            self.menu_layers(m, size, &mut layers);
        }
        if let Some(p) = &self.popover {
            if let Some(layer) = self.popover_layer(p, size) {
                layers.push(layer);
            }
        }
        stack(layers).width(Fill).height(Fill).into()
    }

    fn menu_layers<'a>(&'a self, m: &'a Menu, size: Size, layers: &mut Vec<Element<'a, Message>>) {
        let items = self.menu_items(&m.node);
        let mut tops = Vec::with_capacity(items.len());
        let mut y = PAD + HEADER_H;
        for it in &items {
            y += if it.sep_before { SEP_H } else { 0.0 };
            tops.push(y);
            y += ITEM_H;
        }
        let h = y + PAD;
        // Flip left/up when the menu would leave the window, like native menus do.
        let x0 = if m.at.x + MENU_W > size.width { (m.at.x - MENU_W).max(0.0) } else { m.at.x };
        let y0 = if m.at.y + h > size.height { (m.at.y - h).max(0.0) } else { m.at.y };

        let name = self.model.node(&m.node).map(|n| format!("{} · {}", n.id, n.name)).unwrap_or_default();
        let header = container(text(name).size(12).color(hex(MUTED)).wrapping(Wrapping::None))
            .padding([0, 8])
            .height(HEADER_H)
            .width(Fill)
            .align_y(Center)
            .clip(true);
        let mut col = column![header];
        for (i, it) in items.iter().enumerate() {
            if it.sep_before {
                col = col.push(separator());
            }
            let open = m.sub == Some(i);
            let hl = m.focus == Some(i) || open;
            let is_sub = matches!(it.action, Action::Sub(_));
            let click = (it.enabled || is_sub).then_some(Message::ClickItem(i));
            col = col.push(menu_row("", &it.label, &it.detail, is_sub, hl, it.enabled, Message::HoverItem(i), click));
        }
        layers.push(
            mouse_area(space().width(Fill).height(Fill))
                .on_press(Message::CloseMenu)
                .on_right_press(Message::CloseMenu)
                .into(),
        );
        layers.push(pin(opaque(panel(col, MENU_W, PAD))).x(x0).y(y0).into());

        let Some(si) = m.sub else { return };
        let Some(Action::Sub(subs)) = items.get(si).map(|it| &it.action) else { return };
        let sh = 2.0 * PAD + subs.len() as f32 * ITEM_H;
        let sx = if x0 + MENU_W + SUB_W - 4.0 > size.width { (x0 - SUB_W + 4.0).max(0.0) } else { x0 + MENU_W - 4.0 };
        let sy = (y0 + tops[si] - PAD).min(size.height - sh).max(0.0);
        let rows = subs.iter().enumerate().map(|(j, s)| {
            let marker = if s.checked { "●" } else { "" };
            let click = s.enabled.then(|| Message::Run(s.cmd.clone()));
            menu_row(marker, &s.label, &s.detail, false, m.sub_focus == Some(j), s.enabled, Message::HoverSub(j), click)
        });
        layers.push(pin(opaque(panel(column(rows), SUB_W, PAD))).x(sx).y(sy).into());
    }

    fn popover_layer<'a>(&'a self, p: &'a Popover, size: Size) -> Option<Element<'a, Message>> {
        let r = self.screen_rect(&p.node)?;
        let node = self.model.node(&p.node)?;
        let pack = self.model.pack();
        let (ap, ri) = self.model.preview(&p.levels);
        let mut col = column![
            text("Edit attack feasibility").size(12).color(hex(MUTED)),
            text(format!("{} · {}", node.id, node.name)).size(15).color(hex(INK)),
        ]
        .spacing(8);
        for (fi, f) in pack.factors.iter().enumerate() {
            let cur = p.levels.get(&f.id).and_then(|lid| f.levels.iter().position(|l| &l.id == lid));
            let opts = &self.options[fi];
            let pick = pick_list(&opts[..], cur.map(|i| &opts[i]), |o: LevelOpt| Message::PopLevel(o.factor, o.level))
                .text_size(13)
                .padding([4, 8])
                .width(Fill);
            col = col.push(row![text(&f.label).size(13).width(150), pick].spacing(8).align_y(Center));
        }
        let editor =
            text_editor(&p.rationale).placeholder("Why these values?").size(13).height(84).on_action(Message::PopText);
        let preview = row![
            text("Preview").size(13).color(hex(MUTED)),
            pill(format!("{ap} · {}", pack.ratings[ri].label), rating_color(&self.model, ri)),
        ]
        .spacing(8)
        .align_y(Center);
        let buttons = row![
            space().width(Fill),
            button(text("Cancel").size(13)).style(button::secondary).on_press(Message::PopCancel),
            button(text("OK").size(13))
                .style(button::primary)
                .on_press_maybe((!self.model.read_only).then_some(Message::PopOk)),
        ]
        .spacing(8);
        col = col.push(text("Rationale").size(13)).push(editor).push(preview).push(buttons);

        // Anchor right of the node; flip to the left side when there is no room; keep inside the window.
        let gap = 10.0;
        let x = if r.x + r.width + gap + POP_W <= size.width { r.x + r.width + gap } else { r.x - gap - POP_W };
        let x = x.clamp(4.0, (size.width - POP_W - 4.0).max(4.0));
        let y = r.y.clamp(4.0, (size.height - POP_H - 4.0).max(4.0));
        Some(pin(opaque(panel(col, POP_W, 14.0))).x(x).y(y).into())
    }

    // ----- canvas drawing -----

    /// Draws in screen space. `Frame::scale` is avoided on purpose: in iced 0.14 the wgpu renderer
    /// does not scale `Text::max_width` under a scale transform but tiny-skia does, so wrapping differed.
    fn paint(&self, frame: &mut Frame, size: Size) {
        let t0 = Instant::now();
        let (z, pan) = (self.zoom, self.pan);
        let at = |x: f32, y: f32| Point::new(x * z + pan.x, y * z + pan.y);
        let rounded = |x: f32, y: f32, w: f32, h: f32, r: f32| {
            Path::rounded_rectangle(at(x, y), Size::new(w * z, h * z), (r * z).into())
        };
        let label = |content: String, position: Point, px: f32, color: Color, centered: bool| canvas::Text {
            content,
            position,
            color,
            size: (px * z).into(),
            align_y: if centered { Vertical::Center } else { Vertical::Top },
            ..canvas::Text::default()
        };
        let view = Rectangle::new(Point::ORIGIN, size);
        frame.fill_rectangle(Point::ORIGIN, size, hex(BG));
        let edges = Path::new(|b| {
            for n in &self.layout {
                let Some(&pi) = n.parent.as_ref().and_then(|id| self.index.get(id)) else { continue };
                let parent = &self.layout[pi];
                let (a, c) = (at(parent.x + parent.w, parent.y + parent.h / 2.0), at(n.x, n.y + n.h / 2.0));
                let span =
                    Rectangle::new(Point::new(a.x, a.y.min(c.y) - 1.0), Size::new(c.x - a.x, (c.y - a.y).abs() + 2.0));
                if span.intersects(&view) {
                    let mx = (a.x + c.x) / 2.0;
                    b.move_to(a);
                    b.bezier_curve_to(Point::new(mx, a.y), Point::new(mx, c.y), c);
                }
            }
        });
        frame.stroke(&edges, Stroke::default().with_width(1.5).with_color(hex(EDGE)));

        let detailed = z >= 0.35; // level of detail: no text when zoomed far out
        let mut drawn = 0;
        for n in &self.layout {
            if !Rectangle::new(at(n.x, n.y), Size::new(n.w * z, n.h * z)).intersects(&view) {
                continue; // culling
            }
            drawn += 1;
            let body = rounded(n.x, n.y, n.w, n.h, 8.0);
            frame.fill(&body, Color::WHITE);
            let selected = self.selected.as_deref() == Some(n.id.as_str());
            let (width, color) = if selected { (2.0, hex(SELECTED)) } else { (1.0, hex(NODE_BORDER)) };
            frame.stroke(&body, Stroke::default().with_width(width).with_color(color));
            let Some(result) = self.model.result(&n.id) else { continue };
            let badge_color = rating_color(&self.model, result.rating);
            if !detailed {
                frame.fill(&rounded(n.x + 12.0, n.y + n.h - 29.0, 70.0, 20.0, 10.0), badge_color);
                continue;
            }
            let title = canvas::Text {
                max_width: (n.w - 24.0) * z,
                ..label(n.name.clone(), at(n.x + 12.0, n.y + 9.0), 14.0, hex(INK), false)
            };
            frame.fill_text(title);
            let text = format!("{} · {}", result.ap, self.model.pack().ratings[result.rating].label);
            let (bx, by, bw) = (n.x + 12.0, n.y + n.h - 29.0, self.text_width(&text, 12.0) + 18.0);
            frame.fill(&rounded(bx, by, bw, 20.0, 10.0), badge_color);
            frame.fill_text(label(text, at(bx + 9.0, by + 10.0), 12.0, Color::WHITE, true));
            if n.kind.is_gate() {
                let kind = n.kind.label();
                let w = self.text_width(kind, 11.0) + 14.0;
                let (cx, cy) = (n.x + n.w - 12.0 - w, n.y + n.h - 28.0);
                let chip = rounded(cx, cy, w, 18.0, 4.0);
                frame.fill(&chip, hex(CHIP));
                frame.stroke(&chip, Stroke::default().with_width(1.0).with_color(hex(NODE_BORDER)));
                frame.fill_text(label(kind.into(), at(cx + 7.0, cy + 9.0), 11.0, hex(MUTED), true));
            }
        }
        if self.first_draw.replace(false) {
            let secs = START.get().map_or(0.0, |s| s.elapsed().as_secs_f32());
            println!("FIRST_DRAW {secs:.2}s nodes={} drawn={drawn}", self.layout.len());
        } else if std::env::var_os("PERF").is_some() {
            println!("PAINT {:.2}ms zoom={z:.2} drawn={drawn}", t0.elapsed().as_secs_f32() * 1000.0);
        }
    }
}

// ----- canvas program -----

struct TreeCanvas<'a> {
    app: &'a App,
}

#[derive(Default)]
struct Drag {
    last: Option<Point>,
}

impl canvas::Program<Message> for TreeCanvas<'_> {
    type State = Drag;

    fn update(
        &self,
        drag: &mut Drag,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let p = cursor.position_in(bounds)?;
                let id = self.app.hit(p)?;
                Some(canvas::Action::publish(Message::OpenMenu(id, p)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                let hit = self.app.hit(p);
                if hit.is_none() {
                    drag.last = cursor.position();
                }
                Some(canvas::Action::publish(Message::Select(hit)).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let last = drag.last?;
                drag.last = Some(*position);
                Some(canvas::Action::publish(Message::Pan(*position - last)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                drag.last.take()?;
                Some(canvas::Action::publish(Message::PanEnd))
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let p = cursor.position_in(bounds)?;
                let steps = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 40.0,
                };
                Some(canvas::Action::publish(Message::Zoom(steps, p)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _drag: &Drag,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        vec![self.app.cache.draw(renderer, bounds.size(), |frame| self.app.paint(frame, bounds.size()))]
    }

    fn mouse_interaction(&self, drag: &Drag, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if drag.last.is_some() {
            return mouse::Interaction::Grabbing;
        }
        match cursor.position_in(bounds).and_then(|p| self.app.hit(p)) {
            Some(_) => mouse::Interaction::Pointer,
            None => mouse::Interaction::Idle,
        }
    }
}

// ----- small widget helpers (the custom menu is built from these) -----

fn panel<'a>(content: impl Into<Element<'a, Message>>, width: f32, padding: f32) -> Element<'a, Message> {
    container(content)
        .padding(padding)
        .width(width)
        .style(|_| container::Style {
            background: Some(Color::WHITE.into()),
            border: Border { width: 1.0, color: hex(NODE_BORDER), radius: 8.0.into() },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.18),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 14.0,
            },
            ..Default::default()
        })
        .into()
}

#[allow(clippy::too_many_arguments)]
fn menu_row<'a>(
    marker: &'a str,
    label: &str,
    detail: &str,
    arrow: bool,
    highlighted: bool,
    enabled: bool,
    hover: Message,
    click: Option<Message>,
) -> Element<'a, Message> {
    let fg = hex(if enabled { INK } else { DISABLED });
    let content = row![
        text(marker).size(10).width(14).color(fg),
        text(label.to_string()).size(14).color(fg).wrapping(Wrapping::None).width(Fill),
        text(detail.to_string()).size(12).color(hex(if enabled { MUTED } else { DISABLED })).wrapping(Wrapping::None),
        text(if arrow { "›" } else { "" }).size(15).width(12).color(fg),
    ]
    .spacing(6)
    .align_y(Center);
    let cell = container(content).padding([0, 8]).height(ITEM_H).width(Fill).align_y(Center).style(move |_| {
        container::Style {
            background: highlighted.then(|| hex(HILITE).into()),
            border: Border { radius: 5.0.into(), ..Default::default() },
            ..Default::default()
        }
    });
    let area = mouse_area(cell).on_move(move |_| hover.clone());
    match click {
        Some(msg) => area.on_release(msg).interaction(mouse::Interaction::Pointer).into(),
        None => area.into(),
    }
}

fn separator<'a>() -> Element<'a, Message> {
    let line = container(space())
        .width(Fill)
        .height(1)
        .style(|_| container::Style { background: Some(hex(0xE3E6EB).into()), ..Default::default() });
    container(line).padding([4, 6]).into()
}

fn pill<'a>(label: String, color: Color) -> Element<'a, Message> {
    container(text(label).size(12).color(Color::WHITE))
        .padding([2, 10])
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: Border { radius: 10.0.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
}

/// Test-harness accommodation only: with no window manager (Xvfb), nobody gives the window X input
/// focus, so keys never arrive. winit's focus request needs a WM, so set the focus directly.
#[cfg(target_os = "linux")]
fn focus_without_wm(win: u64) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, InputFocus};
    std::thread::spawn(move || {
        let Ok((conn, screen)) = x11rb::connect(None) else { return };
        let root = conn.setup().roots[screen].root;
        let has_wm = conn
            .intern_atom(false, b"_NET_SUPPORTING_WM_CHECK")
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|a| conn.get_property(false, root, a.atom, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok())
            .is_some_and(|p| p.value_len > 0);
        for _ in 0..20 {
            if has_wm {
                return;
            }
            let _ = conn.set_input_focus(InputFocus::PARENT, win as u32, x11rb::CURRENT_TIME);
            let _ = conn.flush();
            std::thread::sleep(std::time::Duration::from_millis(150));
            if conn.get_input_focus().ok().and_then(|c| c.reply().ok()).is_some_and(|f| f.focus == win as u32) {
                println!("X11 without window manager: input focus set on 0x{win:x}");
                return;
            }
        }
    });
}

fn main() -> iced::Result {
    START.get_or_init(Instant::now);
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,iced_wgpu=info")).init();
    iced::application(App::new, App::update, App::view)
        .title("AegisTARA spike - iced")
        .window_size(Size::new(WINDOW_W, WINDOW_H))
        .theme(Theme::Light)
        .antialiasing(true)
        .subscription(App::subscription)
        .run()
}
