//! AegisTARA UI toolkit spike: egui + eframe.
//!
//! Canvas: painted by hand with `ui.painter()`, one `ui.interact` per visible node.
//! Menus: built-in `Response::context_menu`, `ui.menu_button` (submenus) and `ui.radio`.
//! Popover: built-in `egui::Popup` anchored at the node rect, with `ComboBox` and `TextEdit`.
//! Every factor, level, label, value and rating color comes from the pack at runtime.

use eframe::egui::{
    self, Align, Button, Color32, ComboBox, FontId, Frame, Id, Key, KeyboardShortcut, Layout, Modifiers, PointerButton,
    Popup, PopupAnchor, PopupCloseBehavior, PopupKind, Pos2, Rect, RectAlign, RichText, Sense, Stroke, StrokeKind,
    TextEdit, Ui, Vec2, containers::menu, epaint::CubicBezierShape, pos2, text::LayoutJob, vec2,
};
use spike_model::{Kind, Model, Placed, Rating};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const TITLE: &str = "AegisTARA spike - egui";
const BG: Color32 = Color32::from_rgb(0xF6, 0xF7, 0xF9);
const BORDER: Color32 = Color32::from_rgb(0xD0, 0xD4, 0xDC);
const EDGE: Color32 = Color32::from_rgb(0x9A, 0xA3, 0xB2);
const SELECTED: Color32 = Color32::from_rgb(0x3D, 0x6D, 0xE8);
const TEXT: Color32 = Color32::from_rgb(0x1F, 0x24, 0x2E);
const MUTED: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
const CHIP: Color32 = Color32::from_rgb(0xEC, 0xEF, 0xF4);
const STATUS_H: f32 = 24.0;

/// UI closures only record what the user asked for; the model is changed after the frame is built.
enum Action {
    Select(Option<String>),
    SetFactor { node: String, factor: String, level: String },
    SetKind(String, Kind),
    AddChild(String),
    Delete(String),
    OpenEdit(String),
    Commit(Draft),
    Undo,
}

/// Working copy for the edit popover. Committed with `set_factors` on OK.
#[derive(Clone)]
struct Draft {
    node: String,
    levels: BTreeMap<String, String>,
    rationale: String,
}

struct App {
    model: Model,
    // Layout cache, rebuilt only when the model generation changes.
    layout: Vec<Placed>,
    parent_ix: Vec<Option<usize>>,
    layout_gen: Option<u64>,
    pan: Vec2,
    zoom_step: i32,
    selected: Option<String>,
    kb_menu: Option<String>,
    kb_menu_open: bool,
    kb_just_opened: bool,
    edit: Option<Draft>,
    edit_open: bool,
    edit_just_opened: bool,
    actions: Vec<Action>,
    menu_key: Arc<AtomicBool>,
    frames: u64,
    started: Instant,
    bench: bool,
    visible: usize,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    x11_focus_pending: bool,
}

fn rating_color(r: &Rating) -> Color32 {
    let [r, g, b] = r.rgb();
    Color32::from_rgb(r, g, b)
}

fn level_text(label: &str, value: u32) -> String {
    format!("{label}  ({value})")
}

impl App {
    fn new(model: Model, menu_key: Arc<AtomicBool>, started: Instant) -> Self {
        App {
            model,
            layout: Vec::new(),
            parent_ix: Vec::new(),
            layout_gen: None,
            pan: Vec2::ZERO,
            zoom_step: 0,
            selected: None,
            kb_menu: None,
            kb_menu_open: false,
            kb_just_opened: false,
            edit: None,
            edit_open: false,
            edit_just_opened: false,
            actions: Vec::new(),
            menu_key,
            frames: 0,
            started,
            bench: std::env::var("BENCH").is_ok_and(|v| v == "1"),
            visible: 0,
            x11_focus_pending: cfg!(target_os = "linux"),
        }
    }

    /// Zoom snaps to steps of 1.1^n, which keeps egui's glyph cache warm while zooming.
    fn zoom(&self) -> f32 {
        1.1f32.powi(self.zoom_step)
    }

