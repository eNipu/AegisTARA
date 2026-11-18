//! The `Workbench` view: top bar, dock area and status bar over the sample.
//! It owns the workspace and the explorer state; the panels only render.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::command_table::{command, shortcut_label};
use aegis_client::explorer::{
    ExplorerIndex, ExplorerKey, ExplorerRow, ExplorerState, Folder, Owner, drop_edit,
};
use aegis_client::i18n::Catalog;
use aegis_client::pages::{problems, selection_visual_id};
use aegis_client::settings::{Appearance, Settings, WorkbenchLayout};
use aegis_client::view::Selection;
use aegis_client::workspace::Workspace;
use aegis_model::ElementNamesDef;
use gpui::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div, px,
};
use gpui_component::ActiveTheme as _;
use gpui_component::dock::{
    BasePanel, BasePanelView, DockArea, DockEvent, DockLayout, DockPlacement, DockSkin, PaneRef,
    PanelId, panel_handle,
};
use gpui_component::input::{InputEvent, InputState};

use super::apply_appearance;
use super::commands::on_every_command;
use super::layout::{apply_layout, default_layout, read_layout};
use super::panels::{BottomPanel, ElementTab, ExplorerPanel, InspectorPanel, ListTab, OverviewTab};
use crate::local;
use crate::sample::{SAMPLE_KEY, SAMPLE_NAME, workbench_sample};

/// The tab Root, Overview and Systems open.
const OVERVIEW: ExplorerKey = ExplorerKey::Folder(Owner::Project, Folder::Overview);
/// The one problems tab every Problems folder opens.
const PROBLEMS: ExplorerKey = ExplorerKey::Folder(Owner::Project, Folder::Problems);

/// What a center tab shows, to refresh it after a change.
enum Body {
    Overview(Entity<OverviewTab>),
    List(Entity<ListTab>),
    Element(Entity<ElementTab>),
}

struct Tab {
    handle: Arc<dyn BasePanelView>,
    body: Body,
}

/// The window's root view.
pub struct Workbench {
    workspace: Workspace,
    index: ExplorerIndex,
    state: ExplorerState,
    catalog: Rc<Catalog>,
    names: ElementNamesDef,
    actor: String,
    explorer: Entity<ExplorerPanel>,
    filter: Entity<InputState>,
    area: Entity<DockArea>,
    tabs: HashMap<ExplorerKey, Tab>,
    appearance: Appearance,
    picked: Option<(ExplorerKey, String)>,
    note: Option<String>,
    status: SharedString,
    settings_path: Option<PathBuf>,
    saved: Option<WorkbenchLayout>,
    _skin: Rc<DockSkin>,
    _subscriptions: [Subscription; 3],
}

/// Every row of the tree, all nodes expanded. Time O(tree x depth).
fn all_rows(index: &ExplorerIndex) -> Vec<ExplorerRow> {
    let mut state = ExplorerState {
        selected: None,
        ..ExplorerState::default()
    };
    loop {
        let rows = index.rows(&state);
        let closed: Vec<_> = rows
            .iter()
            .filter(|r| r.has_children && !r.expanded)
            .map(|r| r.key)
            .collect();
        if closed.is_empty() {
            return rows;
        }
        state.expanded.extend(closed);
    }
}

