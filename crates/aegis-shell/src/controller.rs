//! Wires the window to the client core.
//!
//! Slint callbacks (user actions) become [`Command`]s; each new
//! [`ViewModel`] becomes Slint properties. The window reports *positions*
//! (a table row, a dropdown index); this module turns them into the stable
//! IDs of the rows it last showed, so a click always means the entity that
//! was on screen. Pan and zoom are the one thing the shell owns, because
//! they are about *this window* (its size and the user's hand), not about
//! the project: the [`Viewport`] lives here, and every change of it
//! re-renders the display list.
//!
//! All of this runs on the UI thread. The session runs elsewhere, so the
//! window never waits for the network. Before a server is chosen (or while
//! the user logs in to a development server) there is no session; the
//! [`Launcher`](crate::launcher::Launcher) drives the window then.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use aegis_canvas::{Direction, Point, RenderOptions, Size, Viewport, render};
use aegis_client::commands::{Command, Edit};
use aegis_client::report::ExportKind;
use aegis_client::session::Session;
use aegis_client::state::{ConnectionView, Mode, ProjectView, Severity, SyncView, ViewModel};
use aegis_client::view::{AdminAction, Page, RiskView, Selection};
use aegis_model::{AssetId, ClaimId, ControlId, DamageId, GoalId, ThreatId, TreeId};
use slint::{ComponentHandle, ModelRc, SharedString, StandardListViewItem, VecModel};

use crate::adapter::{bucket, draw_items};
use crate::{
    AppWindow, CheckItem, CountItem, DiffItem, FactorItem, LinkItem, MitigationItem, NoticeItem,
    ProblemItem, ProjectItem, ThreatItem,
};

/// Pixels the pointer must move before a press becomes a drag.
const DRAG_THRESHOLD: f32 = 4.0;

struct Press {
    last: Point,
    travelled: f32,
}

struct State {
    view: Option<Arc<ViewModel>>,
    viewport: Viewport,
    /// The tree on the canvas, to refit when another one is shown.
    shown_tree: Option<TreeId>,
    press: Option<Press>,
    /// The baseline row chosen on the versions page.
    baseline: Option<usize>,
    /// The user row chosen on the administration page.
    user: Option<usize>,
}

/// A password login request: user, password, keep the login.
pub type PasswordLogIn = Box<dyn Fn(String, String, bool)>;

/// What the window asks of the part of the app that owns the login (the
/// launcher). Each is called on the UI thread.
#[derive(Default)]
pub struct Hooks {
    /// Company login button.
    pub log_in: Option<Box<dyn Fn()>>,
    /// Connect to a server: address and transport position.
    pub connect_server: Option<Box<dyn Fn(String, i32)>>,
    /// Password login: user, password, keep the login.
    pub password_log_in: Option<PasswordLogIn>,
    /// Development login: user.
    pub dev_log_in: Option<Box<dyn Fn(String)>>,
    /// Go back to choosing a server.
    pub change_server: Option<Box<dyn Fn()>>,
    /// Switch from the company login to the password form.
    pub use_password: Option<Box<dyn Fn()>>,
    /// Switch from the password form back to the company login.
    pub use_company: Option<Box<dyn Fn()>>,
    /// The export folder changed.
    pub export_dir_changed: Option<Box<dyn Fn(String)>>,
}

/// Owns the window-side state and the session handle.
pub struct Controller {
    window: slint::Weak<AppWindow>,
    session: RefCell<Option<Session>>,
    state: RefCell<State>,
    hooks: Hooks,
}

