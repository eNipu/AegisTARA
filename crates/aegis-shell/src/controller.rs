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
use aegis_client::audit::EventGroup;
use aegis_client::commands::{Command, Edit, GATES};
use aegis_client::library::{InsertMode, InsertOptions, LibraryView};
use aegis_client::report::ExportKind;
use aegis_client::session::Session;
use aegis_client::state::{
    ConnectionView, Mode, NoticeAction, NumbersCheck, ProjectView, Severity, SyncView, ViewModel,
};
use aegis_client::view::{AdminAction, DraftKind, Page, RiskView, Selection};
use aegis_model::library::LibraryPack;
use aegis_model::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, GoalId, InterfaceId, NodeId,
    ThreatId, TreeId, TrustBoundaryId,
};
use slint::{ComponentHandle, ModelRc, SharedString, StandardListViewItem, VecModel};

use crate::adapter::{bucket, draw_items, draw_layers};
use crate::{
    AccessNode, AppWindow, CheckItem, CountItem, DescriptionItem, DiffItem, FactorItem, LinkItem,
    MitigationItem, NoticeItem, ProblemItem, ProjectItem, SuggestionItem, ThreatItem,
    TranslationItem,
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
    /// The event log filter chosen on the administration page.
    event_group: Option<EventGroup>,
    /// The session row chosen on the administration page (job 055).
    session: Option<usize>,
    /// The membership row chosen on the Access tab (job 058).
    membership: Option<usize>,
    /// The figure row chosen on the reports page (job 073).
    figure: Option<usize>,
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
    /// Create a standalone project folder: folder and license paths.
    pub new_project_folder: Option<Box<dyn Fn(String, String)>>,
    /// Open a standalone project folder: folder and license paths.
    pub open_project_folder: Option<Box<dyn Fn(String, String)>>,
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
    /// The appearance was chosen: `0` System, `1` Light, `2` Dark.
    pub appearance_changed: Option<Box<dyn Fn(i32)>>,
}

/// Owns the window-side state and the session handle.
pub struct Controller {
    window: slint::Weak<AppWindow>,
    session: RefCell<Option<Session>>,
    state: RefCell<State>,
    /// The Library panel's search, narrowing and pick (job 057).
    library: RefCell<LibraryView>,
    /// The command palette's filtered entries (job 081).
    palette: RefCell<Vec<crate::shortcuts::PaletteEntry>>,
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

/// Adds the draft's row at the end of `list` when `drafting`, with its text in
/// the name column; returns its position.
fn push_draft_row(
    list: &mut Vec<Vec<String>>,
    drafting: bool,
    cols: usize,
    text: String,
) -> Option<i32> {
    if !drafting {
        return None;
    }
    let mut row = vec![String::new(); cols];
    row[1] = text;
    list.push(row);
    i32::try_from(list.len() - 1).ok()
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
                label: if l.visual_id.is_empty() {
                    s(&l.label)
                } else {
                    s(&format!("{} \u{b7} {}", l.visual_id, l.label))
                },
                detail: s(&l.detail),
                tone: 0,
            })
            .collect(),
    )
}

/// The five assignable per-project role keys, least to most capable (job 058, ADR 0061).
const ROLE_KEYS: [&str; 5] = [
    "viewer",
    "reviewer",
    "analyst",
    "project_owner",
    "methodology_owner",
];

/// The shown label for a role key.
fn role_label(key: &str) -> &str {
    match key {
        "viewer" => "Viewer",
        "reviewer" => "Reviewer",
        "analyst" => "Analyst",
        "project_owner" => "Project owner",
        "methodology_owner" => "Methodology owner",
        "admin" => "Admin",
        _ => key,
    }
}