    fn refresh_layout(&mut self) {
        if self.layout_gen == Some(self.model.generation()) {
            return;
        }
        self.layout = self.model.layout();
        let index: HashMap<&str, usize> = self.layout.iter().enumerate().map(|(i, p)| (p.id.as_str(), i)).collect();
        self.parent_ix =
            self.layout.iter().map(|p| p.parent.as_deref().and_then(|id| index.get(id).copied())).collect();
        self.layout_gen = Some(self.model.generation());
    }

    fn handle_keys(&mut self, ui: &mut Ui) {
        if std::env::var("DEBUG_INPUT").is_ok() {
            let focused = ui.memory(|m| m.focused());
            ui.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::Key { key, pressed: true, .. } => println!("DBG key={key:?} focused={focused:?}"),
                        egui::Event::MouseWheel { .. } => println!("DBG {e:?}"),
                        _ => {}
                    }
                }
            });
        }
        let shift_f10 = ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F10));
        let menu_key = self.menu_key.swap(false, Ordering::Relaxed);
        if (shift_f10 || menu_key)
            && let Some(sel) = self.selected.clone()
        {
            println!("KEYBOARD_MENU {sel} via {}", if menu_key { "Menu key" } else { "Shift+F10" });
            self.kb_menu = Some(sel);
            self.kb_menu_open = true;
            self.kb_just_opened = true;
        }
        let undo = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
        // TextEdit has its own undo; leave Ctrl+Z to it while it has focus.
        if !self.model.read_only && !ui.ctx().text_edit_focused() && ui.input_mut(|i| i.consume_shortcut(&undo)) {
            self.actions.push(Action::Undo);
        }
    }

    fn status_bar(&self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            let sel = self.selected.as_deref().and_then(|id| self.model.node(id));
            match sel {
                Some(n) => ui.label(format!("Selected: {} · {}", n.id, n.name)),
                None => ui.label(RichText::new("Right-click a node to edit it").color(MUTED)),
            };
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} nodes · zoom {:.0}%", self.layout.len(), self.zoom() * 100.0))
                        .color(MUTED),
                );
                if self.model.read_only {
                    ui.label(RichText::new(" READ-ONLY ").color(Color32::WHITE).background_color(MUTED));
                }
            });
        });
    }

    fn canvas(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let origin = rect.min;
        // Background first: widgets registered later win the hit test, so nodes sit on top.
        let bg = ui.interact(rect, Id::new("canvas-bg"), Sense::CLICK | Sense::DRAG);
        if bg.dragged_by(PointerButton::Primary) {
            self.pan += bg.drag_delta();
        }
        if bg.clicked() {
            self.actions.push(Action::Select(None));
        }
        // Wheel zooms around the cursor (only when the canvas, not a popup, is under the pointer).
        if let Some(pos) = ui.ctx().pointer_hover_pos().filter(|_| ui.rect_contains_pointer(rect)) {
            let wheel: f32 = ui.input(|i| {
                i.events
                    .iter()
                    .map(|e| match e {
                        egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, .. } => delta.y / 40.0,
                        egui::Event::MouseWheel { delta, .. } => delta.y,
                        _ => 0.0,
                    })
                    .sum()
            });
            if wheel != 0.0 {
                let world = (pos - origin - self.pan) / self.zoom();
                self.zoom_step = (self.zoom_step + wheel.round().clamp(-3.0, 3.0) as i32).clamp(-25, 15);
                self.pan = pos - origin - world * self.zoom();
            }
        }

        let zoom = self.zoom();
        let pan = self.pan;
        let to_screen = |p: Pos2| origin + pan + p.to_vec2() * zoom;
        // Visible world rect, for culling.
        let vis = Rect::from_min_max(
            ((rect.min - origin - pan) / zoom).to_pos2(),
            ((rect.max - origin - pan) / zoom).to_pos2(),
        );
        let painter = ui.painter_at(rect);

        for (i, p) in self.layout.iter().enumerate() {
            let Some(q) = self.parent_ix[i].map(|j| &self.layout[j]) else { continue };
            let (a, b) = (pos2(q.x + q.w, q.y + q.h / 2.0), pos2(p.x, p.y + p.h / 2.0));
            if !Rect::from_two_pos(a, b).intersects(vis) {
                continue;
            }
            let (a, b) = (to_screen(a), to_screen(b));
            let dx = vec2((b.x - a.x) * 0.5, 0.0);
            painter.add(CubicBezierShape::from_points_stroke(
                [a, a + dx, b - dx, b],
                false,
                Color32::TRANSPARENT,
                Stroke::new(1.5, EDGE),
            ));
        }

        let mut kb_rect = None;
        let mut edit_rect = None;
        let mut visible = 0;
        for p in &self.layout {
            let world = Rect::from_min_size(pos2(p.x, p.y), vec2(p.w, p.h));
            if !world.intersects(vis) {
                continue;
            }
            visible += 1;
            let sr = Rect::from_min_max(to_screen(world.min), to_screen(world.max));
            // CLICK without FOCUSABLE: arrow keys in menus must not jump to canvas nodes.
            let resp = ui.interact(sr, Id::new(("node", &p.id)), Sense::CLICK);
            let badge = self.model.result(&p.id).zip(self.model.rating_of(&p.id));
            resp.widget_info(|| {
                let b = badge.map(|(r, rt)| format!("{} · {}", r.ap, rt.label)).unwrap_or_default();
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{} {}: {b}", p.kind.label(), p.name))
            });
            self.paint_node(&painter, p, sr, zoom);
            if resp.clicked() || resp.secondary_clicked() {
                self.actions.push(Action::Select(Some(p.id.clone())));
            }
            let (model, actions) = (&self.model, &mut self.actions);
            resp.context_menu(|ui| {
                node_menu(ui, model, &p.id, actions);
            });
            if self.kb_menu.as_deref() == Some(p.id.as_str()) {
                kb_rect = Some(sr);
            }
            if self.edit.as_ref().is_some_and(|d| d.node == p.id) {
                edit_rect = Some(sr);
            }
        }

        // Keyboard-opened menu (Shift+F10 / Menu key): same menu function, anchored at the node.
        if let (Some(node), Some(anchor)) = (self.kb_menu.as_deref(), kb_rect) {
            let just_opened = std::mem::take(&mut self.kb_just_opened);
            let (model, actions) = (&self.model, &mut self.actions);
            Popup::new(Id::new("kb-menu"), ui.ctx().clone(), PopupAnchor::ParentRect(anchor), ui.layer_id())
                .kind(PopupKind::Menu)
                .layout(Layout::top_down_justified(Align::Min))
                .style(menu::menu_style)
                .align(RectAlign::BOTTOM_START)
                .gap(2.0)
                .open_bool(&mut self.kb_menu_open)
                .show(|ui| {
                    let first = node_menu(ui, model, node, actions);
                    if just_opened && let Some(first) = first {
                        first.request_focus();
                    }
                });
        }
        if !self.kb_menu_open {
            self.kb_menu = None;
        }

        // Edit popover, anchored to the right of the node; flips or shifts at window edges.
        if let (Some(draft), Some(anchor)) = (self.edit.as_mut(), edit_rect) {
            let just_opened = std::mem::take(&mut self.edit_just_opened);
            let (model, actions) = (&self.model, &mut self.actions);
            let mut close = false;
            Popup::new(Id::new("edit-popover"), ui.ctx().clone(), PopupAnchor::ParentRect(anchor), ui.layer_id())
                .align(RectAlign::RIGHT_START)
                .gap(8.0)
                .width(330.0)
                .frame(Frame::popup(ui.style()).inner_margin(12))
                .close_behavior(PopupCloseBehavior::IgnoreClicks)
                .open_bool(&mut self.edit_open)
                .show(|ui| {
                    if let Some(commit) = edit_panel(ui, model, draft, just_opened) {
                        if commit {
                            actions.push(Action::Commit(draft.clone()));
                        }
                        close = true;
                    }
                });
            if close {
                self.edit_open = false;
            }
        }
        if !self.edit_open {
            self.edit = None;
        }

        self.visible = visible;
    }

    fn paint_node(&self, painter: &egui::Painter, p: &Placed, sr: Rect, zoom: f32) {
        let selected = self.selected.as_deref() == Some(p.id.as_str());
        let stroke = if selected { Stroke::new(2.0, SELECTED) } else { Stroke::new(1.0, BORDER) };
        painter.rect(sr, 8.0 * zoom, Color32::WHITE, stroke, StrokeKind::Inside);
        let pad = 10.0 * zoom;
        let rating = self.model.result(&p.id).zip(self.model.rating_of(&p.id));
        // Level of detail: below 40 % text is unreadable, so draw only the rating color as a bar.
        if zoom < 0.4 {
            if let Some((_, rating)) = rating {
                let bar = Rect::from_min_size(
                    pos2(sr.min.x + pad, sr.max.y - pad - 20.0 * zoom),
                    vec2(sr.width() * 0.4, 20.0 * zoom),
                );
                painter.rect_filled(bar, 0.0, rating_color(rating));
            }
            return;
        }
        let mut chip_w = 0.0;
        if p.kind.is_gate() {
            let g = painter.layout_no_wrap(p.kind.label().to_string(), FontId::proportional(11.0 * zoom), MUTED);
            let chip = Rect::from_min_size(
                pos2(sr.max.x - pad - g.size().x - 12.0 * zoom, sr.min.y + pad),
                vec2(g.size().x + 12.0 * zoom, 18.0 * zoom),
            );
            painter.rect_filled(chip, 4.0 * zoom, CHIP);
            painter.galley(chip.center() - g.size() / 2.0, g, MUTED);
            chip_w = chip.width() + 6.0 * zoom;
        }
        // Title: 14 px, wrapped to at most two lines with an ellipsis.
        let mut job = LayoutJob::simple(
            p.name.clone(),
            FontId::proportional(14.0 * zoom),
            TEXT,
            (sr.width() - 2.0 * pad - chip_w).max(1.0),
        );
        job.wrap.max_rows = 2;
        painter.galley(sr.min + vec2(pad, pad - 2.0 * zoom), painter.layout_job(job), TEXT);
        // Rating badge pill: "<AP> · <rating label>" in the rating color from the pack.
        if let Some((r, rating)) = rating {
            let g = painter.layout_no_wrap(
                format!("{} · {}", r.ap, rating.label),
                FontId::proportional(12.0 * zoom),
                Color32::WHITE,
            );
            let h = 20.0 * zoom;
            let pill = Rect::from_min_size(pos2(sr.min.x + pad, sr.max.y - pad - h), vec2(g.size().x + 16.0 * zoom, h));
            painter.rect_filled(pill, h / 2.0, rating_color(rating));
            painter.galley(pill.center() - g.size() / 2.0, g, Color32::WHITE);
            painter.text(
                pos2(sr.max.x - pad, pill.center().y),
                egui::Align2::RIGHT_CENTER,
                &p.id,
                FontId::monospace(11.0 * zoom),
                MUTED,
            );
        }
    }

    fn apply_actions(&mut self) {
        for action in std::mem::take(&mut self.actions) {
            let result = match action {
                Action::Select(id) => {
                    self.selected = id;
                    Ok(())
                }
                Action::SetFactor { node, factor, level } => self.model.set_factor(&node, &factor, &level),
                Action::SetKind(node, kind) => self.model.set_kind(&node, kind),
                Action::AddChild(node) => self.model.add_child_step(&node).map(|id| self.selected = Some(id)),
                Action::Delete(node) => {
                    if self.selected.as_deref() == Some(node.as_str()) {
                        self.selected = None;
                    }
                    self.model.delete(&node)
                }
                Action::OpenEdit(node) => match self.model.node(&node) {
                    Some(n) => {
                        self.edit = Some(Draft { node, levels: n.factors.clone(), rationale: n.rationale.clone() });
                        self.edit_open = true;
                        self.edit_just_opened = true;
                        Ok(())
                    }
                    None => Err("no such node".into()),
                },
                Action::Commit(d) => {
                    println!("COMMIT {} rationale={:?}", d.node, d.rationale);
                    self.model.set_factors(&d.node, d.levels, d.rationale)
                }
                Action::Undo => {
                    if !self.model.undo() {
                        println!("UNDO nothing to undo");
                    }
                    Ok(())
                }
            };
            if let Err(e) = result {
                println!("ERROR {e}");
            }
        }
    }
}