/// The singular name of the kind of `sel`, if the pack names it.
fn kind(names: &ElementNamesDef, sel: Selection) -> Option<String> {
    let n = match sel {
        Selection::Component(_) => &names.component,
        Selection::Interface(_) => &names.interface,
        Selection::DataFlow(_) => &names.data_flow,
        Selection::TrustBoundary(_) => &names.trust_boundary,
        Selection::Asset(_) => &names.asset,
        Selection::Damage(_) => &names.damage_scenario,
        Selection::Threat(_) => &names.threat_scenario,
        Selection::Tree(_) | Selection::TreeNode(..) => &names.attack_tree,
        Selection::Control(_) => &names.security_control,
        Selection::Goal(_) => &names.cybersecurity_goal,
        Selection::Claim(_) => &names.cybersecurity_claim,
        Selection::Party(_) | Selection::Condition(_) => return None,
    };
    Some(n.singular.to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl Workbench {
    /// The workbench over the sample project, with texts from `catalog`.
    /// The dock layout is read from and saved to `settings_path` (none:
    /// the defaults, never saved).
    pub fn new(
        catalog: Rc<Catalog>,
        appearance: Appearance,
        settings_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = workbench_sample();
        let project = workspace.project();
        let names = project.methodology().element_names().clone();
        let index = ExplorerIndex::build(&workspace, &names, &catalog, SAMPLE_NAME);
        let tr = |id: &str| SharedString::from(catalog.tr(id));

        let counts = [
            (&names.asset, project.asset_count()),
            (&names.threat_scenario, project.threat_scenario_count()),
            (&names.attack_tree, project.attack_tree_count()),
            (&names.security_control, project.security_control_count()),
        ]
        .map(|(n, count)| SharedString::from(format!("{}: {count}", n.plural)));
        let heading = Some(SharedString::from(SAMPLE_NAME));
        let overview =
            cx.new(|cx| OverviewTab::new(tr("workbench-overview"), heading, counts.into(), cx));
        let me = cx.weak_entity();
        let placeholder = tr("cmd-filter-explorer");
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let filtered = cx.subscribe(&filter, |this, input, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.state.filter = input.read(cx).value().to_string();
                this.show_rows(cx);
            }
        });
        let explorer =
            cx.new(|cx| ExplorerPanel::new(tr("workbench-explorer"), filter.clone(), me, cx));
        let inspector_text = vec![tr("workbench-inspector-empty")];
        let inspector =
            cx.new(|cx| InspectorPanel::new(tr("workbench-inspector"), None, inspector_text, cx));
        let bottom_text = vec![tr("workbench-bottom-empty")];
        let bottom = cx.new(|cx| BottomPanel::new(tr("workbench-bottom"), None, bottom_text, cx));

        let layout = settings_path
            .as_ref()
            .and_then(|p| Settings::load(p).workbench_layout(SAMPLE_KEY))
            .unwrap_or_else(|| default_layout(SAMPLE_KEY));
        let (area, skin) = DockSkin::dock_area("workbench", None, window, cx);
        let center = DockLayout::tabs().panel_view(panel_handle(overview.clone()), cx);
        let left = DockLayout::tabs().panel_view(panel_handle(explorer.clone()), cx);
        let right = DockLayout::tabs().panel_view(panel_handle(inspector), cx);
        let below = DockLayout::tabs().panel_view(panel_handle(bottom), cx);
        let saved = area.update(cx, |area, cx| {
            area.set_center(center, window, cx);
            area.set_dock(DockPlacement::Left, left, window, cx);
            area.set_dock(DockPlacement::Right, right, window, cx);
            area.set_dock(DockPlacement::Bottom, below, window, cx);
            apply_layout(area, &layout, window, cx);
            read_layout(area, SAMPLE_KEY)
        });
        let changed = cx.subscribe(&area, |this, _, ev: &DockEvent, cx| {
            if let DockEvent::LayoutChanged = ev {
                this.save_layout(cx);
            }
        });
        let me = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = me.update(cx, |w, cx| w.save_layout(cx));
            true
        });

        let status = format!(
            "{}  ·  {}",
            project.methodology().label(),
            catalog.tr("workbench-status-unsaved")
        )
        .into();
        let follow = cx.observe_window_appearance(window, |this, window, cx| {
            if this.appearance == Appearance::System {
                apply_appearance(this.appearance, window, cx);
            }
        });
        let overview = Tab {
            handle: panel_handle(overview.clone()),
            body: Body::Overview(overview),
        };
        window.focus(&gpui::Focusable::focus_handle(&explorer, cx), cx);
        let mut wb = Self {
            workspace,
            index,
            state: ExplorerState::default(),
            catalog,
            names,
            actor: local::actor(),
            explorer,
            filter,
            area,
            tabs: HashMap::from([(OVERVIEW, overview)]),
            appearance,
            picked: None,
            note: None,
            status,
            settings_path,
            saved: Some(saved),
            _skin: skin,
            _subscriptions: [changed, follow, filtered],
        };
        wb.show_rows(cx);
        wb
    }

    /// The number of rows the explorer dock shows.
    pub fn explorer_row_count(&self, cx: &App) -> usize {
        self.explorer.read(cx).rows().len()
    }

    /// The workspace shown.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// The explorer tree of the workspace.
    pub fn index(&self) -> &ExplorerIndex {
        &self.index
    }

    /// What is open and selected in the explorer.
    pub fn state(&self) -> &ExplorerState {
        &self.state
    }

    /// The row of `key` with every node expanded, if the tree has it.
    pub fn row(&self, key: ExplorerKey) -> Option<ExplorerRow> {
        all_rows(&self.index).into_iter().find(|r| r.key == key)
    }

    /// Whether a center tab shows `key` (Root, Overview and Systems share
    /// the Overview tab, every Problems folder the problems tab).
    pub fn has_tab(&self, key: ExplorerKey, cx: &App) -> bool {
        self.tabs
            .get(&shared(key))
            .is_some_and(|t| self.area.read(cx).panel(t.handle.panel_id(cx)).is_some())
    }

    /// The dock area.
    pub fn area(&self) -> &Entity<DockArea> {
        &self.area
    }

    /// The component being moved with the keyboard, if any.
    pub fn picked(&self) -> Option<ExplorerKey> {
        self.picked.as_ref().map(|(k, _)| *k)
    }

    fn show_rows(&mut self, cx: &mut Context<Self>) {
        let rows = self.index.rows(&self.state);
        let selected = self.state.selected;
        self.explorer
            .update(cx, |p, cx| p.set_rows(rows, selected, cx));
        cx.notify();
    }

    /// Selects `key`.
    pub fn select(&mut self, key: ExplorerKey, cx: &mut Context<Self>) {
        self.state.select(key);
        self.show_rows(cx);
    }

    /// Selects the first row of `sel` and opens its ancestors.
    pub fn reveal(&mut self, sel: Selection, cx: &mut Context<Self>) {
        self.state.reveal(&self.index, sel);
        self.show_rows(cx);
    }

    /// Expands or collapses `key`.
    pub fn toggle(&mut self, key: ExplorerKey, cx: &mut Context<Self>) {
        self.state.toggle(key);
        self.show_rows(cx);
    }

    /// Opens the tab of `key`, or shows it when it is open already. A
    /// system row expands or collapses instead.
    pub fn open(&mut self, key: ExplorerKey, window: &mut Window, cx: &mut Context<Self>) {
        let key = shared(key);
        if matches!(key, ExplorerKey::System(_) | ExplorerKey::Unassigned) {
            self.toggle(key, cx);
            return;
        }
        if !self.tabs.contains_key(&key) {
            let Some((title, lines)) = self.content(key) else {
                return;
            };
            let tab = if let ExplorerKey::Element(..) = key {
                let e = cx.new(|cx| ElementTab::new(title, None, lines, cx));
                let handle = panel_handle(e.clone());
                Tab {
                    handle,
                    body: Body::Element(e),
                }
            } else {
                let l = cx.new(|cx| ListTab::new(title, None, lines, cx));
                let handle = panel_handle(l.clone());
                Tab {
                    handle,
                    body: Body::List(l),
                }
            };
            self.tabs.insert(key, tab);
        }
        if let Some(handle) = self.tabs.get(&key).map(|t| t.handle.clone()) {
            self.show_tab(handle, window, cx);
        }
    }

    /// Adds `handle` to the center if it is not there, and shows it.
    fn show_tab(&mut self, handle: Arc<dyn BasePanelView>, window: &mut Window, cx: &mut App) {
        self.area.update(cx, |area, cx| {
            let id = handle.panel_id(cx);
            if area.panel(id).is_none() {
                area.add_panel_view(handle, DockPlacement::Center, None, window, cx);
            }
            area.select_panel(id, window, cx);
        });
    }

    /// The title and lines of the tab of `key`; `None` when the tree no
    /// longer has it.
    fn content(&self, key: ExplorerKey) -> Option<(SharedString, Vec<SharedString>)> {
        let project = self.workspace.project();
        let empty = || vec![SharedString::from(self.catalog.tr("workbench-list-empty"))];
        if key == PROBLEMS {
            let lines: Vec<_> = problems(&self.workspace)
                .into_iter()
                .map(|p| format!("{} {}", p.visual_id, p.text).into())
                .collect();
            let title = self.catalog.tr("explorer-problems").into();
            return Some((title, if lines.is_empty() { empty() } else { lines }));
        }
        let rows = all_rows(&self.index);
        let at = rows.iter().position(|r| r.key == key)?;
        let row = rows.get(at)?;
        if let ExplorerKey::Element(_, _, sel) = key {
            let vid = selection_visual_id(project, sel);
            let name = row
                .label
                .strip_prefix(&vid)
                .unwrap_or(&row.label)
                .trim_start();
            let parent = row.parent.and_then(|p| rows.iter().find(|r| r.key == p));
            let kind = kind(&self.names, sel)
                .or_else(|| parent.map(|p| p.label.clone()))
                .unwrap_or_default();
            let lines = [vid.clone(), name.to_owned(), kind].map(SharedString::from);
            return Some((vid.into(), lines.into()));
        }
        let lines: Vec<_> = rows[at + 1..]
            .iter()
            .take_while(|r| r.depth > row.depth)
            .filter(|r| matches!(r.key, ExplorerKey::Element(..)))
            .map(|r| SharedString::from(r.label.clone()))
            .collect();
        let title = row.label.clone().into();
        Some((title, if lines.is_empty() { empty() } else { lines }))
    }

    /// Rebuilds the tree and every open tab after the project changed.
    fn changed(&mut self, res: Result<(), String>, cx: &mut Context<Self>) {
        self.note = res.err();
        self.index = ExplorerIndex::build(&self.workspace, &self.names, &self.catalog, SAMPLE_NAME);
        let updates: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|(k, t)| Some((self.content(*k)?, &t.body)))
            .collect();
        for ((title, lines), body) in updates {
            match body {
                Body::Overview(_) => {}
                Body::List(e) => e.update(cx, |t, cx| t.set(title, None, lines, cx)),
                Body::Element(e) => e.update(cx, |t, cx| t.set(title, None, lines, cx)),
            }
        }
        self.show_rows(cx);
    }

    /// Drops `dragged` on `target`: moves a component to another system.
    pub fn drop_on(&mut self, dragged: ExplorerKey, target: ExplorerKey, cx: &mut Context<Self>) {
        let Some(edit) = drop_edit(self.workspace.project(), &dragged, &target) else {
            return;
        };
        let res = local::apply_edit(&mut self.workspace, &edit, &self.actor, now_ms());
        if res.is_ok() {
            self.picked = None;
        }
        self.changed(res, cx);
    }

    /// Lets go of the component picked up with the keyboard.
    pub fn let_go(&mut self, cx: &mut Context<Self>) {
        self.picked = None;
        cx.notify();
    }

    fn save_layout(&mut self, cx: &App) {
        let Some(path) = &self.settings_path else {
            return;
        };
        let layout = read_layout(self.area.read(cx), SAMPLE_KEY);
        if self.saved.as_ref() == Some(&layout) {
            return;
        }
        // Reload first: other code may have written other fields.
        let mut settings = Settings::load(path);
        settings.set_workbench_layout(layout.clone());
        if let Err(err) = settings.save(path) {
            eprintln!("could not save the layout: {err}");
        }
        self.saved = Some(layout);
    }

    fn rows_then(&mut self, cx: &mut Context<Self>, f: fn(&mut ExplorerState, &[ExplorerRow])) {
        let rows = self.index.rows(&self.state);
        f(&mut self.state, &rows);
        self.show_rows(cx);
    }

    fn pick_up(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.state.selected else {
            return;
        };
        if let ExplorerKey::Element(_, Folder::Components, Selection::Component(_)) = key {
            let label = self.row(key).map(|r| r.label).unwrap_or_default();
            self.picked = Some((key, label));
            cx.notify();
        }
    }

    /// Runs the command `id` of the command table; every key, menu item,
    /// toolbar button and palette pick ends here.
    pub fn run(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            "undo" | "redo" => {
                let step = if id == "undo" {
                    local::undo
                } else {
                    local::redo
                };
                let res = step(&mut self.workspace);
                self.changed(res, cx);
            }
            "toggle-explorer" => self.toggle_dock(DockPlacement::Left, window, cx),
            "toggle-inspector" => self.toggle_dock(DockPlacement::Right, window, cx),
            "toggle-bottom-dock" => self.toggle_dock(DockPlacement::Bottom, window, cx),
            "focus-explorer" => {
                self.open_dock(DockPlacement::Left, window, cx);
                window.focus(&gpui::Focusable::focus_handle(&self.explorer, cx), cx);
            }
            "filter-explorer" => {
                self.open_dock(DockPlacement::Left, window, cx);
                self.filter.update(cx, |f, cx| f.focus(window, cx));
            }
            "explorer-down" => self.rows_then(cx, ExplorerState::down),
            "explorer-up" => self.rows_then(cx, ExplorerState::up),
            "explorer-expand" => self.rows_then(cx, ExplorerState::right),
            "explorer-collapse" => self.rows_then(cx, ExplorerState::left),
            "open-selected" => {
                if let Some(key) = self.state.selected {
                    self.open(key, window, cx);
                }
            }
            "pick-up" => self.pick_up(cx),
            "drop-here" => {
                if let (Some((dragged, _)), Some(target)) =
                    (self.picked.clone(), self.state.selected)
                {
                    self.drop_on(dragged, target, cx);
                }
            }
            "close-tab" => self.close_tab(window, cx),
            "next-tab" => self.step_tab(1, window, cx),
            "previous-tab" => self.step_tab(-1, window, cx),
            "toggle-theme" => self.toggle_theme(window, cx),
            "reset-layout" => {
                let layout = default_layout(SAMPLE_KEY);
                self.area
                    .update(cx, |a, cx| apply_layout(a, &layout, window, cx));
                self.save_layout(cx);
            }
            "quit" => {
                self.save_layout(cx);
                cx.quit();
            }
            _ => {}
        }
    }

    fn toggle_dock(&mut self, placement: DockPlacement, window: &mut Window, cx: &mut App) {
        self.area
            .update(cx, |a, cx| a.toggle_dock(placement, window, cx));
    }

    fn open_dock(&mut self, placement: DockPlacement, window: &mut Window, cx: &mut App) {
        if !self.area.read(cx).is_dock_open(placement) {
            self.toggle_dock(placement, window, cx);
        }
    }

    /// The panels of the first center tab group and the shown one's index.
    fn center_tabs(&self, cx: &App) -> Option<(Vec<PanelId>, usize)> {
        let mut found = None;
        let tree = self.area.read(cx).layout(DockPlacement::Center)?;
        tree.root().walk(&mut |node| {
            if let PaneRef::Tabs { panels, active_ix } = node.kind()
                && found.is_none()
                && !panels.is_empty()
            {
                found = Some((panels.to_vec(), active_ix));
            }
        });
        found
    }

    /// Shows the tab `step` places right (negative: left), wrapping round.
    fn step_tab(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((panels, at)) = self.center_tabs(cx) else {
            return;
        };
        let n = panels.len().cast_signed();
        let next = (at.cast_signed() + step).rem_euclid(n).cast_unsigned();
        if let Some(id) = panels.get(next).copied() {
            self.area.update(cx, |a, cx| a.select_panel(id, window, cx));
        }
    }

    /// Closes the tab shown in the center.
    fn close_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self
            .center_tabs(cx)
            .and_then(|(panels, at)| panels.get(at).copied())
        else {
            return;
        };
        let area = &self.area;
        let tab = self.tabs.values().find(|t| t.handle.panel_id(cx) == id);
        match tab.map(|t| &t.body) {
            Some(Body::Overview(e)) => remove(area, e, window, cx),
            Some(Body::List(e)) => remove(area, e, window, cx),
            Some(Body::Element(e)) => remove(area, e, window, cx),
            None => {}
        }
    }

    /// Switches light and dark and remembers the choice.
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.appearance = if cx.theme().is_dark() {
            Appearance::Light
        } else {
            Appearance::Dark
        };
        apply_appearance(self.appearance, window, cx);
        if let Some(path) = &self.settings_path {
            let mut settings = Settings::load(path);
            settings.appearance = self.appearance.code().to_owned();
            if let Err(err) = settings.save(path) {
                eprintln!("could not save the appearance: {err}");
            }
        }
        cx.notify();
    }

    fn status_text(&self) -> SharedString {
        if let Some(note) = &self.note {
            return note.clone().into();
        }
        let Some((_, name)) = &self.picked else {
            return self.status.clone();
        };
        let mac = cfg!(target_os = "macos");
        let keys = command("drop-here")
            .and_then(|c| c.keys_for(mac))
            .map(|k| shortcut_label(&k, mac))
            .unwrap_or_default();
        let args = [("name", name.as_str()), ("keys", keys.as_str())];
        self.catalog.tr_with("workbench-picked", &args).into()
    }
}