fn s(text: &str) -> SharedString {
    text.into()
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

fn rows(rows: Vec<Vec<String>>) -> ModelRc<ModelRc<StandardListViewItem>> {
    model(
        rows.into_iter()
            .map(|r| {
                model(
                    r.iter()
                        .map(|c| StandardListViewItem::from(c.as_str()))
                        .collect(),
                )
            })
            .collect(),
    )
}

fn risk_text(r: Option<&RiskView>) -> String {
    r.map_or_else(|| "not rated".to_owned(), |v| v.label.clone())
}

fn tone(r: Option<&RiskView>) -> i32 {
    r.map_or(0, |v| bucket(v.tone))
}

fn index_of<T: PartialEq>(list: impl IntoIterator<Item = T>, wanted: Option<T>) -> i32 {
    wanted
        .and_then(|w| list.into_iter().position(|x| x == w))
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
}

fn pos(i: i32) -> Option<usize> {
    usize::try_from(i).ok()
}

/// A dropdown position where 0 means "none" and the rest are the list.
fn after_none(i: i32) -> Option<usize> {
    pos(i).and_then(|i| i.checked_sub(1))
}

fn with_none(first: &str, rest: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let mut v = vec![s(first)];
    v.extend(rest.into_iter().map(|x| s(&x)));
    model(v)
}

fn links(items: &[aegis_client::pages::LinkRow]) -> ModelRc<LinkItem> {
    model(
        items
            .iter()
            .map(|l| LinkItem {
                label: s(&l.label),
                detail: s(&l.detail),
                tone: 0,
            })
            .collect(),
    )
}

impl Controller {
    /// Connects `window` to `session`. `log_in` is called when the user
    /// presses the company login button.
    pub fn attach(
        window: &AppWindow,
        session: Session,
        log_in: impl Fn() + 'static,
    ) -> Rc<Controller> {
        Self::attach_with(
            window,
            Some(session),
            Hooks {
                log_in: Some(Box::new(log_in)),
                ..Hooks::default()
            },
        )
    }

    /// Connects `window`, with or without a session yet, and the launcher's
    /// hooks.
    pub fn attach_with(
        window: &AppWindow,
        session: Option<Session>,
        hooks: Hooks,
    ) -> Rc<Controller> {
        let controller = Rc::new(Controller {
            window: window.as_weak(),
            session: RefCell::new(session),
            state: RefCell::new(State {
                view: None,
                viewport: Viewport::new(Size::new(800.0, 600.0)),
                shown_tree: None,
                press: None,
                baseline: None,
                user: None,
            }),
            hooks,
        });
        controller.wire(window);
        controller
    }

    /// Replaces the session (a new server or login), or removes it.
    pub fn set_session(&self, session: Option<Session>) {
        if let Some(old) = self.session.replace(session) {
            let _ = old.send(Command::Shutdown);
        }
        let mut st = self.state.borrow_mut();
        st.view = None;
        st.shown_tree = None;
        st.baseline = None;
        st.user = None;
    }

    /// The session, if one runs.
    #[must_use]
    pub fn session(&self) -> Option<Session> {
        self.session.borrow().clone()
    }

    /// The current viewport, for tests that need to turn world positions
    /// into window positions.
    #[doc(hidden)]
    #[must_use]
    pub fn viewport_for_tests(&self) -> Viewport {
        self.state.borrow().viewport
    }

    fn send(&self, command: Command) {
        // A full queue means the session is far behind; dropping a click is
        // better than freezing the window.
        if let Some(session) = self.session.borrow().as_ref() {
            let _ = session.send(command);
        }
    }

    fn edit(&self, edit: Edit) {
        self.send(Command::Edit(edit));
    }

    fn view(&self) -> Option<Arc<ViewModel>> {
        self.state.borrow().view.clone()
    }

    /// Runs `f` with the open project of the last view model shown.
    fn with_project<R>(&self, f: impl FnOnce(&ProjectView) -> Option<R>) -> Option<R> {
        let view = self.view()?;
        f(view.project.as_ref()?)
    }

    fn select(&self, selection: Selection, navigate: bool) {
        if matches!(selection, Selection::Tree(_)) {
            self.state.borrow_mut().shown_tree = None;
        }
        self.send(Command::Select {
            selection,
            navigate,
        });
    }

    fn export(&self, kind: ExportKind) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let Some(name) = self.with_project(|p| Some(p.entry.name.clone())) else {
            return;
        };
        let dir = w.get_export_dir().to_string();
        if let Some(hook) = &self.hooks.export_dir_changed {
            hook(dir.clone());
        }
        let dir = if dir.trim().is_empty() {
            aegis_client::settings::default_export_dir()
        } else {
            std::path::PathBuf::from(dir.trim())
        };
        let what = match kind {
            ExportKind::Report => "tara-report",
            ExportKind::RiskRegister => "risk-register",
            ExportKind::TreeSvg => "attack-tree",
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
        let file = aegis_client::settings::export_file_name(&name, what, now, kind.extension());
        self.send(Command::Export {
            kind,
            path: dir.join(file),
        });
    }

    #[allow(clippy::too_many_lines)]
    fn wire(self: &Rc<Self>, w: &AppWindow) {
        macro_rules! on {
            ($setter:ident, |$c:ident $(, $arg:ident)*| $body:expr) => {{
                let $c = Rc::clone(self);
                w.$setter(move |$($arg),*| $body);
            }};
        }
        // ---- connection and account ----
        on!(on_retry, |c| c.send(Command::Reconnect));
        on!(on_log_in, |c| if let Some(h) = &c.hooks.log_in {
            h()
        });
        on!(
            on_connect_server,
            |c, url, kind| if let Some(h) = &c.hooks.connect_server {
                h(url.to_string(), kind);
            }
        );
        on!(on_password_log_in, |c, user, password, remember| {
            if let Some(h) = &c.hooks.password_log_in {
                h(user.to_string(), password.to_string(), remember);
            }
        });
        on!(
            on_dev_log_in,
            |c, user| if let Some(h) = &c.hooks.dev_log_in {
                h(user.to_string());
            }
        );
        on!(
            on_change_server,
            |c| if let Some(h) = &c.hooks.change_server {
                h()
            }
        );
        on!(
            on_use_password_login,
            |c| if let Some(h) = &c.hooks.use_password {
                h()
            }
        );
        on!(
            on_use_company_login,
            |c| if let Some(h) = &c.hooks.use_company {
                h()
            }
        );
        on!(on_set_password_login, |c, enabled| c.send(Command::Admin(
            AdminAction::SetPasswordLogin { enabled }
        )));
        on!(on_log_out, |c| c.send(Command::LogOut));
        on!(on_change_password, |c, current, new| c.send(
            Command::ChangePassword {
                current: current.to_string(),
                new: new.to_string(),
            }
        ));

        // ---- projects ----
        on!(on_open_project, |c, id| c
            .send(Command::OpenProject { id: id.to_string() }));
        on!(on_create_project, |c, name| c.send(
            Command::CreateProject {
                name: name.to_string(),
                methodology_key: "iso21434-r155".to_owned(),
            }
        ));
        on!(on_refresh_projects, |c| c.send(Command::Refresh));
        on!(on_close_project, |c| c.send(Command::CloseProject));
        on!(on_rename_project, |c, name| c
            .send(Command::RenameProject(name.to_string())));
        on!(on_delete_project, |c| {
            if let Some(id) = c.with_project(|p| Some(p.entry.id.clone())) {
                c.send(Command::DeleteProject(id));
            }
        });
        on!(
            on_show_page,
            |c, i| if let Some(page) = pos(i).and_then(Page::from_index) {
                c.send(Command::ShowPage(page));
            }
        );
        on!(on_begin_edit, |c| c.send(Command::BeginEdit));
        on!(on_end_edit, |c| c.send(Command::EndEdit));
        on!(on_undo, |c| c.send(Command::Undo));
        on!(on_redo, |c| c.send(Command::Redo));
        on!(on_create_sample, |c| c.send(Command::CreateSample));
        on!(on_set_residual, |c, on| c.send(Command::ShowResidual(on)));
        on!(on_dismiss_notice, |c, id| c.send(Command::DismissNotice(
            u64::try_from(id).unwrap_or(0)
        )));
        on!(on_ov_top_clicked, |c, i| {
            let id = c.with_project(|p| p.data.overview.as_ref()?.top.get(pos(i)?).map(|r| r.id));
            if let Some(id) = id {
                c.select(Selection::Threat(id), true);
            }
        });

        // ---- assets ----
        on!(on_pick_asset, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.assets.get(pos(i)?).map(|a| a.id)) {
                c.select(Selection::Asset(id), false);
            }
        });
        on!(on_add_asset, |c, name| {
            let all = c.with_project(|p| Some((0..p.vocab.properties.len()).collect()));
            c.edit(Edit::AddAsset {
                id: AssetId::new_v7(),
                name: name.to_string(),
                properties: all.unwrap_or_default(),
            });
        });
        on!(on_rename_asset, |c, name| {
            if let Some(asset) = c.selected_asset() {
                c.edit(Edit::RenameAsset {
                    asset,
                    name: name.to_string(),
                });
            }
        });
        on!(on_toggle_asset_property, |c, i, on| {
            if let (Some(asset), Some(property)) = (c.selected_asset(), pos(i)) {
                c.edit(Edit::SetAssetProperty {
                    asset,
                    property,
                    on,
                });
            }
        });
        on!(
            on_remove_asset,
            |c| if let Some(asset) = c.selected_asset() {
                c.edit(Edit::RemoveAsset { asset });
            }
        );
        on!(on_asset_threat_clicked, |c, i| {
            let t = c.with_project(|p| {
                p.data
                    .asset
                    .as_ref()?
                    .threats
                    .get(pos(i)?)
                    .map(|l| l.target)
            });
            if let Some(t) = t {
                c.select(t, true);
            }
        });

        // ---- damage ----
        on!(on_pick_damage, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.damages.get(pos(i)?).map(|d| d.id)) {
                c.select(Selection::Damage(id), false);
            }
        });
        on!(on_add_damage, |c, name| c.edit(Edit::AddDamage {
            id: DamageId::new_v7(),
            name: name.to_string(),
        }));
        on!(
            on_rename_damage,
            |c, name| if let Some(damage) = c.selected_damage() {
                c.edit(Edit::RenameDamage {
                    damage,
                    name: name.to_string(),
                });
            }
        );
        on!(on_set_impact, |c, category, i| {
            if let (Some(damage), Some(category)) = (c.selected_damage(), pos(category)) {
                c.edit(Edit::SetImpact {
                    damage,
                    category,
                    level: after_none(i),
                });
            }
        });
        on!(
            on_remove_damage,
            |c| if let Some(damage) = c.selected_damage() {
                c.edit(Edit::RemoveDamage { damage });
            }
        );
        on!(on_damage_threat_clicked, |c, i| {
            let t = c.with_project(|p| {
                p.data
                    .damage
                    .as_ref()?
                    .threats
                    .get(pos(i)?)
                    .map(|l| l.target)
            });
            if let Some(t) = t {
                c.select(t, true);
            }
        });

        // ---- threats ----
        on!(on_select_threat, |c, i| {
            c.state.borrow_mut().shown_tree = None;
            c.send(Command::SelectThreat(usize::try_from(i).unwrap_or(0)));
        });
        on!(on_add_threat, |c, name, asset, category| {
            let asset = c.with_project(|p| p.data.assets.get(pos(asset)?).map(|a| a.id));
            if let (Some(asset), Some(category)) = (asset, pos(category)) {
                c.edit(Edit::AddThreat {
                    id: ThreatId::new_v7(),
                    name: name.to_string(),
                    asset,
                    category,
                });
            }
        });
        on!(
            on_rename_threat,
            |c, name| if let Some(threat) = c.selected_threat() {
                c.edit(Edit::RenameThreat {
                    threat,
                    name: name.to_string(),
                });
            }
        );
        on!(on_set_threat_category, |c, i| {
            if let (Some(threat), Some(category)) = (c.selected_threat(), pos(i)) {
                c.edit(Edit::SetThreatCategory { threat, category });
            }
        });
        on!(on_toggle_threat_damage, |c, i, linked| {
            let damage =
                c.with_project(|p| p.data.threat.as_ref()?.damages.get(pos(i)?).map(|d| d.id));
            if let (Some(threat), Some(damage)) = (c.selected_threat(), damage) {
                c.edit(Edit::LinkDamage {
                    threat,
                    damage,
                    linked,
                });
            }
        });
        on!(on_add_catalog_ref, |c, scheme, text| {
            if let (Some(threat), Some(scheme)) = (c.selected_threat(), pos(scheme)) {
                c.edit(Edit::AddCatalogRef {
                    threat,
                    scheme,
                    text: text.to_string(),
                });
            }
        });
        on!(on_remove_catalog_ref, |c, i| {
            if let (Some(threat), Some(index)) = (c.selected_threat(), pos(i)) {
                c.edit(Edit::RemoveCatalogRef { threat, index });
            }
        });
        on!(
            on_set_threat_feasibility,
            |c, i| if let Some(threat) = c.selected_threat() {
                c.edit(Edit::SetThreatFeasibility {
                    threat,
                    level: after_none(i),
                });
            }
        );
        on!(on_set_threat_tree, |c, i| {
            let tree = c.with_project(|p| {
                let d = p.data.threat.as_ref()?;
                Some(after_none(i).and_then(|i| d.trees.get(i)).map(|t| t.id))
            });
            if let (Some(threat), Some(tree)) = (c.selected_threat(), tree) {
                c.edit(Edit::SetThreatTree { threat, tree });
            }
        });
        on!(on_new_tree_for_threat, |c| {
            let detail = c.with_project(|p| p.data.threat.as_ref().map(|d| (d.id, d.name.clone())));
            if let Some((threat, name)) = detail {
                c.edit(Edit::NewTree {
                    id: TreeId::new_v7(),
                    root: name,
                    threat: Some(threat),
                });
                c.send(Command::ShowPage(Page::Trees));
            }
        });
        on!(on_open_threat_tree, |c| {
            if let Some(tree) = c.with_project(|p| p.data.threat.as_ref()?.tree) {
                c.select(Selection::Tree(tree), true);
            }
        });
        on!(
            on_remove_threat,
            |c| if let Some(threat) = c.selected_threat() {
                c.edit(Edit::RemoveThreat { threat });
            }
        );
        on!(
            on_set_treatment,
            |c, i| if let Some(threat) = c.selected_threat() {
                c.edit(Edit::SetTreatment {
                    threat,
                    treatment: after_none(i),
                });
            }
        );
        on!(on_set_threat_goal, |c, i| {
            let goal = c.with_project(|p| {
                let d = p.data.threat.as_ref()?;
                Some(after_none(i).and_then(|i| d.goals.get(i)).map(|g| g.id))
            });
            if let (Some(threat), Some(goal)) = (c.selected_threat(), goal) {
                c.edit(Edit::SetThreatGoal { threat, goal });
            }
        });
        on!(on_set_threat_claim, |c, i| {
            let claim = c.with_project(|p| {
                let d = p.data.threat.as_ref()?;
                Some(after_none(i).and_then(|i| d.claims.get(i)).map(|g| g.id))
            });
            if let (Some(threat), Some(claim)) = (c.selected_threat(), claim) {
                c.edit(Edit::SetThreatClaim { threat, claim });
            }
        });
        on!(on_pick_risk, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.risks.get(pos(i)?).map(|r| r.id)) {
                c.select(Selection::Threat(id), false);
            }
        });

        // ---- trees ----
        on!(on_pick_tree, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.trees.get(pos(i)?).map(|t| t.id)) {
                c.select(Selection::Tree(id), false);
            }
        });
        on!(on_new_tree, |c, name| c.edit(Edit::NewTree {
            id: TreeId::new_v7(),
            root: name.to_string(),
            threat: None,
        }));
        on!(
            on_apply_outline,
            |c, text| if let Some(tree) = c.shown_tree() {
                c.edit(Edit::ReplaceTree {
                    tree,
                    outline: text.to_string(),
                });
            }
        );
        on!(on_remove_tree, |c| if let Some(tree) = c.shown_tree() {
            c.edit(Edit::RemoveTree { tree });
        });
        on!(on_attach_control, |c, ci, level| {
            let target = c.with_project(|p| {
                let i = p.inspector.as_ref()?;
                Some((i.tree, i.node, i.attachable.get(pos(ci)?)?.0))
            });
            if let Some((tree, node, control)) = target {
                c.edit(Edit::SetStepMitigation {
                    tree,
                    node,
                    control,
                    level: pos(level),
                });
            }
        });
        on!(on_set_mitigation, |c, mx, level| {
            let target = c.with_project(|p| {
                let i = p.inspector.as_ref()?;
                Some((i.tree, i.node, i.mitigations.get(pos(mx)?)?.control))
            });
            if let Some((tree, node, control)) = target {
                c.edit(Edit::SetStepMitigation {
                    tree,
                    node,
                    control,
                    level: pos(level),
                });
            }
        });
        on!(on_detach_control, |c, mx| {
            let target = c.with_project(|p| {
                let i = p.inspector.as_ref()?;
                Some((i.tree, i.node, i.mitigations.get(pos(mx)?)?.control))
            });
            if let Some((tree, node, control)) = target {
                c.edit(Edit::SetStepMitigation {
                    tree,
                    node,
                    control,
                    level: None,
                });
            }
        });
        on!(on_set_rating, |c, index| c.set_rating(index));
        on!(on_set_factor, |c, factor, index| c
            .set_factor(factor, index));

        // ---- controls, goals, claims ----
        on!(on_pick_control, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.controls.get(pos(i)?).map(|x| x.id)) {
                c.select(Selection::Control(id), false);
            }
        });
        on!(on_add_control, |c, name| c.edit(Edit::AddControl {
            id: ControlId::new_v7(),
            name: name.to_string(),
        }));
        on!(
            on_rename_control,
            |c, name| if let Some(control) = c.selected_control() {
                c.edit(Edit::RenameControl {
                    control,
                    name: name.to_string(),
                });
            }
        );
        on!(
            on_remove_control,
            |c| if let Some(control) = c.selected_control() {
                c.edit(Edit::RemoveControl { control });
            }
        );
        on!(on_control_step_clicked, |c, i| {
            let t = c.with_project(|p| {
                p.data
                    .control
                    .as_ref()?
                    .steps
                    .get(pos(i)?)
                    .map(|l| l.target)
            });
            if let Some(t) = t {
                c.select(t, true);
            }
        });
        on!(on_control_goal_clicked, |c, i| {
            let t = c.with_project(|p| {
                p.data
                    .control
                    .as_ref()?
                    .goals
                    .get(pos(i)?)
                    .map(|l| l.target)
            });
            if let Some(t) = t {
                c.select(t, true);
            }
        });
        on!(on_pick_goal, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.goals.get(pos(i)?).map(|x| x.id)) {
                c.select(Selection::Goal(id), false);
            }
        });
        on!(on_add_goal, |c, name| c.edit(Edit::AddGoal {
            id: GoalId::new_v7(),
            name: name.to_string(),
        }));
        on!(
            on_rename_goal,
            |c, name| if let Some(goal) = c.selected_goal() {
                c.edit(Edit::RenameGoal {
                    goal,
                    name: name.to_string(),
                });
            }
        );
        on!(on_toggle_goal_control, |c, i, linked| {
            let control =
                c.with_project(|p| p.data.goal.as_ref()?.controls.get(pos(i)?).map(|x| x.id));
            if let (Some(goal), Some(control)) = (c.selected_goal(), control) {
                c.edit(Edit::LinkGoalControl {
                    goal,
                    control,
                    linked,
                });
            }
        });
        on!(on_remove_goal, |c| if let Some(goal) = c.selected_goal() {
            c.edit(Edit::RemoveGoal { goal });
        });
        on!(on_goal_threat_clicked, |c, i| {
            let t =
                c.with_project(|p| p.data.goal.as_ref()?.threats.get(pos(i)?).map(|l| l.target));
            if let Some(t) = t {
                c.select(t, true);
            }
        });
        on!(on_pick_claim, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.claims.get(pos(i)?).map(|x| x.id)) {
                c.select(Selection::Claim(id), false);
            }
        });
        on!(on_add_claim, |c, name| c.edit(Edit::AddClaim {
            id: ClaimId::new_v7(),
            name: name.to_string(),
        }));
        on!(
            on_rename_claim,
            |c, name| if let Some(claim) = c.selected_claim() {
                c.edit(Edit::RenameClaim {
                    claim,
                    name: name.to_string(),
                });
            }
        );
        on!(
            on_remove_claim,
            |c| if let Some(claim) = c.selected_claim() {
                c.edit(Edit::RemoveClaim { claim });
            }
        );
        on!(on_claim_threat_clicked, |c, i| {
            let t = c.with_project(|p| {
                p.data
                    .claim
                    .as_ref()?
                    .threats
                    .get(pos(i)?)
                    .map(|l| l.target)
            });
            if let Some(t) = t {
                c.select(t, true);
            }
        });

        // ---- problems, versions, history, reports ----
        on!(on_problem_clicked, |c, i| {
            if let Some(t) = c.with_project(|p| p.data.problems.get(pos(i)?)?.target) {
                c.select(t, true);
            }
        });
        on!(on_pick_baseline, |c, i| {
            c.state.borrow_mut().baseline = pos(i);
            if let Some(w) = c.window.upgrade() {
                w.set_baseline_selected(i);
            }
        });
        on!(on_create_baseline, |c, name| c
            .send(Command::CreateBaseline(name.to_string())));
        on!(on_compare_baseline, |c| {
            let chosen = c.state.borrow().baseline;
            let id = c.with_project(|p| p.baselines.get(chosen?).map(|b| b.id.clone()));
            if let Some(id) = id {
                c.send(Command::CompareWith(Some(id)));
            }
        });
        on!(on_stop_compare, |c| c.send(Command::CompareWith(None)));
        on!(on_refresh_history, |c| c.send(Command::LoadHistory));
        on!(on_export_report, |c| c.export(ExportKind::Report));
        on!(on_export_csv, |c| c.export(ExportKind::RiskRegister));
        on!(on_export_svg, |c| c.export(ExportKind::TreeSvg));

        // ---- administration ----
        on!(on_pick_user, |c, i| {
            c.state.borrow_mut().user = pos(i);
            c.apply_users();
        });
        on!(on_create_user, |c,
                             username,
                             display_name,
                             password,
                             admin| {
            c.send(Command::Admin(AdminAction::CreateUser {
                username: username.to_string(),
                display_name: display_name.to_string(),
                password: password.to_string(),
                admin,
            }));
        });
        on!(
            on_toggle_user_admin,
            |c| if let Some(u) = c.selected_user() {
                c.send(Command::Admin(AdminAction::SetAdmin {
                    username: u.username,
                    admin: !u.admin,
                }));
            }
        );
        on!(
            on_toggle_user_disabled,
            |c| if let Some(u) = c.selected_user() {
                c.send(Command::Admin(AdminAction::SetDisabled {
                    username: u.username,
                    disabled: !u.disabled,
                }));
            }
        );
        on!(
            on_reset_user_password,
            |c, password| if let Some(u) = c.selected_user() {
                c.send(Command::Admin(AdminAction::ResetPassword {
                    username: u.username,
                    password: password.to_string(),
                }));
            }
        );
        on!(on_delete_user, |c| if let Some(u) = c.selected_user() {
            c.state.borrow_mut().user = None;
            c.send(Command::Admin(AdminAction::DeleteUser {
                username: u.username,
            }));
        });
        on!(on_break_lock, |c| c.send(Command::BreakLock));
        on!(on_verify_audit, |c| c.send(Command::VerifyAudit));

        // ---- canvas ----
        on!(on_canvas_resized, |c, width, height| c
            .resize(width, height));
        on!(on_canvas_down, |c, x, y| {
            c.state.borrow_mut().press = Some(Press {
                last: Point::new(x, y),
                travelled: 0.0,
            });
        });
        on!(on_canvas_move, |c, x, y| c.drag(Point::new(x, y)));
        on!(on_canvas_up, |c, x, y| c.release(Point::new(x, y)));
        on!(on_canvas_scroll, |c, x, y, delta| c
            .zoom(Point::new(x, y), delta));
        on!(on_canvas_key, |c, text, shift| c.key(&text, shift));
        on!(on_fit, |c| c.fit());
        on!(on_zoom_reset, |c| c.reset_zoom());
    }

    // ---- what is selected -------------------------------------------------

    fn selected_asset(&self) -> Option<AssetId> {
        self.with_project(|p| p.data.asset.as_ref().map(|a| a.id))
    }

    fn selected_damage(&self) -> Option<DamageId> {
        self.with_project(|p| p.data.damage.as_ref().map(|d| d.id))
    }

    fn selected_threat(&self) -> Option<ThreatId> {
        self.with_project(|p| p.data.threat.as_ref().map(|t| t.id))
    }

    fn selected_control(&self) -> Option<ControlId> {
        self.with_project(|p| p.data.control.as_ref().map(|c| c.id))
    }

    fn selected_goal(&self) -> Option<GoalId> {
        self.with_project(|p| p.data.goal.as_ref().map(|g| g.id))
    }

    fn selected_claim(&self) -> Option<ClaimId> {
        self.with_project(|p| p.data.claim.as_ref().map(|c| c.id))
    }

    fn shown_tree(&self) -> Option<TreeId> {
        self.with_project(|p| p.tree.as_ref().map(|t| t.tree))
    }

    fn selected_user(&self) -> Option<aegis_client::state::UserRow> {
        let i = self.state.borrow().user?;
        self.view()?.users.get(i).cloned()
    }

    // ---- edits from the inspector ---------------------------------------

    fn set_rating(&self, index: i32) {
        let target = self.with_project(|p| p.inspector.as_ref().map(|i| (i.tree, i.node)));
        if let Some((tree, node)) = target {
            // Index 0 is "Not rated"; the levels follow.
            self.edit(Edit::SetStepFeasibility {
                tree,
                node,
                level: after_none(index),
            });
        }
    }

    fn set_factor(&self, factor: i32, index: i32) {
        let target = self.with_project(|p| p.inspector.as_ref().map(|i| (i.tree, i.node)));
        if let (Some((tree, node)), Some(factor), Some(index)) = (target, pos(factor), pos(index)) {
            self.edit(Edit::SetStepFactor {
                tree,
                node,
                factor,
                level: index.checked_sub(1),
            });
        }
    }

    // ---- canvas interaction ---------------------------------------------

    fn resize(&self, width: f32, height: f32) {
        if width < 1.0 || height < 1.0 {
            return;
        }
        {
            let mut st = self.state.borrow_mut();
            if st.viewport.size == Size::new(width, height) {
                return;
            }
            st.viewport.size = Size::new(width, height);
            // A canvas that just appeared (the trees page opened) refits.
            st.shown_tree = None;
        }
        if let Some(view) = self.view() {
            self.apply_canvas(&view);
        }
    }

    fn drag(&self, at: Point) {
        {
            let mut st = self.state.borrow_mut();
            let Some(press) = st.press.as_mut() else {
                return;
            };
            let (dx, dy) = (at.x - press.last.x, at.y - press.last.y);
            press.last = at;
            press.travelled += dx.abs() + dy.abs();
            if press.travelled <= DRAG_THRESHOLD {
                return;
            }
            st.viewport.pan_by_screen(dx, dy);
        }
        self.redraw();
    }

    fn release(&self, at: Point) {
        let click = {
            let mut st = self.state.borrow_mut();
            st.press
                .take()
                .is_some_and(|p| p.travelled <= DRAG_THRESHOLD)
        };
        if !click {
            return;
        }
        let (scene, world) = {
            let st = self.state.borrow();
            let scene = st
                .view
                .as_ref()
                .and_then(|v| v.project.as_ref())
                .and_then(|p| p.scene().cloned());
            (scene, st.viewport.to_world(at))
        };
        if let Some(scene) = scene {
            let hit = scene.hit_test(world).map(aegis_model::NodeIdx::index);
            self.send(Command::SelectNode(hit));
        }
    }

    fn zoom(&self, at: Point, delta: f32) {
        // One wheel notch (about 120) is a 20% change.
        let factor = 1.0015f32.powf(delta).clamp(0.5, 2.0);
        self.state.borrow_mut().viewport.zoom_at(at, factor);
        self.redraw();
    }

    fn fit(&self) {
        let scene = self.current_scene();
        if let Some(scene) = scene {
            self.state.borrow_mut().viewport.fit(scene.bounds(), 32.0);
        }
        self.redraw();
    }

    fn reset_zoom(&self) {
        let mut st = self.state.borrow_mut();
        let centre = Point::new(st.viewport.size.w / 2.0, st.viewport.size.h / 2.0);
        let factor = 1.0 / st.viewport.zoom;
        st.viewport.zoom_at(centre, factor);
        drop(st);
        self.redraw();
    }

    fn current_scene(&self) -> Option<Arc<aegis_canvas::Scene>> {
        self.state
            .borrow()
            .view
            .as_ref()
            .and_then(|v| v.project.as_ref())
            .and_then(|p| p.scene().cloned())
    }

    /// Handles a key on the canvas. Returns whether it was used.
    fn key(&self, text: &str, shift: bool) -> bool {
        use slint::platform::Key;
        let key = |k: Key| text == char::from(k).to_string();
        let direction = if key(Key::LeftArrow) {
            Some(Direction::Left)
        } else if key(Key::RightArrow) {
            Some(Direction::Right)
        } else if key(Key::UpArrow) {
            Some(Direction::Up)
        } else if key(Key::DownArrow) {
            Some(Direction::Down)
        } else {
            None
        };
        if let Some(direction) = direction {
            return self.navigate(direction);
        }
        if key(Key::Escape) {
            self.send(Command::SelectNode(None));
            return true;
        }
        match text {
            "+" | "=" => {
                self.zoom_centre(1.2);
                true
            }
            "-" | "_" => {
                self.zoom_centre(1.0 / 1.2);
                true
            }
            "0" => {
                self.reset_zoom();
                true
            }
            "f" | "F" if !shift || text == "F" => {
                self.fit();
                true
            }
            _ => false,
        }
    }

    fn zoom_centre(&self, factor: f32) {
        {
            let mut st = self.state.borrow_mut();
            let centre = Point::new(st.viewport.size.w / 2.0, st.viewport.size.h / 2.0);
            st.viewport.zoom_at(centre, factor);
        }
        self.redraw();
    }

    fn navigate(&self, direction: Direction) -> bool {
        let Some(view) = self.view() else {
            return false;
        };
        let Some(project) = view.project.as_ref() else {
            return false;
        };
        let Some(scene) = project.scene() else {
            return false;
        };
        // With nothing selected, an arrow key selects the root.
        let target = match project.selected_node {
            Some(from) => scene.navigate(from, direction),
            None => scene.root(),
        };
        let Some(target) = target else { return true };
        if let Some(rect) = scene.node_rect(target) {
            self.state.borrow_mut().viewport.reveal(rect, 40.0);
        }
        self.send(Command::SelectNode(Some(target.index())));
        self.redraw();
        true
    }

    // ---- view model -> window -------------------------------------------

    /// Applies the session's latest view model to the window. Call it on the
    /// UI thread whenever the view model changes. Does nothing without a
    /// session (the launcher shows its own screens then).
    pub fn apply_latest(&self) {
        let Some(view) = self.session.borrow().as_ref().map(Session::view) else {
            return;
        };
        self.apply(&view);
    }

    #[allow(clippy::too_many_lines)]
    fn apply(&self, view: &Arc<ViewModel>) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        self.state.borrow_mut().view = Some(Arc::clone(view));

        let (code, text) = match &view.connection {
            ConnectionView::Connecting => (0, "Connecting to the server…".to_owned()),
            ConnectionView::Online { .. } => (1, String::new()),
            ConnectionView::Offline { reason } => {
                (2, format!("The server cannot be reached. {reason}"))
            }
            ConnectionView::LoginRequired => (
                3,
                match w.get_login_method() {
                    1 => "Log in with your user name and password.".to_owned(),
                    2 => "Enter a user name.".to_owned(),
                    _ => "Please log in with your company account.".to_owned(),
                },
            ),
        };
        w.set_connection(code);
        w.set_connection_text(s(&text));
        w.set_user(s(&view.user));
        w.set_is_admin(view.me.as_ref().is_some_and(|m| m.admin));
        w.set_is_password(view.me.as_ref().is_some_and(|m| m.method == "password"));
        w.set_engine_text(s(&match &view.connection {
            ConnectionView::Online {
                server_version,
                engine_ok,
                license,
            } => {
                let mut text = format!(
                    "Server {server_version}, engine {}",
                    aegis_model::ENGINE_VERSION
                );
                if !engine_ok {
                    text.push_str(". Engine differs: update AegisTARA");
                }
                if let Some(license) = license {
                    text.push_str(&format!(
                        " \u{b7} Licensed to {} \u{b7} {} of {} seats",
                        license.customer, license.seats_used, license.seats
                    ));
                }
                text
            }
            _ => String::new(),
        }));
        w.set_projects(model(
            view.projects
                .iter()
                .map(|p| ProjectItem {
                    id: s(&p.id),
                    name: s(&p.name),
                    methodology: s(&p.methodology),
                })
                .collect(),
        ));
        w.set_notices(model(
            view.notices
                .iter()
                .map(|n| NoticeItem {
                    id: i32::try_from(n.id).unwrap_or(i32::MAX),
                    severity: match n.severity {
                        Severity::Info => 0,
                        Severity::Warning => 1,
                        Severity::Error => 2,
                    },
                    text: s(&n.text),
                })
                .collect(),
        ));
        w.set_admin_text(s(view.admin_text.as_deref().unwrap_or("")));
        w.set_settings_known(view.settings.is_some());
        if let Some(settings) = view.settings {
            w.set_password_login_on(settings.password_login_enabled);
            w.set_company_login_available(settings.company_login_available);
        }
        self.apply_users();

        let Some(project) = view.project.as_ref() else {
            w.set_has_project(false);
            w.set_draw(model(Vec::new()));
            self.state.borrow_mut().shown_tree = None;
            return;
        };
        w.set_has_project(true);
        w.set_project_name(s(&project.entry.name));
        w.set_editing(project.mode == Mode::Editing);
        let me = view
            .me
            .as_ref()
            .map_or(view.user.as_str(), |m| m.subject.as_str());
        let (kind, band) = match (&project.mode, project.lock_holder.as_deref(), project.sync) {
            (_, _, SyncView::Resyncing) => (1, "Reloading the project from the server…".to_owned()),
            (Mode::Editing, _, _) => (
                2,
                "You're editing. Others on the project see your changes live.".to_owned(),
            ),
            (Mode::ReadOnly, Some(holder), _) if holder != me => (
                1,
                format!("Read-only. {holder} is editing; their changes appear here live."),
            ),
            (Mode::ReadOnly, _, _) => {
                (0, "Read-only. Press Edit to change the project.".to_owned())
            }
        };
        w.set_band_kind(kind);
        w.set_band_text(s(&band));
        w.set_sync_text(s(&match project.sync {
            SyncView::Synced => match project.risks_verified_seq {
                Some(_) => "Saved \u{b7} numbers checked with the server".to_owned(),
                None => "Saved".to_owned(),
            },
            SyncView::Syncing { pending: 1 } => "Saving 1 change…".to_owned(),
            SyncView::Syncing { pending } => format!("Saving {pending} changes…"),
            SyncView::Resyncing => "Reloading…".to_owned(),
        }));
        w.set_can_undo(project.can_undo);
        w.set_can_redo(project.can_redo);
        w.set_show_residual(project.show_residual);
        w.set_is_empty(project.is_empty);
        w.set_page(i32::try_from(project.page.index()).unwrap_or(0));
        w.set_nav_counts(model(
            project
                .nav_counts
                .iter()
                .map(|c| c.map_or_else(SharedString::new, |n| s(&n.to_string())))
                .collect(),
        ));
        self.apply_vocabulary(&w, project);
        self.apply_threats(&w, project);
        self.apply_pages(&w, project);
        self.apply_canvas(view);
    }

    fn apply_vocabulary(&self, w: &AppWindow, p: &ProjectView) {
        let v = &p.vocab;
        w.set_property_labels(model(v.properties.iter().map(|x| s(x)).collect()));
        w.set_threat_categories(model(v.threat_categories.iter().map(|x| s(x)).collect()));
        w.set_feasibility_options(with_none("Not rated", v.feasibility.iter().cloned()));
        w.set_feasibility_levels(model(v.feasibility.iter().map(|x| s(x)).collect()));
        w.set_treatment_options(with_none("Not decided", v.treatments.iter().cloned()));
        w.set_catalog_schemes(model(v.catalog_schemes.iter().map(|x| s(x)).collect()));
    }

    fn apply_threats(&self, w: &AppWindow, p: &ProjectView) {
        w.set_threats(model(
            p.threats
                .iter()
                .enumerate()
                .map(|(i, t)| ThreatItem {
                    name: s(&t.name),
                    asset: s(&t.asset),
                    inherent: s(t.inherent.as_ref().map_or("", |r| r.label.as_str())),
                    residual: s(t.residual.as_ref().map_or("", |r| r.label.as_str())),
                    inherent_tone: tone(t.inherent.as_ref()),
                    residual_tone: tone(t.residual.as_ref()),
                    selected: p.selected_threat == Some(i),
                })
                .collect(),
        ));
        w.set_threat_rows(rows(
            p.threats
                .iter()
                .map(|t| {
                    vec![
                        t.name.clone(),
                        t.asset.clone(),
                        risk_text(t.inherent.as_ref()),
                        risk_text(t.residual.as_ref()),
                    ]
                })
                .collect(),
        ));
        w.set_threat_selected(
            p.selected_threat
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        let d = p.data.threat.as_ref();
        w.set_has_threat_detail(d.is_some());
        let Some(d) = d else {
            return;
        };
        w.set_td_name(s(&d.name));
        w.set_td_asset(s(&d.asset_name));
        w.set_td_category(i32::try_from(d.category).unwrap_or(0));
        w.set_td_damages(model(
            d.damages
                .iter()
                .map(|x| CheckItem {
                    label: s(&x.name),
                    checked: x.linked,
                })
                .collect(),
        ));
        w.set_td_refs(model(d.catalog_refs.iter().map(|x| s(x)).collect()));
        let plus_one = |x: Option<usize>| x.map_or(0, |i| i32::try_from(i + 1).unwrap_or(0));
        w.set_td_feasibility(plus_one(d.direct_feasibility));
        w.set_tree_names(with_none("None", d.trees.iter().map(|t| t.name.clone())));
        w.set_td_tree(plus_one(
            d.tree.and_then(|t| d.trees.iter().position(|x| x.id == t)),
        ));
        w.set_td_feasibility_text(s(d.feasibility.as_deref().unwrap_or("")));
        w.set_td_inherent(s(d.inherent.as_ref().map_or("", |r| r.label.as_str())));
        w.set_td_residual(s(d.residual.as_ref().map_or("", |r| r.label.as_str())));
        w.set_td_inherent_tone(tone(d.inherent.as_ref()));
        w.set_td_residual_tone(tone(d.residual.as_ref()));
        w.set_td_why(model(d.why.iter().map(|x| s(x)).collect()));
        w.set_td_changed(s(&d.changed));
        w.set_td_treatment(plus_one(d.treatment));
        w.set_goal_names(with_none("None", d.goals.iter().map(|g| g.name.clone())));
        w.set_td_goal(plus_one(
            d.goal.and_then(|g| d.goals.iter().position(|x| x.id == g)),
        ));
        w.set_claim_names(with_none("None", d.claims.iter().map(|g| g.name.clone())));
        w.set_td_claim(plus_one(
            d.claim
                .and_then(|g| d.claims.iter().position(|x| x.id == g)),
        ));
    }

    #[allow(clippy::too_many_lines)]
    fn apply_pages(&self, w: &AppWindow, p: &ProjectView) {
        let d = &p.data;
        // Overview.
        if let Some(ov) = &d.overview {
            let counts = |list: &[aegis_client::pages::Count]| {
                model(
                    list.iter()
                        .map(|c| CountItem {
                            label: s(&c.label),
                            count: i32::try_from(c.count).unwrap_or(i32::MAX),
                            tone: bucket(c.tone),
                        })
                        .collect(),
                )
            };
            w.set_ov_counts(counts(&ov.counts));
            w.set_ov_inherent(counts(&ov.inherent));
            w.set_ov_residual(counts(&ov.residual));
            let most = ov
                .inherent
                .iter()
                .chain(&ov.residual)
                .map(|c| c.count)
                .max()
                .unwrap_or(1)
                .max(1);
            w.set_ov_most(i32::try_from(most).unwrap_or(1));
            w.set_ov_errors(i32::try_from(ov.errors).unwrap_or(0));
            w.set_ov_warnings(i32::try_from(ov.warnings).unwrap_or(0));
            w.set_ov_top(model(
                ov.top
                    .iter()
                    .map(|r| LinkItem {
                        label: s(&r.threat),
                        detail: s(&format!(
                            "{} before, {} after",
                            risk_text(r.inherent.as_ref()),
                            risk_text(r.residual.as_ref())
                        )),
                        tone: tone(r.inherent.as_ref()),
                    })
                    .collect(),
            ));
        }

        // Assets (also the asset choices of the threats page).
        w.set_asset_names(model(d.assets.iter().map(|a| s(&a.name)).collect()));
        w.set_asset_rows(rows(
            d.assets
                .iter()
                .map(|a| {
                    let props: Vec<&str> = a
                        .properties
                        .iter()
                        .zip(&p.vocab.properties)
                        .filter(|(on, _)| **on)
                        .map(|(_, l)| l.as_str())
                        .collect();
                    vec![
                        a.name.clone(),
                        props.join(", "),
                        a.threats.to_string(),
                        a.worst
                            .as_ref()
                            .map_or_else(String::new, |r| r.label.clone()),
                    ]
                })
                .collect(),
        ));
        w.set_asset_selected(index_of(
            d.assets.iter().map(|a| a.id),
            d.asset.as_ref().map(|a| a.id),
        ));
        w.set_has_asset(d.asset.is_some());
        if let Some(a) = &d.asset {
            w.set_asset_name(s(&a.name));
            w.set_asset_properties(model(
                a.properties
                    .iter()
                    .zip(&p.vocab.properties)
                    .map(|(on, label)| CheckItem {
                        label: s(label),
                        checked: *on,
                    })
                    .collect(),
            ));
            w.set_asset_threats(links(&a.threats));
            w.set_asset_changed(s(&a.changed));
        }

        // Damage.
        let impact_text = |impacts: &[Option<usize>]| {
            impacts
                .iter()
                .zip(&p.vocab.impact_categories)
                .filter_map(|(l, c)| Some(format!("{}: {}", c.label, c.levels.get((*l)?)?)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        w.set_damage_rows(rows(
            d.damages
                .iter()
                .map(|x| {
                    vec![
                        x.name.clone(),
                        impact_text(&x.impacts),
                        x.threats.to_string(),
                    ]
                })
                .collect(),
        ));
        w.set_damage_selected(index_of(
            d.damages.iter().map(|x| x.id),
            d.damage.as_ref().map(|x| x.id),
        ));
        w.set_has_damage(d.damage.is_some());
        if let Some(x) = &d.damage {
            w.set_damage_name(s(&x.name));
            w.set_damage_impacts(model(
                x.impacts
                    .iter()
                    .zip(&p.vocab.impact_categories)
                    .map(|(level, c)| FactorItem {
                        label: s(&c.label),
                        options: with_none("Not rated", c.levels.iter().cloned()),
                        selected: level.map_or(0, |l| i32::try_from(l + 1).unwrap_or(0)),
                    })
                    .collect(),
            ));
            w.set_damage_threats(links(&x.threats));
            w.set_damage_changed(s(&x.changed));
        }

        // Risks.
        w.set_risk_rows(rows(
            d.risks
                .iter()
                .map(|r| {
                    vec![
                        r.threat.clone(),
                        r.damage.clone(),
                        r.impact.clone(),
                        r.feasibility.clone(),
                        risk_text(r.inherent.as_ref()),
                        risk_text(r.residual.as_ref()),
                        r.treatment.clone(),
                    ]
                })
                .collect(),
        ));
        w.set_risk_selected(index_of(
            d.risks.iter().map(|r| r.id),
            d.threat.as_ref().map(|t| t.id),
        ));

        // Trees.
        w.set_tree_rows(rows(
            d.trees
                .iter()
                .map(|t| vec![t.title.clone(), t.nodes.to_string(), t.feasibility.clone()])
                .collect(),
        ));
        w.set_tree_selected(index_of(
            d.trees.iter().map(|t| t.id),
            p.tree.as_ref().map(|t| t.tree),
        ));
        w.set_outline_text(s(d.outline.as_deref().unwrap_or("")));

        // Controls.
        w.set_control_rows(rows(
            d.controls
                .iter()
                .map(|c| vec![c.name.clone(), c.steps.to_string(), c.goals.to_string()])
                .collect(),
        ));
        w.set_control_selected(index_of(
            d.controls.iter().map(|c| c.id),
            d.control.as_ref().map(|c| c.id),
        ));
        w.set_has_control(d.control.is_some());
        if let Some(c) = &d.control {
            w.set_control_name(s(&c.name));
            w.set_control_steps(links(&c.steps));
            w.set_control_goals(links(&c.goals));
            w.set_control_changed(s(&c.changed));
        }

        // Goals and claims.
        w.set_goal_rows(rows(
            d.goals
                .iter()
                .map(|g| {
                    vec![
                        g.name.clone(),
                        g.controls.to_string(),
                        g.threats.to_string(),
                    ]
                })
                .collect(),
        ));
        w.set_goal_selected(index_of(
            d.goals.iter().map(|g| g.id),
            d.goal.as_ref().map(|g| g.id),
        ));
        w.set_has_goal(d.goal.is_some());
        if let Some(g) = &d.goal {
            w.set_goal_name(s(&g.name));
            w.set_goal_controls(model(
                g.controls
                    .iter()
                    .map(|c| CheckItem {
                        label: s(&c.name),
                        checked: c.linked,
                    })
                    .collect(),
            ));
            w.set_goal_threats(links(&g.threats));
        }
        w.set_claim_rows(rows(
            d.claims
                .iter()
                .map(|c| vec![c.name.clone(), c.threats.to_string()])
                .collect(),
        ));
        w.set_claim_selected(index_of(
            d.claims.iter().map(|c| c.id),
            d.claim.as_ref().map(|c| c.id),
        ));
        w.set_has_claim(d.claim.is_some());
        if let Some(c) = &d.claim {
            w.set_claim_name(s(&c.name));
            w.set_claim_threats(links(&c.threats));
        }

        // Problems.
        w.set_problems(model(
            d.problems
                .iter()
                .map(|x| ProblemItem {
                    error: x.error,
                    text: s(&x.text),
                })
                .collect(),
        ));

        // Versions and history.
        w.set_baseline_rows(rows(
            p.baselines
                .iter()
                .map(|b| {
                    vec![
                        b.name.clone(),
                        b.seq.to_string(),
                        b.created_by.clone(),
                        b.created.clone(),
                    ]
                })
                .collect(),
        ));
        {
            let mut st = self.state.borrow_mut();
            if st.baseline.is_some_and(|i| i >= p.baselines.len()) {
                st.baseline = None;
            }
            w.set_baseline_selected(
                st.baseline
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1),
            );
        }
        w.set_comparing(p.comparison.is_some());
        if let Some(c) = &p.comparison {
            w.set_compare_summary(s(&format!(
                "Compared with \"{}\". {}",
                c.baseline, c.summary
            )));
            w.set_diff_lines(model(
                c.lines
                    .iter()
                    .map(|l| DiffItem {
                        kind: s(&l.kind),
                        text: s(&l.text),
                    })
                    .collect(),
            ));
        }
        w.set_history_rows(rows(
            p.history
                .iter()
                .map(|h| {
                    vec![
                        h.seq.to_string(),
                        h.actor.clone(),
                        h.when.clone(),
                        h.kind.clone(),
                    ]
                })
                .collect(),
        ));
        w.set_last_export(s(p.last_export.as_deref().unwrap_or("")));
    }

    fn apply_users(&self) {
        let (Some(w), Some(view)) = (self.window.upgrade(), self.view()) else {
            return;
        };
        w.set_user_rows(rows(
            view.users
                .iter()
                .map(|u| {
                    vec![
                        u.username.clone(),
                        u.display_name.clone(),
                        if u.admin { "yes" } else { "" }.to_owned(),
                        if u.disabled { "disabled" } else { "active" }.to_owned(),
                        u.last_login.clone(),
                        u.sessions.to_string(),
                    ]
                })
                .collect(),
        ));
        let mut st = self.state.borrow_mut();
        if st.user.is_some_and(|i| i >= view.users.len()) {
            st.user = None;
        }
        let chosen = st.user.and_then(|i| view.users.get(i));
        w.set_user_selected(st.user.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
        w.set_selected_user_admin(chosen.is_some_and(|u| u.admin));
        w.set_selected_user_disabled(chosen.is_some_and(|u| u.disabled));
    }

    /// The tree, its title, and the inspector.
    fn apply_canvas(&self, view: &Arc<ViewModel>) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let Some(project) = view.project.as_ref() else {
            return;
        };
        let how = if project.show_residual {
            "after controls"
        } else {
            "before controls"
        };
        let title = project.tree.as_ref().map(|t| {
            let root = t
                .scene
                .root()
                .map_or("", |r| t.scene.label(u32::try_from(r.index()).unwrap_or(0)))
                .to_owned();
            let threat = t
                .threat
                .and_then(|id| project.threats.iter().find(|r| r.id == id))
                .map(|r| r.name.clone());
            match threat {
                Some(name) if name != root => format!("{name} \u{b7} {root} \u{b7} {how}"),
                _ => format!("{root} \u{b7} {how}"),
            }
        });
        w.set_tree_title(s(title.as_deref().unwrap_or("")));
        w.set_has_tree(project.tree.is_some());
        w.set_canvas_summary(s(&match (&project.tree, &project.inspector) {
            (Some(t), Some(i)) => format!(
                "Attack tree with {} nodes. Selected: {}.",
                t.scene.len(),
                i.title
            ),
            (Some(t), None) => format!("Attack tree with {} nodes.", t.scene.len()),
            _ => "No attack tree".to_owned(),
        }));

        match project.inspector.as_ref() {
            Some(i) => {
                w.set_has_inspector(true);
                w.set_inspector_title(s(&i.title));
                w.set_inspector_kind(s(&i.kind));
                w.set_inspector_is_step(i.is_step);
                w.set_level_options(with_none(
                    "Not rated",
                    i.levels.iter().map(|c| c.label.clone()),
                ));
                w.set_rating_index(i.rating.map_or(0, |r| i32::try_from(r + 1).unwrap_or(0)));
                w.set_effective_text(s(i.effective.as_deref().unwrap_or("")));
                w.set_potential_text(s(i.potential.as_deref().unwrap_or("")));
                w.set_evaluated_text(s(i.evaluated.as_deref().unwrap_or("")));
                w.set_evaluated_tone(evaluated_tone(project, i.evaluated.is_some()));
                w.set_why_lines(model(i.why.iter().map(|l| s(l)).collect()));
                w.set_factors(model(
                    i.factors
                        .iter()
                        .map(|f| FactorItem {
                            label: s(&f.label),
                            options: with_none(
                                "Not rated",
                                f.options.iter().map(|c| c.label.clone()),
                            ),
                            selected: f.selected.map_or(0, |x| i32::try_from(x + 1).unwrap_or(0)),
                        })
                        .collect(),
                ));
                w.set_mitigations(model(
                    i.mitigations
                        .iter()
                        .map(|m| MitigationItem {
                            name: s(&m.name),
                            level: i32::try_from(m.level).unwrap_or(0),
                        })
                        .collect(),
                ));
                w.set_attach_names(model(i.attachable.iter().map(|(_, n)| s(n)).collect()));
                if pos(w.get_attach_control_index()).is_none_or(|x| x >= i.attachable.len()) {
                    w.set_attach_control_index(0);
                }
                w.set_step_residual_text(s(i.residual.as_deref().unwrap_or("")));
            }
            None => w.set_has_inspector(false),
        }

        // A tree that was not on the canvas before is fitted to the window.
        let tree_id = project.tree.as_ref().map(|t| t.tree);
        {
            let mut st = self.state.borrow_mut();
            if tree_id != st.shown_tree {
                st.shown_tree = tree_id;
                if let Some(t) = &project.tree {
                    st.viewport.fit(t.scene.bounds(), 32.0);
                }
            }
        }
        self.redraw();
    }

    /// Re-renders the display list for the current view model and viewport.
    fn redraw(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let st = self.state.borrow();
        let Some(project) = st.view.as_ref().and_then(|v| v.project.as_ref()) else {
            return;
        };
        let items = match &project.tree {
            Some(tree) => {
                let options = RenderOptions {
                    selected: project.selected_node,
                };
                draw_items(&render(&tree.scene, &st.viewport, &options))
            }
            None => Vec::new(),
        };
        w.set_draw(model(items));
        w.set_zoom_text(s(&format!("{:.0}%", st.viewport.zoom * 100.0)));
    }
}

/// The bucket of the selected node's evaluated rating, for the swatch.
fn evaluated_tone(project: &ProjectView, has_value: bool) -> i32 {
    if !has_value {
        return 0;
    }
    let (Some(tree), Some(selected)) = (project.tree.as_ref(), project.selected_node) else {
        return 0;
    };
    tree.scene
        .style(u32::try_from(selected.index()).unwrap_or(0))
        .map_or(0, |style| bucket(style.tone))
}