/// The node menu, built from the pack every frame. Shared by the right-click and the keyboard menu.
/// Returns the first item, so a keyboard-opened menu can focus it.
fn node_menu(ui: &mut Ui, model: &Model, id: &str, out: &mut Vec<Action>) -> Option<egui::Response> {
    let node = model.node(id)?;
    let mut first = None;
    // Read-only: the menu still opens, but every editing item is greyed out.
    ui.add_enabled_ui(!model.read_only, |ui| {
        if node.kind == Kind::Step {
            for f in &model.pack().factors {
                let current = node.factors.get(&f.id);
                let r = submenu(ui, &f.label, |ui, focus_current| {
                    let mut any_focused = false;
                    for l in &f.levels {
                        let is_current = current == Some(&l.id);
                        let item = ui.radio(is_current, level_text(&l.label, l.value));
                        if focus_current && is_current {
                            item.request_focus();
                        }
                        any_focused |= item.has_focus();
                        if item.clicked() {
                            if !is_current {
                                out.push(Action::SetFactor {
                                    node: id.into(),
                                    factor: f.id.clone(),
                                    level: l.id.clone(),
                                });
                            }
                            ui.close();
                        }
                    }
                    any_focused
                });
                first.get_or_insert(r);
            }
            ui.separator();
            if ui.button("Edit feasibility…").clicked() {
                out.push(Action::OpenEdit(id.into()));
                ui.close();
            }
        } else {
            let r = submenu(ui, "Gate type", |ui, focus_current| {
                let mut any_focused = false;
                for kind in [Kind::And, Kind::Or] {
                    let item = ui.radio(node.kind == kind, kind.label());
                    if focus_current && node.kind == kind {
                        item.request_focus();
                    }
                    any_focused |= item.has_focus();
                    if item.clicked() {
                        if node.kind != kind {
                            out.push(Action::SetKind(id.into(), kind));
                        }
                        ui.close();
                    }
                }
                any_focused
            });
            first = Some(r);
            if ui.button("Add child step").clicked() {
                out.push(Action::AddChild(id.into()));
                ui.close();
            }
        }
        if id != model.root().id {
            ui.separator();
            if ui.button("Delete").clicked() {
                out.push(Action::Delete(id.into()));
                ui.close();
            }
        }
        ui.separator();
        if ui.add_enabled(model.can_undo(), Button::new("Undo").shortcut_text("Ctrl+Z")).clicked() {
            out.push(Action::Undo);
            ui.close();
        }
    });
    first
}