/// The dropdown position of a role key, or -1 when unknown.
fn role_index(key: &str) -> i32 {
    ROLE_KEYS
        .iter()
        .position(|k| *k == key)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
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
        // Platform fidelity (job 113): the one shortcut table drives every
        // menu accelerator, and the native font families are applied before
        // the first frame.
        crate::shortcuts::install(window);
        crate::platform::apply_fonts(window);
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
                event_group: None,
                session: None,
                membership: None,
                figure: None,
            }),
            library: RefCell::new(LibraryView::new(Arc::new(LibraryPack::builtin_demo()))),
            palette: RefCell::new(Vec::new()),
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
        if navigate && let Some(w) = self.window.upgrade() {
            match selection {
                Selection::Component(_) => w.set_item_tab(0),
                Selection::Interface(_) => w.set_item_tab(1),
                Selection::DataFlow(_) => w.set_item_tab(2),
                Selection::TrustBoundary(_) => w.set_item_tab(3),
                _ => {}
            }
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
            ExportKind::ReportPdf => "tara-report",
            ExportKind::ReportXlsx => "tara-workbook",
            ExportKind::RiskRegister => "risk-register",
            ExportKind::TreeSvg => "attack-tree",
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
        let file = aegis_client::settings::export_file_name(&name, what, now, kind.extension());
        // The report profile is edited on the Reports page; the settings
        // store holds the active profile and the session records it in the
        // export audit event (RPT-02 to RPT-04).
        let profile = aegis_client::report::ReportProfile::default();
        self.send(Command::Export {
            kind,
            path: dir.join(file),
            profile: Box::new(profile),
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
        on!(on_new_project_folder, |c, folder, license| {
            if let Some(h) = &c.hooks.new_project_folder {
                h(folder.to_string(), license.to_string());
            }
        });
        on!(on_open_project_folder, |c, folder, license| {
            if let Some(h) = &c.hooks.open_project_folder {
                h(folder.to_string(), license.to_string());
            }
        });
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
        on!(on_set_event_filter, |c, which| {
            c.state.borrow_mut().event_group = match which {
                1 => Some(EventGroup::Logins),
                2 => Some(EventGroup::Exports),
                3 => Some(EventGroup::AdminActions),
                _ => None,
            };
            if let Some(w) = c.window.upgrade() {
                w.set_event_filter(which);
            }
            c.apply_events();
        });
        on!(on_canvas_node_activated, |c, node| {
            let view = c.view();
            if let Ok(n) = usize::try_from(node)
                && let Some(project) = view.as_ref().and_then(|v| v.project.as_ref())
                && let Some(scene) = project.scene()
                && let Some(idx) = scene.node_idx(u32::try_from(n).unwrap_or(u32::MAX))
            {
                if let Some(rect) = scene.node_rect(idx) {
                    c.state.borrow_mut().viewport.reveal(rect, 40.0);
                }
                c.send(Command::SelectNode(Some(idx.index())));
                c.redraw();
            }
        });
        on!(on_refresh_events, |c| c.send(Command::LoadEvents));
        on!(on_check_for_updates, |c| c.send(Command::CheckForUpdates));
        on!(on_download_update, |c| c.send(Command::DownloadUpdate));
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
        on!(on_create_project, |c, name| {
            let methodology_key = match c
                .window
                .upgrade()
                .map(|w| w.get_new_project_methodology())
                .unwrap_or(0)
            {
                1 => "eu-cra",
                2 => "stride-services-cloud",
                _ => "iso21434-r155",
            };
            c.send(Command::CreateProject {
                name: name.to_string(),
                methodology_key: methodology_key.to_owned(),
            });
        });
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
        on!(on_reload, |c| c.send(Command::Reload));
        on!(on_begin_edit, |c| c.send(Command::BeginEdit));
        on!(on_end_edit, |c| c.send(Command::EndEdit));
        on!(on_undo, |c| c.send(Command::Undo));
        on!(on_redo, |c| c.send(Command::Redo));
        on!(on_create_sample, |c| c.send(Command::CreateSample));
        on!(on_set_residual, |c, on| c.send(Command::ShowResidual(on)));
        on!(on_appearance_selected, |c, i| {
            if let Some(hook) = &c.hooks.appearance_changed {
                hook(i);
            }
        });
        on!(on_dismiss_notice, |c, id| c.send(Command::DismissNotice(
            u64::try_from(id).unwrap_or(0)
        )));
        on!(on_ov_top_clicked, |c, i| {
            let id = c.with_project(|p| p.data.overview.as_ref()?.top.get(pos(i)?).map(|r| r.id));
            if let Some(id) = id {
                c.select(Selection::Threat(id), true);
            }
        });

        // ---- item definition (job 074) ----
        on!(on_pick_component, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.components.get(pos(i)?).map(|x| x.id)) {
                c.select(Selection::Component(id), false);
            }
        });
        on!(on_add_component, |c, name| c.edit(Edit::AddComponent {
            id: ComponentId::new_v7(),
            name: name.to_string(),
            parent: None,
        }));
        on!(on_rename_component, |c, name| {
            if let Some(component) = c.selected_component() {
                c.edit(Edit::RenameComponent {
                    component,
                    name: name.to_string(),
                });
            }
        });
        on!(on_set_component_parent, |c, i| {
            let parent = c.with_project(|p| {
                let d = p.data.component.as_ref()?;
                Some(
                    after_none(i)
                        .and_then(|idx| d.parents.get(idx))
                        .map(|x| x.id),
                )
            });
            if let (Some(component), Some(parent)) = (c.selected_component(), parent) {
                c.edit(Edit::SetComponentParent { component, parent });
            }
        });
        on!(on_remove_component, |c| {
            if let Some(component) = c.selected_component() {
                c.edit(Edit::RemoveComponent { component });
            }
        });
        on!(on_pick_interface, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.interfaces.get(pos(i)?).map(|x| x.id)) {
                c.select(Selection::Interface(id), false);
            }
        });
        on!(on_add_interface, |c, name, comp, kind| {
            let component = c.with_project(|p| p.data.components.get(pos(comp)?).map(|x| x.id));
            if let (Some(component), Some(kind)) = (component, pos(kind)) {
                c.edit(Edit::AddInterface {
                    id: InterfaceId::new_v7(),
                    component,
                    name: name.to_string(),
                    kind,
                });
            }
        });
        on!(on_rename_interface, |c, name| {
            if let Some(interface) = c.selected_interface() {
                c.edit(Edit::RenameInterface {
                    interface,
                    name: name.to_string(),
                });
            }
        });
        on!(on_set_interface_kind, |c, i| {
            if let (Some(interface), Some(kind)) = (c.selected_interface(), pos(i)) {
                c.edit(Edit::SetInterfaceKind { interface, kind });
            }
        });
        on!(on_remove_interface, |c| {
            if let Some(interface) = c.selected_interface() {
                c.edit(Edit::RemoveInterface { interface });
            }
        });
        on!(on_pick_data_flow, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.data_flows.get(pos(i)?).map(|x| x.id)) {
                c.select(Selection::DataFlow(id), false);
            }
        });
        on!(on_add_data_flow, |c, name, from, to| {
            let pair = c.with_project(|p| {
                let from_id = p.data.components.get(pos(from)?)?.id;
                let to_id = p.data.components.get(pos(to)?)?.id;
                Some((from_id, to_id))
            });
            if let Some((from, to)) = pair {
                c.edit(Edit::AddDataFlow {
                    id: DataFlowId::new_v7(),
                    name: name.to_string(),
                    from,
                    to,
                    interface: None,
                });
            }
        });
        on!(on_rename_data_flow, |c, name| {
            if let Some(data_flow) = c.selected_data_flow() {
                c.edit(Edit::RenameDataFlow {
                    data_flow,
                    name: name.to_string(),
                });
            }
        });
        on!(on_set_data_flow_interface, |c, i| {
            let interface = c.with_project(|p| {
                let d = p.data.data_flow.as_ref()?;
                Some(
                    after_none(i)
                        .and_then(|idx| d.interfaces.get(idx))
                        .map(|x| x.id),
                )
            });
            if let (Some(data_flow), Some(interface)) = (c.selected_data_flow(), interface) {
                c.edit(Edit::SetDataFlowInterface {
                    data_flow,
                    interface,
                });
            }
        });
        on!(on_remove_data_flow, |c| {
            if let Some(data_flow) = c.selected_data_flow() {
                c.edit(Edit::RemoveDataFlow { data_flow });
            }
        });
        on!(on_pick_trust_boundary, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.trust_boundaries.get(pos(i)?).map(|x| x.id))
            {
                c.select(Selection::TrustBoundary(id), false);
            }
        });
        on!(on_add_trust_boundary, |c, name| c.edit(
            Edit::AddTrustBoundary {
                id: TrustBoundaryId::new_v7(),
                name: name.to_string(),
            }
        ));
        on!(on_rename_trust_boundary, |c, name| {
            if let Some(boundary) = c.selected_trust_boundary() {
                c.edit(Edit::RenameTrustBoundary {
                    boundary,
                    name: name.to_string(),
                });
            }
        });
        on!(on_toggle_trust_boundary_member, |c, i, member| {
            let component = c.with_project(|p| {
                p.data
                    .trust_boundary
                    .as_ref()?
                    .members
                    .get(pos(i)?)
                    .map(|m| m.id)
            });
            if let (Some(boundary), Some(component)) = (c.selected_trust_boundary(), component) {
                c.edit(Edit::SetTrustBoundaryMember {
                    boundary,
                    component,
                    member,
                });
            }
        });
        on!(on_remove_trust_boundary, |c| {
            if let Some(boundary) = c.selected_trust_boundary() {
                c.edit(Edit::RemoveTrustBoundary { boundary });
            }
        });

        // ---- assets ----
        on!(on_pick_asset, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.assets.get(pos(i)?).map(|a| a.id)) {
                c.select(Selection::Asset(id), false);
            }
        });
        on!(on_begin_draft, |c, kind| {
            if let Some(kind) = DraftKind::from_key(&kind) {
                c.send(Command::BeginDraft(kind));
            }
        });
        on!(on_commit_draft, |c, name, next| c.send(
            Command::CommitDraft {
                name: name.to_string(),
                next,
            }
        ));
        on!(on_cancel_draft, |c| c.send(Command::CancelDraft));
        on!(on_set_draft_asset, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.assets.get(pos(i)?).map(|a| a.id)) {
                c.send(Command::SetDraftAsset(id));
            }
        });
        on!(on_set_draft_component, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.components.get(pos(i)?).map(|x| x.id)) {
                c.send(Command::SetDraftComponent(id));
            }
        });
        on!(on_set_draft_to, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.components.get(pos(i)?).map(|x| x.id)) {
                c.send(Command::SetDraftTo(id));
            }
        });
        on!(on_set_draft_interface_kind, |c, i| {
            if let Some(i) = pos(i) {
                c.send(Command::SetDraftInterfaceKind(i));
            }
        });
        on!(on_set_draft_category, |c, i| {
            if let Some(i) = pos(i) {
                c.send(Command::SetDraftCategory(i));
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
        on!(on_set_description, |c, index, text| {
            let target = c.with_project(|p| {
                usize::try_from(index)
                    .ok()
                    .and_then(|i| p.descriptions.get(i))
                    .map(|d| d.target)
            });
            if let Some(target) = target {
                c.edit(Edit::SetDescription {
                    target,
                    text: text.to_string(),
                });
            }
        });
        on!(on_set_description_translation, |c, index, lang, text| {
            let target = c.with_project(|p| {
                usize::try_from(index)
                    .ok()
                    .and_then(|i| p.descriptions.get(i))
                    .map(|d| d.target)
            });
            if let Some(target) = target {
                c.edit(Edit::SetDescriptionTranslation {
                    target,
                    lang: lang.to_string(),
                    text: text.to_string(),
                });
            }
        });
        on!(on_remove_description_translation, |c, index, lang| {
            let target = c.with_project(|p| {
                usize::try_from(index)
                    .ok()
                    .and_then(|i| p.descriptions.get(i))
                    .map(|d| d.target)
            });
            if let Some(target) = target {
                c.edit(Edit::SetDescriptionTranslation {
                    target,
                    lang: lang.to_string(),
                    text: String::new(),
                });
            }
        });
        on!(on_rename_asset, |c, name| {
            if let Some(asset) = c.selected_asset() {
                c.edit(Edit::RenameAsset {
                    asset,
                    name: name.to_string(),
                });
            }
        });
        on!(on_set_asset_component, |c, i| {
            let component = c.with_project(|p| {
                let d = p.data.asset.as_ref()?;
                Some(
                    after_none(i)
                        .and_then(|idx| d.components.get(idx))
                        .map(|x| x.id),
                )
            });
            if let (Some(asset), Some(component)) = (c.selected_asset(), component) {
                c.edit(Edit::SetAssetComponent { asset, component });
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
        on!(on_mark_reviewed, |c| {
            if let Some((tree, node)) =
                c.with_project(|p| p.inspector.as_ref().map(|i| (i.tree, i.node)))
            {
                c.edit(Edit::MarkReviewed { tree, node });
            }
        });
        on!(on_library_opened, |c| c.show_library());
        on!(on_library_query, |c, text| {
            c.library.borrow_mut().set_query(&text);
            c.show_library();
        });
        on!(on_library_facet, |c, i| {
            if let Some(i) = pos(i) {
                c.library.borrow_mut().toggle_facet(i);
                c.show_library();
            }
        });
        on!(on_library_clear_filters, |c| {
            c.library.borrow_mut().clear_filters();
            c.show_library();
        });
        on!(on_library_pick, |c, i| {
            c.library.borrow_mut().pick(pos(i));
            c.show_library();
        });
        on!(on_library_pick_catalog, |c, i| {
            c.library.borrow_mut().pick_catalog(pos(i));
            c.show_library();
        });
        on!(on_library_insert, |c, mode, ratings, share| {
            c.insert_library_item(mode, ratings, share);
        });
        on!(on_remove_tree, |c| if let Some(tree) = c.shown_tree() {
            c.edit(Edit::RemoveTree { tree });
        });
        on!(on_rename_tree_node, |c, name| {
            if let Some((tree, node)) =
                c.with_project(|p| p.inspector.as_ref().map(|i| (i.tree, i.node)))
            {
                c.edit(Edit::RenameTreeNode {
                    tree,
                    node,
                    label: name.to_string(),
                });
            }
        });
        on!(on_set_tree_node_gate, |c, i| {
            let target = c.with_project(|p| {
                let ins = p.inspector.as_ref()?;
                let gate = *GATES.get(pos(i)?)?;
                Some((ins.tree, ins.node, gate))
            });
            if let Some((tree, node, gate)) = target {
                c.edit(Edit::SetTreeNodeGate { tree, node, gate });
            }
        });
        on!(on_add_tree_child, |c, kind, label| {
            let target = c.with_project(|p| {
                let ins = p.inspector.as_ref()?;
                let gate = after_none(kind).and_then(|k| GATES.get(k).copied());
                Some((ins.tree, ins.node, gate))
            });
            if let Some((tree, parent, gate)) = target {
                c.edit(Edit::AddTreeChild {
                    tree,
                    parent,
                    id: NodeId::new_v7(),
                    gate,
                    label: label.to_string(),
                });
            }
        });
        on!(on_branch_step_to_gate, |c, gate, label| {
            let target = c.with_project(|p| {
                let ins = p.inspector.as_ref()?;
                let gate = *GATES.get(pos(gate)?)?;
                Some((ins.tree, ins.node, gate))
            });
            if let Some((tree, node, gate)) = target {
                c.edit(Edit::BranchStepToGate {
                    tree,
                    node,
                    id: NodeId::new_v7(),
                    gate,
                    child_label: label.to_string(),
                });
            }
        });
        on!(on_insert_parent_gate, |c, gate, label| {
            let target = c.with_project(|p| {
                let ins = p.inspector.as_ref()?;
                let gate = *GATES.get(pos(gate)?)?;
                Some((ins.tree, ins.node, gate))
            });
            if let Some((tree, node, gate)) = target {
                c.edit(Edit::InsertParentGate {
                    tree,
                    node,
                    id: NodeId::new_v7(),
                    gate,
                    label: label.to_string(),
                });
            }
        });
        on!(on_add_sibling_node, |c, kind, label| {
            let target = c.with_project(|p| {
                let ins = p.inspector.as_ref()?;
                let gate = after_none(kind).and_then(|k| GATES.get(k).copied());
                Some((ins.tree, ins.node, gate))
            });
            if let Some((tree, node, gate)) = target {
                c.edit(Edit::AddSiblingNode {
                    tree,
                    node,
                    id: NodeId::new_v7(),
                    gate,
                    label: label.to_string(),
                });
            }
        });
        on!(on_move_tree_node, |c, delta| {
            if let Some((tree, node)) =
                c.with_project(|p| p.inspector.as_ref().map(|i| (i.tree, i.node)))
            {
                c.edit(Edit::MoveTreeNode {
                    tree,
                    node,
                    delta: isize::try_from(delta).unwrap_or(0),
                });
            }
        });
        on!(on_remove_tree_node, |c| {
            if let Some((tree, node)) =
                c.with_project(|p| p.inspector.as_ref().map(|i| (i.tree, i.node)))
            {
                c.edit(Edit::RemoveTreeNode { tree, node });
            }
        });
        on!(on_suggest_tree_nodes, |c| c.send(Command::SuggestTreeNodes));
        on!(on_accept_suggestion, |c, i| {
            if let Some(index) = pos(i) {
                c.send(Command::AcceptSuggestion(index));
            }
        });
        on!(on_accept_all_suggestions, |c| {
            c.send(Command::AcceptAllSuggestions);
        });
        on!(on_reject_suggestion, |c, i| {
            if let Some(index) = pos(i) {
                c.send(Command::RejectSuggestion(index));
            }
        });
        on!(on_reject_all_suggestions, |c| {
            c.send(Command::RejectAllSuggestions);
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

        // ---- row actions (UI-09) ----
        // + starts a draft with the selected row as the parent context (the
        // client core fills the context from the selection, job 114).
        on!(on_add_child_component, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.components.get(pos(i)?).map(|x| x.id)) {
                c.select(Selection::Component(id), false);
                c.send(Command::BeginDraft(DraftKind::Asset));
            }
        });
        on!(on_add_child_asset, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.assets.get(pos(i)?).map(|a| a.id)) {
                c.select(Selection::Asset(id), false);
                c.send(Command::BeginDraft(DraftKind::Threat));
            }
        });
        on!(on_add_child_damage, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.damages.get(pos(i)?).map(|d| d.id)) {
                c.select(Selection::Damage(id), false);
                c.send(Command::BeginDraft(DraftKind::Threat));
            }
        });
        on!(on_add_child_threat, |c, i| {
            if let Some(id) = c.with_project(|p| p.threats.get(pos(i)?).map(|t| t.id)) {
                c.select(Selection::Threat(id), false);
                c.send(Command::BeginDraft(DraftKind::Tree));
            }
        });
        on!(on_add_child_control, |c, i| {
            if let Some(id) = c.with_project(|p| p.data.controls.get(pos(i)?).map(|x| x.id)) {
                c.select(Selection::Control(id), false);
                c.send(Command::BeginDraft(DraftKind::Goal));
            }
        });

        // Duplicate (UI-09.2, UI-09.4).
        on!(on_duplicate_component, |c, i| {
            if let Some(source) = c.with_project(|p| p.data.components.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::DuplicateComponent {
                    id: ComponentId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_interface, |c, i| {
            if let Some(source) = c.with_project(|p| p.data.interfaces.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::DuplicateInterface {
                    id: InterfaceId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_data_flow, |c, i| {
            if let Some(source) = c.with_project(|p| p.data.data_flows.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::DuplicateDataFlow {
                    id: DataFlowId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_trust_boundary, |c, i| {
            if let Some(source) =
                c.with_project(|p| p.data.trust_boundaries.get(pos(i)?).map(|x| x.id))
            {
                c.edit(Edit::DuplicateTrustBoundary {
                    id: TrustBoundaryId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_asset, |c, i| {
            if let Some(source) = c.with_project(|p| p.data.assets.get(pos(i)?).map(|a| a.id)) {
                c.edit(Edit::DuplicateAsset {
                    id: AssetId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_damage, |c, i| {
            if let Some(source) = c.with_project(|p| p.data.damages.get(pos(i)?).map(|d| d.id)) {
                c.edit(Edit::DuplicateDamage {
                    id: DamageId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_threat, |c, i| {
            if let Some(source) = c.with_project(|p| p.threats.get(pos(i)?).map(|t| t.id)) {
                c.edit(Edit::DuplicateThreat {
                    id: ThreatId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_control, |c, i| {
            if let Some(source) = c.with_project(|p| p.data.controls.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::DuplicateControl {
                    id: ControlId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_goal, |c, i| {
            if let Some(source) = c.with_project(|p| p.data.goals.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::DuplicateGoal {
                    id: GoalId::new_v7(),
                    source,
                });
            }
        });
        on!(on_duplicate_claim, |c, i| {
            if let Some(source) = c.with_project(|p| p.data.claims.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::DuplicateClaim {
                    id: ClaimId::new_v7(),
                    source,
                });
            }
        });

        // Remove from the row menu: immediate, one undo step, with the undo
        // toast (job 116).
        on!(on_remove_row_component, |c, i| {
            if let Some(component) =
                c.with_project(|p| p.data.components.get(pos(i)?).map(|x| x.id))
            {
                c.edit(Edit::RemoveComponent { component });
            }
        });
        on!(on_remove_row_interface, |c, i| {
            if let Some(interface) =
                c.with_project(|p| p.data.interfaces.get(pos(i)?).map(|x| x.id))
            {
                c.edit(Edit::RemoveInterface { interface });
            }
        });
        on!(on_remove_row_data_flow, |c, i| {
            if let Some(data_flow) =
                c.with_project(|p| p.data.data_flows.get(pos(i)?).map(|x| x.id))
            {
                c.edit(Edit::RemoveDataFlow { data_flow });
            }
        });
        on!(on_remove_row_trust_boundary, |c, i| {
            if let Some(boundary) =
                c.with_project(|p| p.data.trust_boundaries.get(pos(i)?).map(|x| x.id))
            {
                c.edit(Edit::RemoveTrustBoundary { boundary });
            }
        });
        on!(on_remove_row_asset, |c, i| {
            if let Some(asset) = c.with_project(|p| p.data.assets.get(pos(i)?).map(|a| a.id)) {
                c.edit(Edit::RemoveAsset { asset });
            }
        });
        on!(on_remove_row_damage, |c, i| {
            if let Some(damage) = c.with_project(|p| p.data.damages.get(pos(i)?).map(|d| d.id)) {
                c.edit(Edit::RemoveDamage { damage });
            }
        });
        on!(on_remove_row_threat, |c, i| {
            if let Some(threat) = c.with_project(|p| p.threats.get(pos(i)?).map(|t| t.id)) {
                c.edit(Edit::RemoveThreat { threat });
            }
        });
        on!(on_remove_row_control, |c, i| {
            if let Some(control) = c.with_project(|p| p.data.controls.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::RemoveControl { control });
            }
        });
        on!(on_remove_row_goal, |c, i| {
            if let Some(goal) = c.with_project(|p| p.data.goals.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::RemoveGoal { goal });
            }
        });
        on!(on_remove_row_claim, |c, i| {
            if let Some(claim) = c.with_project(|p| p.data.claims.get(pos(i)?).map(|x| x.id)) {
                c.edit(Edit::RemoveClaim { claim });
            }
        });
        on!(on_remove_row_tree, |c, i| {
            if let Some(tree) = c.with_project(|p| p.data.trees.get(pos(i)?).map(|t| t.id)) {
                c.edit(Edit::RemoveTree { tree });
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
        on!(on_export_pdf, |c| c.export(ExportKind::ReportPdf));
        on!(on_export_xlsx, |c| c.export(ExportKind::ReportXlsx));
        on!(on_export_csv, |c| c.export(ExportKind::RiskRegister));
        on!(on_export_svg, |c| c.export(ExportKind::TreeSvg));
        on!(on_pick_figure, |c, i| {
            c.state.borrow_mut().figure = pos(i);
            if let Some(w) = c.window.upgrade() {
                w.set_figure_selected(i);
            }
        });
        on!(on_add_figure, |c, path, caption| {
            if let Ok(data) = std::fs::read(path.as_str().trim()) {
                c.send(Command::PutFigure {
                    caption: caption.to_string(),
                    target: "project".to_owned(),
                    data,
                });
            }
        });
        on!(on_update_figure, |c, caption| {
            let chosen = c.state.borrow().figure;
            let figure = c.with_project(|p| {
                let f = p.figures.get(chosen?)?;
                Some((f.figure_id.clone(), f.target.clone()))
            });
            if let Some((figure_id, target)) = figure {
                c.send(Command::UpdateFigure {
                    figure_id,
                    caption: caption.to_string(),
                    target,
                });
            }
        });
        on!(on_delete_figure, |c| {
            let chosen = c.state.borrow().figure;
            let id = c.with_project(|p| p.figures.get(chosen?).map(|f| f.figure_id.clone()));
            if let Some(figure_id) = id {
                c.state.borrow_mut().figure = None;
                c.send(Command::DeleteFigure { figure_id });
            }
        });
        on!(on_preview_excel_import, |c, path| {
            if let Ok(bytes) = std::fs::read(path.as_str().trim()) {
                c.send(Command::PreviewExcelImport {
                    bytes,
                    mapping: Box::default(),
                });
            }
        });
        on!(on_apply_excel_import, |c| c.send(Command::ApplyExcelImport));
        on!(on_clear_excel_preview, |c| c
            .send(Command::ClearExcelPreview));

        // ---- administration ----
        on!(on_pick_user, |c, i| {
            c.state.borrow_mut().user = pos(i);
            c.apply_users();
        });
        on!(on_refresh_sessions, |c| c.send(Command::LoadSessions));
        on!(on_pick_session, |c, i| {
            c.state.borrow_mut().session = pos(i);
            c.apply_sessions();
        });
        on!(
            on_revoke_session,
            |c| if let Some(row) = c.selected_session() {
                c.state.borrow_mut().session = None;
                c.send(Command::Admin(AdminAction::RevokeSession {
                    id: row.id,
                    username: row.user,
                }));
            }
        );
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
        on!(on_refresh_memberships, |c| c.send(Command::LoadMemberships));
        on!(on_pick_membership, |c, i| {
            c.state.borrow_mut().membership = pos(i);
            c.apply_memberships();
        });
        on!(on_add_membership, |c, member, role| {
            let role = usize::try_from(role)
                .ok()
                .and_then(|i| ROLE_KEYS.get(i))
                .copied()
                .unwrap_or("viewer");
            let member = member.trim().to_owned();
            if member.is_empty() {
                return;
            }
            c.send(Command::SetMembership {
                member,
                role: role.to_owned(),
            });
        });
        on!(on_set_membership_role, |c, role| {
            let role = usize::try_from(role)
                .ok()
                .and_then(|i| ROLE_KEYS.get(i))
                .copied()
                .unwrap_or("viewer");
            if let Some(m) = c.selected_membership() {
                c.send(Command::SetMembership {
                    member: m.member,
                    role: role.to_owned(),
                });
            }
        });
        on!(on_remove_membership, |c| {
            if let Some(m) = c.selected_membership() {
                c.state.borrow_mut().membership = None;
                c.send(Command::RemoveMembership { member: m.member });
            }
        });

        // ---- command palette (job 081) ----
        on!(on_open_palette, |c| {
            if let Some(w) = c.window.upgrade() {
                w.set_palette_open(true);
                w.set_palette_query(s(""));
                w.set_palette_shortcuts_open(false);
            }
            c.apply_palette("");
        });
        on!(on_palette_query_changed, |c, text| c.apply_palette(&text));
        on!(on_palette_pick, |c, i| {
            let picked =
                pos(i).and_then(|i| c.palette.borrow().get(i).map(|e| (e.id.clone(), e.target)));
            if let Some((id, target)) = picked {
                if let Some(target) = target {
                    c.select(target, true);
                } else if let Some(page) = id
                    .strip_prefix("page:")
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(aegis_client::view::Page::from_index)
                {
                    c.send(Command::ShowPage(page));
                } else if let Some(cmd) = id.strip_prefix("cmd:") {
                    c.run_palette_command(cmd);
                }
            }
            if let Some(w) = c.window.upgrade() {
                w.set_palette_open(false);
            }
        });
        on!(on_palette_show_shortcuts, |c| {
            if let Some(w) = c.window.upgrade() {
                let open = !w.get_palette_shortcuts_open();
                w.set_palette_shortcuts_open(open);
                if open {
                    c.apply_shortcuts();
                }
            }
        });

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

    // ---- command palette (job 081) ------------------------------------

    /// Filters the palette entries by `query` and shows them.
    fn apply_palette(&self, query: &str) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let q = query.to_lowercase();
        let mut all = crate::shortcuts::palette_entries();
        if let Some(elements) = self.with_project(|p| Some(p.palette_elements.clone())) {
            for e in elements {
                all.push(crate::shortcuts::PaletteEntry {
                    id: format!("el:{}", e.visual_id),
                    label: format!("{} \u{b7} {}", e.visual_id, e.name),
                    group: e.kind.to_owned(),
                    keys: e.visual_id,
                    name: e.name,
                    target: Some(e.target),
                });
            }
        }
        let entries: Vec<crate::shortcuts::PaletteEntry> = all
            .into_iter()
            .filter(|e| {
                if q.is_empty() {
                    return true;
                }
                let label = e.label.to_lowercase();
                let group = e.group.to_lowercase();
                let keys = e.keys.to_lowercase();
                // Element visual IDs are matched by prefix so `T00` finds the
                // threat `T00` and not the attack tree `AT00` (whose ID merely
                // contains `T00`). Their name is matched separately.
                if e.id.starts_with("el:") {
                    keys.starts_with(&q) || e.name.to_lowercase().contains(&q) || group.contains(&q)
                } else {
                    label.contains(&q) || group.contains(&q) || keys.contains(&q)
                }
            })
            .collect();
        w.set_palette_rows(rows(
            entries
                .iter()
                .map(|e| vec![e.label.clone(), e.keys.clone()])
                .collect(),
        ));
        w.set_palette_selected(if entries.is_empty() { -1 } else { 0 });
        *self.palette.borrow_mut() = entries;
    }

    /// Shows the shortcut table as the palette's cheat-sheet view (UI-11.5).
    fn apply_shortcuts(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        w.set_shortcut_rows(rows(
            crate::shortcuts::table()
                .iter()
                .map(|s| vec![s.label.to_owned(), crate::shortcuts::keys(s).to_owned()])
                .collect(),
        ));
    }

    /// Runs a command chosen from the palette by its shortcut id.
    fn run_palette_command(&self, id: &str) {
        match id {
            "undo" => self.send(Command::Undo),
            "redo" => self.send(Command::Redo),
            "begin-edit" => self.send(Command::BeginEdit),
            "end-edit" => self.send(Command::EndEdit),
            "palette" => {
                if let Some(w) = self.window.upgrade() {
                    w.set_palette_open(true);
                }
            }
            "new-element" => {
                if let Some(w) = self.window.upgrade() {
                    w.set_palette_open(false);
                    w.invoke_begin_new();
                }
            }
            page => {
                if let Some(page) = crate::shortcuts::page_for(page) {
                    self.send(Command::ShowPage(page));
                }
            }
        }
    }

    // ---- what is selected -------------------------------------------------

    /// Draws the Library panel's state, and which places the selection on
    /// the canvas offers.
    fn show_library(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let lib = self.library.borrow();
        w.set_library_rows(rows(lib.rows()));
        w.set_library_selected(
            lib.picked_row()
                .and_then(|r| i32::try_from(r).ok())
                .unwrap_or(-1),
        );
        w.set_library_preview(s(&lib.preview()));
        w.set_library_facets(model(lib.facet_choices().iter().map(|c| s(c)).collect()));
        w.set_library_filter(s(&lib.filter_text()));
        w.set_catalog_rows(rows(lib.catalog_rows()));
        w.set_catalog_selected(
            lib.catalog_picked_row()
                .and_then(|r| i32::try_from(r).ok())
                .unwrap_or(-1),
        );
        w.set_catalog_preview(s(&lib.catalog_preview()));
        let place = self.library_place();
        w.set_library_can_gate(matches!(place, Some((_, Some(false)))));
        w.set_library_can_leaf(matches!(place, Some((_, Some(true)))));
    }

    /// The tree on the canvas and, when a node of it is selected, whether
    /// that node is a step.
    fn library_place(&self) -> Option<(TreeId, Option<bool>)> {
        self.with_project(|p| {
            let tree = p.tree.as_ref()?.tree;
            let node = p
                .inspector
                .as_ref()
                .filter(|i| i.tree == tree)
                .map(|i| i.is_step);
            Some((tree, node))
        })
    }

    fn insert_library_item(&self, mode: i32, ratings: bool, share: bool) {
        let (item, pack) = {
            let lib = self.library.borrow();
            let Some(item) = lib.picked_item() else {
                return;
            };
            (item, Arc::clone(lib.pack()))
        };
        let mode = match (mode, self.library_place()) {
            (0, _) => InsertMode::NewTree {
                id: TreeId::new_v7(),
                threat: None,
            },
            (1, Some((tree, Some(false)))) => {
                let Some(gate) = self.with_project(|p| p.inspector.as_ref().map(|i| i.node)) else {
                    return;
                };
                InsertMode::UnderGate { tree, gate }
            }
            (2, Some((tree, Some(true)))) => {
                let Some(leaf) = self.with_project(|p| p.inspector.as_ref().map(|i| i.node)) else {
                    return;
                };
                InsertMode::InPlaceOfLeaf { tree, leaf }
            }
            _ => return,
        };
        self.edit(Edit::InsertLibraryItem {
            pack,
            item,
            mode,
            options: InsertOptions {
                with_ratings: ratings,
                share_all: share,
                ..InsertOptions::default()
            },
        });
        self.send(Command::ShowPage(Page::Trees));
    }

    fn selected_component(&self) -> Option<ComponentId> {
        self.with_project(|p| p.data.component.as_ref().map(|c| c.id))
    }

    fn selected_interface(&self) -> Option<InterfaceId> {
        self.with_project(|p| p.data.interface.as_ref().map(|i| i.id))
    }

    fn selected_data_flow(&self) -> Option<DataFlowId> {
        self.with_project(|p| p.data.data_flow.as_ref().map(|d| d.id))
    }

    fn selected_trust_boundary(&self) -> Option<TrustBoundaryId> {
        self.with_project(|p| p.data.trust_boundary.as_ref().map(|t| t.id))
    }

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

    fn selected_session(&self) -> Option<aegis_client::audit::SessionRow> {
        let i = self.state.borrow().session?;
        self.view()?.sessions.get(i).cloned()
    }

    fn selected_user(&self) -> Option<aegis_client::state::UserRow> {
        let i = self.state.borrow().user?;
        self.view()?.users.get(i).cloned()
    }

    fn selected_membership(&self) -> Option<aegis_client::state::MembershipRow> {
        let i = self.state.borrow().membership?;
        self.view()?.memberships.get(i).cloned()
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
            self.apply_canvas(&view, None);
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
        self.redraw_inner(false);
    }

    fn release(&self, at: Point) {
        let click = {
            let mut st = self.state.borrow_mut();
            st.press
                .take()
                .is_some_and(|p| p.travelled <= DRAG_THRESHOLD)
        };
        if !click {
            self.redraw_inner(true);
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
        let prev = self.state.borrow_mut().view.replace(Arc::clone(view));
        if prev
            .as_ref()
            .is_some_and(|old| old.revision == view.revision)
        {
            return;
        }

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
        if prev
            .as_ref()
            .is_none_or(|old| old.projects != view.projects)
        {
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
        }
        if prev.as_ref().is_none_or(|old| old.notices != view.notices) {
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
                        action: match n.action {
                            None => 0,
                            Some(NoticeAction::Undo) => 1,
                            Some(NoticeAction::Redo) => 2,
                        },
                    })
                    .collect(),
            ));
        }
        w.set_admin_text(s(view.admin_text.as_deref().unwrap_or("")));
        w.set_settings_known(view.settings.is_some());
        if let Some(settings) = view.settings {
            w.set_password_login_on(settings.password_login_enabled);
            w.set_company_login_available(settings.company_login_available);
        }
        if prev.as_ref().is_none_or(|old| old.users != view.users) {
            self.apply_users();
        }
        if prev
            .as_ref()
            .is_none_or(|old| old.sessions != view.sessions)
        {
            self.apply_sessions();
        }
        if prev.as_ref().is_none_or(|old| {
            old.memberships != view.memberships
                || old.memberships_error != view.memberships_error
                || old.membership_error != view.membership_error
        }) {
            self.apply_memberships();
        }
        if prev.as_ref().is_none_or(|old| {
            old.events != view.events
                || old.events_error != view.events_error
                || old.connection != view.connection
        }) {
            self.apply_events();
        }

        let Some(project) = view.project.as_ref() else {
            w.set_has_project(false);
            w.set_draw(model(Vec::new()));
            w.set_draw_paths(model(Vec::new()));
            w.set_draw_rects(model(Vec::new()));
            w.set_draw_texts(model(Vec::new()));
            self.state.borrow_mut().shown_tree = None;
            return;
        };
        let prev_proj = prev.as_ref().and_then(|old| old.project.as_ref());
        w.set_has_project(true);
        w.set_project_name(s(&project.entry.name));
        w.set_editing(project.mode == Mode::Editing);
        if prev_proj.is_none_or(|old| old.descriptions != project.descriptions) {
            w.set_descriptions(model(
                project
                    .descriptions
                    .iter()
                    .map(|d| DescriptionItem {
                        kind: s(d.kind),
                        text: s(&d.text),
                        by: s(&d.by),
                        translations: model(
                            d.translations
                                .iter()
                                .map(|t| TranslationItem {
                                    lang: s(&t.lang),
                                    text: s(&t.text),
                                })
                                .collect(),
                        ),
                    })
                    .collect(),
            ));
        }
        let me = view
            .me
            .as_ref()
            .map_or(view.user.as_str(), |m| m.subject.as_str());
        let (kind, mut band) = match (&project.mode, project.lock_holder.as_deref(), project.sync) {
            (_, _, SyncView::Resyncing) => (1, "Reloading the project from the server…".to_owned()),
            (Mode::Editing, _, _) => (
                2,
                "You're editing. Others read the last saved version.".to_owned(),
            ),
            (Mode::ReadOnly, Some(holder), _) if holder != me => (
                1,
                format!(
                    "Read-only. {holder} is editing; you see the version saved when you opened it."
                ),
            ),
            (Mode::ReadOnly, _, _) => {
                (0, "Read-only. Press Edit to change the project.".to_owned())
            }
        };
        let newer = project.newer_version && project.mode == Mode::ReadOnly;
        if newer {
            band.push_str(" A newer version is available.");
        }
        w.set_newer_version(newer);
        w.set_band_kind(kind);
        w.set_band_text(s(&band));
        w.set_sync_text(s(&match project.sync {
            SyncView::Synced => "Saved".to_owned(),
            SyncView::Syncing { pending: 1 } => "Saving 1 change…".to_owned(),
            SyncView::Syncing { pending } => format!("Saving {pending} changes…"),
            SyncView::Resyncing => "Reloading…".to_owned(),
        }));
        let (numbers_kind, numbers_text) = match project.numbers {
            NumbersCheck::Checked => (0, "Numbers checked with the server"),
            NumbersCheck::Checking => (1, "Checking the numbers with the server\u{2026}"),
            NumbersCheck::ServerNumbersShown => (2, "Showing the server's numbers"),
            NumbersCheck::NotComparable => {
                (3, "Numbers not checked: the server runs another engine")
            }
        };
        w.set_numbers_kind(numbers_kind);
        w.set_numbers_text(s(numbers_text));
        w.set_can_undo(project.can_undo);
        w.set_can_redo(project.can_redo);
        w.set_draft_kind(s(project.draft.map_or("", |k| k.key())));
        w.set_draft_seq(i32::try_from(project.draft_seq).unwrap_or(i32::MAX));
        w.set_draft_incomplete(project.draft_incomplete);
        w.set_draft_error(s(project.draft_error.as_deref().unwrap_or("")));
        w.set_draft_category(
            project
                .draft_category
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        w.set_draft_asset(index_of(
            project.data.assets.iter().map(|a| a.id),
            project.draft_asset,
        ));
        w.set_draft_component(index_of(
            project.data.components.iter().map(|c| c.id),
            project.draft_component,
        ));
        w.set_draft_to(index_of(
            project.data.components.iter().map(|c| c.id),
            project.draft_to,
        ));
        w.set_draft_interface_kind(
            project
                .draft_interface_kind
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        w.set_undo_label(s(project.undo_label.as_deref().unwrap_or("")));
        w.set_redo_label(s(project.redo_label.as_deref().unwrap_or("")));
        w.set_show_residual(project.show_residual);
        w.set_is_empty(project.is_empty);
        w.set_page(i32::try_from(project.page.index()).unwrap_or(0));
        if prev_proj.is_none_or(|old| old.nav_counts != project.nav_counts) {
            w.set_nav_counts(model(
                project
                    .nav_counts
                    .iter()
                    .map(|c| c.map_or_else(SharedString::new, |n| s(&n.to_string())))
                    .collect(),
            ));
        }
        if prev_proj.is_none_or(|old| !Arc::ptr_eq(&old.vocab, &project.vocab)) {
            self.apply_vocabulary(&w, project);
        }
        if prev_proj.is_none_or(|old| {
            old.threats != project.threats
                || old.draft != project.draft
                || old.selected_threat != project.selected_threat
                || old.data.threat != project.data.threat
        }) {
            self.apply_threats(&w, project);
        }
        if prev_proj.is_none_or(|old| {
            old.data != project.data
                || old.draft != project.draft
                || old.baselines != project.baselines
                || old.comparison != project.comparison
                || old.history != project.history
                || old.last_export != project.last_export
                || old.figures != project.figures
                || old.excel_preview != project.excel_preview
                || old.tree.as_ref().map(|t| t.tree) != project.tree.as_ref().map(|t| t.tree)
        }) {
            self.apply_pages(&w, project);
        }
        if w.get_palette_open()
            && prev_proj.is_none_or(|old| old.palette_elements != project.palette_elements)
        {
            self.apply_palette(&w.get_palette_query());
        }
        self.apply_canvas(view, prev_proj);
    }

    fn apply_vocabulary(&self, w: &AppWindow, p: &ProjectView) {
        let v = &p.vocab;
        w.set_has_attack_potential(v.has_attack_potential);
        w.set_feasibility_label(s(&v.element_names.feasibility));
        w.set_damage_singular(s(&v.element_names.damage_scenario.singular));
        w.set_damage_plural(s(&v.element_names.damage_scenario.plural));
        w.set_goal_singular(s(&v.element_names.cybersecurity_goal.singular));
        w.set_goal_plural(s(&v.element_names.cybersecurity_goal.plural));
        w.set_property_labels(model(v.properties.iter().map(|x| s(x)).collect()));
        w.set_threat_categories(model(v.threat_categories.iter().map(|x| s(x)).collect()));
        w.set_feasibility_options(with_none("Not rated", v.feasibility.iter().cloned()));
        w.set_feasibility_levels(model(v.feasibility.iter().map(|x| s(x)).collect()));
        w.set_treatment_options(with_none("Not decided", v.treatments.iter().cloned()));
        w.set_catalog_schemes(model(v.catalog_schemes.iter().map(|x| s(x)).collect()));
        w.set_interface_kinds(model(v.interface_kinds.iter().map(|x| s(x)).collect()));
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
        let mut list: Vec<Vec<String>> = p
            .threats
            .iter()
            .map(|t| {
                vec![
                    t.visual_id.clone(),
                    t.name.clone(),
                    t.asset.clone(),
                    risk_text(t.inherent.as_ref()),
                    risk_text(t.residual.as_ref()),
                ]
            })
            .collect();
        let draft_at = push_draft_row(
            &mut list,
            p.draft == Some(DraftKind::Threat),
            5,
            w.get_draft_row_threat().to_string(),
        );
        w.set_threat_rows(rows(list));
        w.set_threat_selected(draft_at.unwrap_or_else(|| {
            p.selected_threat
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1)
        }));
        let d = p.data.threat.as_ref();
        w.set_has_threat_detail(d.is_some());
        let Some(d) = d else {
            w.set_td_id(s(""));
            return;
        };
        w.set_td_id(s(&d.visual_id));
        w.set_td_name(s(&d.name));
        w.set_td_asset(s(&format!("{} \u{b7} {}", d.asset_visual_id, d.asset_name)));
        w.set_td_category(i32::try_from(d.category).unwrap_or(0));
        w.set_td_damages(model(
            d.damages
                .iter()
                .map(|x| CheckItem {
                    label: s(&format!("{} \u{b7} {}", x.visual_id, x.name)),
                    checked: x.linked,
                })
                .collect(),
        ));
        w.set_td_refs(model(d.catalog_refs.iter().map(|x| s(x)).collect()));
        let plus_one = |x: Option<usize>| x.map_or(0, |i| i32::try_from(i + 1).unwrap_or(0));
        w.set_td_feasibility(plus_one(d.direct_feasibility));
        w.set_tree_names(with_none(
            "None",
            d.trees
                .iter()
                .map(|t| format!("{} \u{b7} {}", t.visual_id, t.name)),
        ));
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
        w.set_goal_names(with_none(
            "None",
            d.goals
                .iter()
                .map(|g| format!("{} \u{b7} {}", g.visual_id, g.name)),
        ));
        w.set_td_goal(plus_one(
            d.goal.and_then(|g| d.goals.iter().position(|x| x.id == g)),
        ));
        w.set_claim_names(with_none(
            "None",
            d.claims
                .iter()
                .map(|g| format!("{} \u{b7} {}", g.visual_id, g.name)),
        ));
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
                        label: s(&format!("{} \u{b7} {}", r.visual_id, r.threat)),
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

        // Item definition (job 074).
        let plus_one = |x: Option<usize>| x.map_or(0, |i| i32::try_from(i + 1).unwrap_or(0));
        w.set_component_names(model(
            d.components
                .iter()
                .map(|c| s(&format!("{} \u{b7} {}", c.visual_id, c.name)))
                .collect(),
        ));
        let mut list: Vec<Vec<String>> = d
            .components
            .iter()
            .map(|c| {
                vec![
                    c.visual_id.clone(),
                    c.name.clone(),
                    c.parent.clone(),
                    c.interfaces.to_string(),
                    c.assets.to_string(),
                ]
            })
            .collect();
        let draft_at = push_draft_row(
            &mut list,
            p.draft == Some(DraftKind::Component),
            5,
            w.get_draft_row_component().to_string(),
        );
        w.set_component_rows(rows(list));
        w.set_component_selected(draft_at.unwrap_or_else(|| {
            index_of(
                d.components.iter().map(|c| c.id),
                d.component.as_ref().map(|c| c.id),
            )
        }));
        w.set_has_component(d.component.is_some());
        if let Some(c) = &d.component {
            w.set_component_id(s(&c.visual_id));
            w.set_component_name(s(&c.name));
            w.set_component_parent_options(with_none(
                "None",
                c.parents
                    .iter()
                    .map(|x| format!("{} \u{b7} {}", x.visual_id, x.name)),
            ));
            w.set_component_parent(plus_one(
                c.parent
                    .and_then(|pid| c.parents.iter().position(|x| x.id == pid)),
            ));
            w.set_component_interfaces(links(&c.interfaces));
            w.set_component_assets(links(&c.assets));
            w.set_component_changed(s(&c.changed));
        } else {
            w.set_component_id(s(""));
        }
        let mut list: Vec<Vec<String>> = d
            .interfaces
            .iter()
            .map(|i| {
                vec![
                    i.visual_id.clone(),
                    i.name.clone(),
                    i.component.clone(),
                    i.kind.clone(),
                    i.flows.to_string(),
                ]
            })
            .collect();
        let draft_at = push_draft_row(
            &mut list,
            p.draft == Some(DraftKind::Interface),
            5,
            w.get_draft_row_interface().to_string(),
        );
        w.set_interface_rows(rows(list));
        w.set_interface_selected(draft_at.unwrap_or_else(|| {
            index_of(
                d.interfaces.iter().map(|i| i.id),
                d.interface.as_ref().map(|i| i.id),
            )
        }));
        w.set_has_interface(d.interface.is_some());
        if let Some(i) = &d.interface {
            w.set_interface_id(s(&i.visual_id));
            w.set_interface_name(s(&i.name));
            w.set_interface_component(s(&i.component_name));
            w.set_interface_kind(i32::try_from(i.kind).unwrap_or(0));
            w.set_interface_flows(links(&i.flows));
            w.set_interface_changed(s(&i.changed));
        } else {
            w.set_interface_id(s(""));
        }
        let mut list: Vec<Vec<String>> = d
            .data_flows
            .iter()
            .map(|df| {
                vec![
                    df.visual_id.clone(),
                    df.name.clone(),
                    df.from.clone(),
                    df.to.clone(),
                    df.interface.clone(),
                ]
            })
            .collect();
        let draft_at = push_draft_row(
            &mut list,
            p.draft == Some(DraftKind::DataFlow),
            5,
            w.get_draft_row_data_flow().to_string(),
        );
        w.set_data_flow_rows(rows(list));
        w.set_data_flow_selected(draft_at.unwrap_or_else(|| {
            index_of(
                d.data_flows.iter().map(|df| df.id),
                d.data_flow.as_ref().map(|df| df.id),
            )
        }));
        w.set_has_data_flow(d.data_flow.is_some());
        if let Some(df) = &d.data_flow {
            w.set_data_flow_id(s(&df.visual_id));
            w.set_data_flow_name(s(&df.name));
            w.set_data_flow_from(s(&df.from_label));
            w.set_data_flow_to(s(&df.to_label));
            w.set_data_flow_interface_options(with_none(
                "None",
                df.interfaces
                    .iter()
                    .map(|x| format!("{} \u{b7} {}", x.visual_id, x.name)),
            ));
            w.set_data_flow_interface(plus_one(
                df.interface
                    .and_then(|iid| df.interfaces.iter().position(|x| x.id == iid)),
            ));
            w.set_data_flow_crosses_boundary(df.crosses_boundary);
            w.set_data_flow_changed(s(&df.changed));
        } else {
            w.set_data_flow_id(s(""));
        }
        let mut list: Vec<Vec<String>> = d
            .trust_boundaries
            .iter()
            .map(|tb| {
                vec![
                    tb.visual_id.clone(),
                    tb.name.clone(),
                    tb.members.to_string(),
                ]
            })
            .collect();
        let draft_at = push_draft_row(
            &mut list,
            p.draft == Some(DraftKind::TrustBoundary),
            3,
            w.get_draft_row_trust_boundary().to_string(),
        );
        w.set_trust_boundary_rows(rows(list));
        w.set_trust_boundary_selected(draft_at.unwrap_or_else(|| {
            index_of(
                d.trust_boundaries.iter().map(|tb| tb.id),
                d.trust_boundary.as_ref().map(|tb| tb.id),
            )
        }));
        w.set_has_trust_boundary(d.trust_boundary.is_some());
        if let Some(tb) = &d.trust_boundary {
            w.set_trust_boundary_id(s(&tb.visual_id));
            w.set_trust_boundary_name(s(&tb.name));
            w.set_trust_boundary_members(model(
                tb.members
                    .iter()
                    .map(|m| CheckItem {
                        label: s(&format!("{} \u{b7} {}", m.visual_id, m.name)),
                        checked: m.member,
                    })
                    .collect(),
            ));
            w.set_trust_boundary_changed(s(&tb.changed));
        } else {
            w.set_trust_boundary_id(s(""));
        }

        // Assets (also the asset choices of the threats page).
        w.set_asset_names(model(
            d.assets
                .iter()
                .map(|a| s(&format!("{} \u{b7} {}", a.visual_id, a.name)))
                .collect(),
        ));
        let mut list: Vec<Vec<String>> = d
            .assets
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
                    a.visual_id.clone(),
                    a.name.clone(),
                    props.join(", "),
                    a.threats.to_string(),
                    a.worst
                        .as_ref()
                        .map_or_else(String::new, |r| r.label.clone()),
                ]
            })
            .collect();
        let drafting = p.draft == Some(DraftKind::Asset);
        if drafting {
            let mut row = vec![String::new(); 5];
            row[1] = w.get_draft_row_asset().to_string();
            list.push(row);
        }
        let draft_at = list.len() as i32 - 1;
        w.set_asset_rows(rows(list));

        let picked = index_of(
            d.assets.iter().map(|a| a.id),
            d.asset.as_ref().map(|a| a.id),
        );
        w.set_asset_selected(if drafting { draft_at } else { picked });
        w.set_has_asset(d.asset.is_some());
        if let Some(a) = &d.asset {
            w.set_asset_id(s(&a.visual_id));
            w.set_asset_name(s(&a.name));
            w.set_asset_component_options(with_none(
                "None",
                a.components
                    .iter()
                    .map(|c| format!("{} \u{b7} {}", c.visual_id, c.name)),
            ));
            w.set_asset_component(plus_one(
                a.component
                    .and_then(|cid| a.components.iter().position(|c| c.id == cid)),
            ));
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
        } else {
            w.set_asset_id(s(""));
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
        let mut list: Vec<Vec<String>> = d
            .damages
            .iter()
            .map(|x| {
                vec![
                    x.visual_id.clone(),
                    x.name.clone(),
                    impact_text(&x.impacts),
                    x.threats.to_string(),
                ]
            })
            .collect();
        let drafting = p.draft == Some(DraftKind::Damage);
        if drafting {
            let mut row = vec![String::new(); 4];
            row[1] = w.get_draft_row_damage().to_string();
            list.push(row);
        }
        let draft_at = list.len() as i32 - 1;
        w.set_damage_rows(rows(list));

        let picked = index_of(
            d.damages.iter().map(|x| x.id),
            d.damage.as_ref().map(|x| x.id),
        );
        w.set_damage_selected(if drafting { draft_at } else { picked });
        w.set_has_damage(d.damage.is_some());
        if let Some(x) = &d.damage {
            w.set_damage_id(s(&x.visual_id));
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
        } else {
            w.set_damage_id(s(""));
        }

        // Risks.
        w.set_risk_rows(rows(
            d.risks
                .iter()
                .map(|r| {
                    vec![
                        r.visual_id.clone(),
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
        let mut list: Vec<Vec<String>> = d
            .trees
            .iter()
            .map(|t| vec![t.visual_id.clone(), t.title.clone(), t.feasibility.clone()])
            .collect();
        let draft_at = push_draft_row(
            &mut list,
            p.draft == Some(DraftKind::Tree),
            3,
            w.get_draft_row_tree().to_string(),
        );
        w.set_tree_rows(rows(list));
        w.set_tree_selected(draft_at.unwrap_or_else(|| {
            index_of(
                d.trees.iter().map(|t| t.id),
                p.tree.as_ref().map(|t| t.tree),
            )
        }));
        w.set_outline_text(s(d.outline.as_deref().unwrap_or("")));

        // Controls.
        let mut list: Vec<Vec<String>> = d
            .controls
            .iter()
            .map(|c| {
                vec![
                    c.visual_id.clone(),
                    c.name.clone(),
                    c.steps.to_string(),
                    c.goals.to_string(),
                ]
            })
            .collect();
        let drafting = p.draft == Some(DraftKind::Control);
        if drafting {
            let mut row = vec![String::new(); 4];
            row[1] = w.get_draft_row_control().to_string();
            list.push(row);
        }
        let draft_at = list.len() as i32 - 1;
        w.set_control_rows(rows(list));

        let picked = index_of(
            d.controls.iter().map(|c| c.id),
            d.control.as_ref().map(|c| c.id),
        );
        w.set_control_selected(if drafting { draft_at } else { picked });
        w.set_has_control(d.control.is_some());
        if let Some(c) = &d.control {
            w.set_control_id(s(&c.visual_id));
            w.set_control_name(s(&c.name));
            w.set_control_steps(links(&c.steps));
            w.set_control_goals(links(&c.goals));
            w.set_control_changed(s(&c.changed));
        } else {
            w.set_control_id(s(""));
        }

        // Goals and claims.
        let mut list: Vec<Vec<String>> = d
            .goals
            .iter()
            .map(|g| {
                vec![
                    g.visual_id.clone(),
                    g.name.clone(),
                    g.controls.to_string(),
                    g.threats.to_string(),
                ]
            })
            .collect();
        let draft_at = push_draft_row(
            &mut list,
            p.draft == Some(DraftKind::Goal),
            4,
            w.get_draft_row_goal().to_string(),
        );
        w.set_goal_rows(rows(list));
        w.set_goal_selected(draft_at.unwrap_or_else(|| {
            index_of(d.goals.iter().map(|g| g.id), d.goal.as_ref().map(|g| g.id))
        }));
        w.set_has_goal(d.goal.is_some());
        if let Some(g) = &d.goal {
            w.set_goal_id(s(&g.visual_id));
            w.set_goal_name(s(&g.name));
            w.set_goal_controls(model(
                g.controls
                    .iter()
                    .map(|c| CheckItem {
                        label: s(&format!("{} \u{b7} {}", c.visual_id, c.name)),
                        checked: c.linked,
                    })
                    .collect(),
            ));
            w.set_goal_threats(links(&g.threats));
        } else {
            w.set_goal_id(s(""));
        }
        let mut list: Vec<Vec<String>> = d
            .claims
            .iter()
            .map(|c| vec![c.visual_id.clone(), c.name.clone(), c.threats.to_string()])
            .collect();
        let draft_at = push_draft_row(
            &mut list,
            p.draft == Some(DraftKind::Claim),
            3,
            w.get_draft_row_claim().to_string(),
        );
        w.set_claim_rows(rows(list));
        w.set_claim_selected(draft_at.unwrap_or_else(|| {
            index_of(
                d.claims.iter().map(|c| c.id),
                d.claim.as_ref().map(|c| c.id),
            )
        }));
        w.set_has_claim(d.claim.is_some());
        if let Some(c) = &d.claim {
            w.set_claim_id(s(&c.visual_id));
            w.set_claim_name(s(&c.name));
            w.set_claim_threats(links(&c.threats));
        } else {
            w.set_claim_id(s(""));
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
        w.set_figure_rows(rows(
            p.figures
                .iter()
                .map(|f| vec![f.caption.clone(), f.target.clone(), f.media_type.clone()])
                .collect(),
        ));
        {
            let mut st = self.state.borrow_mut();
            if st.figure.is_some_and(|i| i >= p.figures.len()) {
                st.figure = None;
            }
            w.set_figure_selected(st.figure.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
        }
        w.set_has_excel_preview(p.excel_preview.is_some());
        if let Some(preview) = &p.excel_preview {
            w.set_excel_summary(s(&preview.summary()));
            w.set_excel_unmapped_rows(rows(
                preview
                    .unmapped_rows
                    .iter()
                    .map(|u| vec![u.row_number.to_string(), u.reason.clone()])
                    .collect(),
            ));
        } else {
            w.set_excel_summary(SharedString::new());
            w.set_excel_unmapped_rows(rows(Vec::new()));
        }
    }

    fn apply_sessions(&self) {
        let (Some(w), Some(view)) = (self.window.upgrade(), self.view()) else {
            return;
        };
        w.set_session_rows(rows(
            view.sessions
                .iter()
                .map(|r| {
                    vec![
                        r.user.clone(),
                        r.device.clone(),
                        r.started.clone(),
                        r.expires.clone(),
                    ]
                })
                .collect(),
        ));
        let mut st = self.state.borrow_mut();
        if st.session.is_some_and(|i| i >= view.sessions.len()) {
            st.session = None;
        }
        w.set_session_selected(st.session.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
        w.set_sessions_text(s(if view.sessions.is_empty() {
            "No open sessions."
        } else {
            ""
        }));
    }

    fn apply_memberships(&self) {
        let (Some(w), Some(view)) = (self.window.upgrade(), self.view()) else {
            return;
        };
        w.set_membership_rows(rows(
            view.memberships
                .iter()
                .map(|m| vec![m.name.clone(), role_label(&m.role).to_owned()])
                .collect(),
        ));
        let mut st = self.state.borrow_mut();
        if st.membership.is_some_and(|i| i >= view.memberships.len()) {
            st.membership = None;
        }
        w.set_membership_selected(
            st.membership
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        let chosen = st.membership.and_then(|i| view.memberships.get(i));
        w.set_membership_role(chosen.map_or(0, |m| role_index(&m.role)));
        w.set_selected_member_name(s(chosen.map_or("", |m| m.name.as_str())));
        let empty = if let Some(why) = &view.memberships_error {
            why.clone()
        } else if view.memberships.is_empty() {
            "No members yet. Add the first one above.".to_owned()
        } else {
            String::new()
        };
        w.set_memberships_text(s(&empty));
        w.set_membership_error(s(view.membership_error.as_deref().unwrap_or("")));
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

    /// The event log, filtered, and the license facts.
    fn apply_events(&self) {
        let (Some(w), Some(view)) = (self.window.upgrade(), self.view()) else {
            return;
        };
        let group = self.state.borrow().event_group;
        let shown = aegis_client::audit::filter(&view.events, group);
        w.set_event_rows(rows(
            shown
                .iter()
                .map(|e| {
                    vec![
                        e.when.clone(),
                        e.who.clone(),
                        e.what.clone(),
                        e.about.clone(),
                    ]
                })
                .collect(),
        ));
        w.set_events_text(s(match (&view.events_error, shown.is_empty()) {
            (Some(why), _) => why,
            (None, true) => "No events to show.",
            (None, false) => "",
        }));
        let license = match &view.connection {
            ConnectionView::Online {
                license: Some(l), ..
            } => l.lines().join("\n"),
            _ => "No license is checked on this server (development build).".to_owned(),
        };
        w.set_license_text(s(&license));
    }

    /// The tree, its title, and the inspector.
    fn apply_canvas(&self, view: &Arc<ViewModel>, prev_proj: Option<&ProjectView>) {
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
                .map(|r| format!("{} \u{b7} {}", r.visual_id, r.name));
            match threat {
                Some(name) if name != root => {
                    format!("{} \u{b7} {name} \u{b7} {root} \u{b7} {how}", t.visual_id)
                }
                _ => format!("{} \u{b7} {root} \u{b7} {how}", t.visual_id),
            }
        });
        w.set_tree_title(s(title.as_deref().unwrap_or("")));
        w.set_has_tree(project.tree.is_some());
        // What a screen reader says on the canvas: the tree's size, and the
        // selected node in a full sentence (where it sits, its rating).
        w.set_canvas_summary(s(&match (&project.tree, project.selected_node) {
            (Some(t), Some(sel)) => {
                let node = u32::try_from(sel.index()).unwrap_or(u32::MAX);
                let said = aegis_canvas::access::describe(&t.scene, node)
                    .map(|a| aegis_canvas::access::sentence(&a, true))
                    .unwrap_or_default();
                format!("Attack tree with {} nodes. {said}", t.scene.len())
            }
            (Some(t), None) => format!(
                "Attack tree with {} nodes. Nothing is selected. Press an arrow key to select the root.",
                t.scene.len()
            ),
            _ => "No attack tree".to_owned(),
        }));

        if prev_proj.is_none_or(|old| {
            old.suggestions != project.suggestions
                || old.selected_suggestion != project.selected_suggestion
        }) {
            w.set_suggestion_rows(model(
                project
                    .suggestions
                    .iter()
                    .enumerate()
                    .map(|(idx, r)| SuggestionItem {
                        label: s(&r.label),
                        detail: s(&r.detail),
                        kind: s(&r.kind),
                        selected: project.selected_suggestion == Some(idx),
                    })
                    .collect(),
            ));
        }

        if prev_proj.is_none_or(|old| old.inspector != project.inspector) {
            match project.inspector.as_ref() {
                Some(i) => {
                    w.set_has_inspector(true);
                    w.set_inspector_tree_id(s(&i.tree_visual_id));
                    w.set_inspector_title(s(&i.title));
                    w.set_inspector_kind(s(&i.kind));
                    w.set_inspector_is_step(i.is_step);
                    w.set_inspector_is_root(i.is_root);
                    w.set_inspector_gate_index(i32::try_from(i.gate_index).unwrap_or(0));
                    w.set_inspector_can_move_up(i.can_move_up);
                    w.set_inspector_can_move_down(i.can_move_down);
                    w.set_inspector_needs_review(i.needs_review);
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
                                selected: f
                                    .selected
                                    .map_or(0, |x| i32::try_from(x + 1).unwrap_or(0)),
                            })
                            .collect(),
                    ));
                    w.set_mitigations(model(
                        i.mitigations
                            .iter()
                            .map(|m| MitigationItem {
                                name: s(&format!("{} \u{b7} {}", m.visual_id, m.name)),
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
                None => {
                    w.set_has_inspector(false);
                    w.set_inspector_tree_id(s(""));
                }
            }
        }

        // A tree that was not on the canvas before, or newly synthesized ghost
        // nodes, is fitted to the window.
        let tree_id = project.tree.as_ref().map(|t| t.tree);
        let new_suggestions = prev_proj
            .is_some_and(|old| old.suggestions.is_empty() && !project.suggestions.is_empty());
        let mut refitted = false;
        {
            let mut st = self.state.borrow_mut();
            if tree_id != st.shown_tree || new_suggestions {
                st.shown_tree = tree_id;
                refitted = true;
                if let Some(t) = &project.tree {
                    st.viewport.fit(t.scene.bounds(), 32.0);
                }
            }
        }
        let canvas_changed = refitted
            || prev_proj.is_none_or(|old| {
                old.selected_node != project.selected_node
                    || old.show_residual != project.show_residual
                    || match (&old.tree, &project.tree) {
                        (Some(a), Some(b)) => a.tree != b.tree || !Arc::ptr_eq(&a.scene, &b.scene),
                        (None, None) => false,
                        _ => true,
                    }
            });
        if canvas_changed {
            self.redraw();
        }
    }

    /// Re-renders the display list for the current view model and viewport.
    fn redraw(&self) {
        self.redraw_inner(true);
    }

    fn redraw_inner(&self, include_access: bool) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let st = self.state.borrow();
        let Some(project) = st.view.as_ref().and_then(|v| v.project.as_ref()) else {
            return;
        };
        match &project.tree {
            Some(tree) => {
                let options = RenderOptions {
                    selected: project.selected_node,
                };
                let list = render(&tree.scene, &st.viewport, &options);
                w.set_draw(model(draw_items(&list)));
                let (paths, rects, texts) = draw_layers(&list);
                w.set_draw_paths(model(paths));
                w.set_draw_rects(model(rects));
                w.set_draw_texts(model(texts));
            }
            None => {
                w.set_draw(model(Vec::new()));
                w.set_draw_paths(model(Vec::new()));
                w.set_draw_rects(model(Vec::new()));
                w.set_draw_texts(model(Vec::new()));
            }
        }
        w.set_zoom_text(s(&format!("{:.0}%", st.viewport.zoom * 100.0)));
        if !include_access {
            return;
        }
        // The nodes on screen, in words, for screen readers.
        let access = match &project.tree {
            Some(tree) => {
                let nodes = aegis_canvas::access::visible(&tree.scene, &st.viewport);
                let count = i32::try_from(nodes.len()).unwrap_or(i32::MAX);
                nodes
                    .into_iter()
                    .enumerate()
                    .map(|(i, n)| {
                        let selected = project
                            .selected_node
                            .is_some_and(|sel| u32::try_from(sel.index()) == Ok(n.access.node));
                        AccessNode {
                            x: n.x,
                            y: n.y,
                            w: n.w,
                            h: n.h,
                            label: s(&aegis_canvas::access::sentence(&n.access, selected)),
                            selected,
                            position: i32::try_from(i + 1).unwrap_or(i32::MAX),
                            count,
                            node: i32::try_from(n.access.node).unwrap_or(i32::MAX),
                        }
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        w.set_access_nodes(model(access));
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