/// Takes the panel `e` out of the dock area.
fn remove<P: BasePanel>(area: &Entity<DockArea>, e: &Entity<P>, window: &mut Window, cx: &mut App) {
    area.update(cx, |a, cx| a.remove_panel(e.clone(), window, cx));
}

/// The tab key of an explorer key: Root, Overview and Systems share the
/// Overview tab, every Problems folder the problems tab.
fn shared(key: ExplorerKey) -> ExplorerKey {
    match key {
        ExplorerKey::Root
        | ExplorerKey::Folder(_, Folder::Overview)
        | ExplorerKey::Folder(_, Folder::Systems) => OVERVIEW,
        ExplorerKey::Folder(_, Folder::Problems) => PROBLEMS,
        k => k,
    }
}

impl Render for Workbench {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.status_text();
        let root = on_every_command(div().key_context("Workbench"), cx);
        let theme = cx.theme();
        let bar = || {
            div()
                .flex()
                .items_center()
                .px_3()
                .bg(theme.title_bar)
                .border_color(theme.border)
        };
        root.relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                bar()
                    .h(px(36.))
                    .gap_1()
                    .border_b_1()
                    .child(div().flex_1().child(SAMPLE_NAME)),
            )
            .child(div().flex_1().min_h_0().child(self.area.clone()))
            .child(
                bar()
                    .h(px(24.))
                    .border_t_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(status),
            )
    }
}