/// `ui.menu_button` (which becomes a submenu inside a menu) plus keyboard glue that egui 0.36 lacks:
/// Right or Enter on the item opens the submenu and focuses its current entry; Left goes back.
/// Without this, egui's spatial focus moves onto the submenu's popup area instead of an item.
/// `items` gets "focus the current entry now" and returns whether one of its entries has focus.
fn submenu(ui: &mut Ui, label: &str, items: impl FnOnce(&mut Ui, bool) -> bool) -> egui::Response {
    let button_id = ui.next_auto_id(); // the id SubMenuButton will use
    let focused = ui.memory(|m| m.has_focus(button_id));
    let (right, left, enter) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowRight),
            i.key_pressed(Key::ArrowLeft),
            i.key_pressed(Key::Enter) || i.key_pressed(Key::Space),
        )
    });
    let enter_sub = focused && (right || enter);
    if enter_sub {
        if right {
            // A submenu that was never shown would be reset at once, so mark it shown first.
            let sub_id = menu::SubMenu::id_from_widget_id(button_id);
            menu::MenuState::mark_shown(ui.ctx(), sub_id);
            menu::MenuState::from_ui(ui, |s, _| s.open_item = Some(sub_id));
        }
        ui.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
    }
    let r = ui.menu_button(label, |ui| items(ui, enter_sub));
    if left && r.inner == Some(true) {
        menu::MenuState::from_ui(ui, |s, _| s.open_item = None);
        ui.memory_mut(|m| {
            m.request_focus(button_id);
            m.move_focus(egui::FocusDirection::None);
        });
    }
    r.response
}

/// Edit popover body. Returns Some(true) on OK, Some(false) on Cancel.
fn edit_panel(ui: &mut Ui, model: &Model, d: &mut Draft, just_opened: bool) -> Option<bool> {
    let node = model.node(&d.node)?;
    ui.label(RichText::new("Edit feasibility").strong().size(15.0).color(TEXT));
    ui.label(RichText::new(format!("{} · {}", node.id, node.name)).color(MUTED));
    ui.add_space(6.0);
    egui::Grid::new("edit-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        for (i, f) in model.pack().factors.iter().enumerate() {
            ui.label(&f.label);
            let current = d.levels.entry(f.id.clone()).or_default();
            let text =
                f.levels.iter().find(|l| &l.id == current).map(|l| level_text(&l.label, l.value)).unwrap_or_default();
            let combo =
                ComboBox::from_id_salt(("edit-factor", &f.id)).width(170.0).selected_text(text).show_ui(ui, |ui| {
                    for l in &f.levels {
                        ui.selectable_value(current, l.id.clone(), level_text(&l.label, l.value));
                    }
                });
            if just_opened && i == 0 {
                combo.response.request_focus();
            }
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label("Rationale");
    ui.add(
        TextEdit::multiline(&mut d.rationale)
            .desired_rows(4)
            .desired_width(f32::INFINITY)
            .hint_text("Why these values?"),
    );
    ui.add_space(4.0);
    // Live preview from the draft; the model is not changed until OK.
    let (ap, idx) = model.preview(&d.levels);
    let rating = &model.pack().ratings[idx];
    let mut result = None;
    ui.horizontal(|ui| {
        ui.label("Preview");
        Frame::new().fill(rating_color(rating)).corner_radius(10).inner_margin(egui::Margin::symmetric(8, 2)).show(
            ui,
            |ui| {
                ui.label(RichText::new(format!("{ap} · {}", rating.label)).color(Color32::WHITE));
            },
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                result = Some(false);
            }
            if ui.add(Button::new(RichText::new("  OK  ").color(Color32::WHITE)).fill(SELECTED)).clicked() {
                result = Some(true);
            }
        });
    });
    result
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        self.frames += 1;
        if self.frames == 2 {
            println!("STARTUP first_frame_ms={}", self.started.elapsed().as_millis());
        }
        #[cfg(target_os = "linux")]
        if self.x11_focus_pending && self.frames >= 2 {
            self.x11_focus_pending = !x11_focus_if_no_wm(frame);
            if self.x11_focus_pending {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        // cpu_ms: eframe's measure of the previous frame (UI code + tessellation + render submit).
        if self.frames.is_multiple_of(if self.bench { 60 } else { 20 }) && (self.bench || self.layout.len() > 1000) {
            let cpu_ms = frame.info().cpu_usage.unwrap_or(0.0) * 1000.0;
            println!(
                "PERF frame={} t_ms={} cpu_ms={cpu_ms:.2} visible_nodes={} of {}",
                self.frames,
                self.started.elapsed().as_millis(),
                self.visible,
                self.layout.len()
            );
        }
        self.refresh_layout();
        self.handle_keys(ui);
        egui::Panel::bottom("status")
            .exact_size(STATUS_H)
            .resizable(false)
            .frame(Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().frame(Frame::NONE.fill(BG)).show(ui, |ui| self.canvas(ui));
        self.apply_actions();
        if self.bench {
            ui.ctx().request_repaint();
        }
    }
}

/// eframe's winit adapter, plus one thing egui-winit lacks: the Menu (Apps) key.
/// egui::Key has no ContextMenu variant, so catch the raw winit key and forward everything.
struct MenuKeyHook<'a> {
    inner: eframe::EframeWinitApplication<'a>,
    flag: Arc<AtomicBool>,
}

impl winit::application::ApplicationHandler<eframe::UserEvent> for MenuKeyHook<'_> {
    fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        self.inner.resumed(el);
    }
    fn window_event(
        &mut self,
        el: &winit::event_loop::ActiveEventLoop,
        id: winit::window::WindowId,
        ev: winit::event::WindowEvent,
    ) {
        use winit::keyboard::{Key as WKey, NamedKey};
        if let winit::event::WindowEvent::KeyboardInput { event, .. } = &ev
            && event.state.is_pressed()
            && event.logical_key == WKey::Named(NamedKey::ContextMenu)
        {
            self.flag.store(true, Ordering::Relaxed);
        }
        self.inner.window_event(el, id, ev);
    }
    fn new_events(&mut self, el: &winit::event_loop::ActiveEventLoop, cause: winit::event::StartCause) {
        self.inner.new_events(el, cause);
    }
    fn user_event(&mut self, el: &winit::event_loop::ActiveEventLoop, ev: eframe::UserEvent) {
        self.inner.user_event(el, ev);
    }
    fn device_event(
        &mut self,
        el: &winit::event_loop::ActiveEventLoop,
        id: winit::event::DeviceId,
        ev: winit::event::DeviceEvent,
    ) {
        self.inner.device_event(el, id, ev);
    }
    fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        self.inner.about_to_wait(el);
    }
    fn suspended(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        self.inner.suspended(el);
    }
    fn exiting(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        self.inner.exiting(el);
    }
    fn memory_warning(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        self.inner.memory_warning(el);
    }
}

fn log_renderer(cc: &eframe::CreationContext<'_>) {
    if let Some(rs) = &cc.wgpu_render_state {
        let info = rs.adapter.get_info();
        println!(
            "RENDERER wgpu backend={:?} device_type={:?} adapter={:?} driver={:?}",
            info.backend, info.device_type, info.name, info.driver_info
        );
    }
    if let Some(gl) = &cc.gl {
        use eframe::glow::HasContext;
        let (renderer, version) = unsafe {
            (gl.get_parameter_string(eframe::glow::RENDERER), gl.get_parameter_string(eframe::glow::VERSION))
        };
        println!("RENDERER glow renderer={renderer:?} version={version:?}");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    env_logger::init();
    let model = Model::load_default()?;
    model.log_layout(0.0, 0.0);
    model.log_results();
    let renderer = match std::env::var("RENDERER").as_deref() {
        Ok("glow") => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let options = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([spike_model::WINDOW_W, spike_model::WINDOW_H]),
        ..Default::default()
    };
    let flag = Arc::new(AtomicBool::new(false));
    let app_flag = flag.clone();
    let event_loop = winit::event_loop::EventLoop::<eframe::UserEvent>::with_user_event().build()?;
    let inner = eframe::create_native(
        TITLE,
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_theme(egui::Theme::Light);
            // Slightly roomier than egui's compact defaults (12.5 px text), to sit next to 14 px titles.
            cc.egui_ctx.all_styles_mut(|s| {
                for style in [egui::TextStyle::Body, egui::TextStyle::Button] {
                    s.text_styles.insert(style, FontId::proportional(13.5));
                }
                s.spacing.button_padding = vec2(8.0, 3.0);
                s.spacing.item_spacing.y = 4.0;
            });
            log_renderer(cc);
            Ok(Box::new(App::new(model, app_flag, started)))
        }),
        &event_loop,
    );
    event_loop.run_app(&mut MenuKeyHook { inner, flag })?;
    Ok(())
}

/// Test-harness helper. With no window manager (Xvfb), nobody gives the window X input focus,
/// so winit never receives key events. Only acts when no EWMH window manager is running.
#[cfg(target_os = "linux")]
fn x11_focus_if_no_wm(frame: &eframe::Frame) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, InputFocus};
    let Ok(handle) = frame.window_handle() else { return true };
    let win = match handle.as_raw() {
        RawWindowHandle::Xlib(x) => x.window as u32,
        RawWindowHandle::Xcb(x) => x.window.get(),
        _ => return true, // not X11
    };
    let Ok((conn, screen)) = x11rb::connect(None) else { return true };
    let root = conn.setup().roots[screen].root;
    let has_wm = conn
        .intern_atom(false, b"_NET_SUPPORTING_WM_CHECK")
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|a| conn.get_property(false, root, a.atom, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok())
        .is_some_and(|p| p.value_len > 0);
    if has_wm {
        return true;
    }
    // Fails with BadMatch until the window is mapped, so the caller retries.
    let ok = conn.set_input_focus(InputFocus::PARENT, win, x11rb::CURRENT_TIME).is_ok_and(|c| c.check().is_ok());
    if ok {
        println!("X11 no window manager: set input focus on 0x{win:x}");
    }
    ok
}
