//! The state the UI renders: one immutable snapshot per change.

use std::sync::Arc;

use aegis_canvas::Scene;
use aegis_model::NodeIdx;

use crate::pages::{
    AssetDetail, AssetRow, ClaimRow, Comparison, ComponentDetail, ComponentRow, ControlDetail,
    ControlRow, DamageDetail, DamageRow, DataFlowDetail, DataFlowRow, GoalDetail, GoalRowOf,
    InterfaceDetail, InterfaceRow, Overview, ProblemRow, RiskRow, ThreatDetail, TreeRow,
    TrustBoundaryDetail, TrustBoundaryRow, Vocabulary,
};
use crate::time::format_ms;
use crate::view::{Inspector, Page, SuggestionRow, ThreatRow, TreeView};

/// How the client stands with the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionView {
    /// Connecting for the first time.
    Connecting,
    /// Connected.
    Online {
        /// Server version.
        server_version: String,
        /// Whether the server's engine is this client's engine. If not,
        /// numbers shown here may differ from the server's and editing is
        /// off.
        engine_ok: bool,
        /// The server's license, if it reports one.
        license: Option<LicenseView>,
    },
    /// Not reachable; the session keeps retrying.
    Offline {
        /// Why, in words for the user.
        reason: String,
    },
    /// The server needs a login.
    LoginRequired,
}

/// Where the server's license stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LicenseKind {
    /// In the paid period.
    Active,
    /// Past the paid period; everything still works for a while.
    Grace,
    /// Over: reading works, editing does not.
    Expired,
}

/// What a user needs to know about the server's license.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LicenseView {
    /// The licensed organisation.
    pub customer: String,
    /// Where the license stands.
    pub kind: LicenseKind,
    /// Seats in the license.
    pub seats: u32,
    /// Seats in use.
    pub seats_used: u32,
    /// Whole days until the paid period ends (active) or the grace period
    /// ends (grace). `None` once expired.
    pub days_left: Option<u64>,
    /// When the paid period ends, in UTC.
    pub valid_until: String,
    /// When the grace period ends, in UTC.
    pub grace_until: String,
    /// Licensed features, sorted.
    pub features: Vec<String>,
}

impl LicenseView {
    /// Days after which the app starts to remind users that the license ends.
    pub const REMIND_DAYS: u64 = 30;

    /// Reads the server's summary. `now_secs` is this computer's clock; it
    /// only decides how many days to say, never whether editing is allowed
    /// (the server decides that).
    #[must_use]
    pub fn from_info(info: &aegis_proto::LicenseInfo, now_secs: u64) -> Option<LicenseView> {
        use aegis_proto::license_info::State;
        let (kind, end) = match info.state() {
            State::Active => (LicenseKind::Active, info.expires_at),
            State::Grace => (LicenseKind::Grace, info.grace_ends_at),
            State::Expired => (LicenseKind::Expired, 0),
            State::Unspecified => return None,
        };
        Some(LicenseView {
            customer: info.customer.clone(),
            kind,
            seats: info.seats,
            seats_used: info.seats_used,
            days_left: (kind != LicenseKind::Expired)
                .then(|| end.saturating_sub(now_secs) / 86_400),
            valid_until: format_ms(info.expires_at.saturating_mul(1000)),
            grace_until: format_ms(info.grace_ends_at.saturating_mul(1000)),
            features: {
                let mut f = info.features.clone();
                f.sort();
                f
            },
        })
    }

    /// The license page, one fact per line.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Licensed to {}", self.customer),
            format!("{} of {} seats in use", self.seats_used, self.seats),
        ];
        lines.push(match self.kind {
            LicenseKind::Active => format!("Valid until {}", self.valid_until),
            LicenseKind::Grace => format!(
                "The paid period ended on {}. Everything works until {}.",
                self.valid_until, self.grace_until
            ),
            LicenseKind::Expired => format!(
                "The license expired on {} (grace ended {}). Projects can be read, not changed.",
                self.valid_until, self.grace_until
            ),
        });
        if !self.features.is_empty() {
            lines.push(format!("Features: {}", self.features.join(", ")));
        }
        lines
    }

    /// A sentence to show when the license needs attention, with how bad it is.
    #[must_use]
    pub fn reminder(&self) -> Option<(Severity, String)> {
        match (self.kind, self.days_left) {
            (LicenseKind::Expired, _) => Some((
                Severity::Error,
                "The license has expired. You can read projects but not change them. Ask your administrator to renew it.".to_owned(),
            )),
            (LicenseKind::Grace, days) => Some((
                Severity::Warning,
                format!(
                    "The license has expired. Everything works for {} more days. Ask your administrator to renew it.",
                    days.unwrap_or(0)
                ),
            )),
            (LicenseKind::Active, Some(days)) if days <= Self::REMIND_DAYS => Some((
                Severity::Info,
                format!("The license ends in {days} days. Ask your administrator to renew it."),
            )),
            (LicenseKind::Active, _) => None,
        }
    }
}

/// Whether the user may change the open project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Look only. Changes by the editor appear live.
    ReadOnly,
    /// The user holds the edit lock.
    Editing,
}

/// How far the user's changes have got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncView {
    /// Everything the user did is saved on the server.
    Synced,
    /// This many edits are not saved yet.
    Syncing {
        /// Edits waiting for the server.
        pending: usize,
    },
    /// The project is being reloaded from the server.
    Resyncing,
}

/// One project in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectEntry {
    /// ID, as the server writes it.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Methodology key.
    pub methodology: String,
}

/// How loudly to show a notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// For information.
    Info,
    /// Something the user should know.
    Warning,
    /// Something failed.
    Error,
}

/// The button on a toast: undo or redo what it reports (UI-10.5, UI-10.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeAction {
    /// "Undo".
    Undo,
    /// "Redo".
    Redo,
}

/// A message for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// Unique, to dismiss it.
    pub id: u64,
    /// Severity.
    pub severity: Severity,
    /// The text.
    pub text: String,
    /// A button that undoes or redoes what the text reports.
    pub action: Option<NoticeAction>,
}

/// The data of the page on screen. Only the fields of that page are
/// filled; the rest stay empty (the session builds one page per change).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageData {
    /// Overview page.
    pub overview: Option<Overview>,
    /// Item definition page: components (job 074).
    pub components: Vec<ComponentRow>,
    /// The selected component.
    pub component: Option<ComponentDetail>,
    /// Item definition page: interfaces (job 074).
    pub interfaces: Vec<InterfaceRow>,
    /// The selected interface.
    pub interface: Option<InterfaceDetail>,
    /// Item definition page: data flows (job 074).
    pub data_flows: Vec<DataFlowRow>,
    /// The selected data flow.
    pub data_flow: Option<DataFlowDetail>,
    /// Item definition page: trust boundaries (job 074).
    pub trust_boundaries: Vec<TrustBoundaryRow>,
    /// The selected trust boundary.
    pub trust_boundary: Option<TrustBoundaryDetail>,
    /// Assets page (and the asset choices of the threats page).
    pub assets: Vec<AssetRow>,
    /// The selected asset.
    pub asset: Option<AssetDetail>,
    /// Damage page.
    pub damages: Vec<DamageRow>,
    /// The selected damage scenario.
    pub damage: Option<DamageDetail>,
    /// The selected threat (threats and risks pages).
    pub threat: Option<ThreatDetail>,
    /// Trees page.
    pub trees: Vec<TreeRow>,
    /// The outline of the tree on the canvas, for the editor.
    pub outline: Option<String>,
    /// Risks page, worst first.
    pub risks: Vec<RiskRow>,
    /// Controls page.
    pub controls: Vec<ControlRow>,
    /// The selected control.
    pub control: Option<ControlDetail>,
    /// Goals page: goals.
    pub goals: Vec<GoalRowOf<aegis_model::GoalId>>,
    /// The selected goal.
    pub goal: Option<GoalDetail<aegis_model::GoalId>>,
    /// Goals page: claims.
    pub claims: Vec<ClaimRow>,
    /// The selected claim.
    pub claim: Option<GoalDetail<aegis_model::ClaimId>>,
    /// Problems page (and the overview's counts).
    pub problems: Vec<ProblemRow>,
}

/// One baseline in the versions list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaselineRow {
    /// ID, as the server writes it.
    pub id: String,
    /// Name.
    pub name: String,
    /// The change number it freezes.
    pub seq: u64,
    /// Who made it.
    pub created_by: String,
    /// When, as text.
    pub created: String,
}

/// One accepted change in the history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRow {
    /// Change number.
    pub seq: u64,
    /// Who.
    pub actor: String,
    /// When, as text.
    pub when: String,
    /// What kind of change, in words.
    pub kind: String,
}

/// One of this session's edits that undo can reverse (UI-03).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditRow {
    /// What it did, in words: "remove asset 'Brake ECU'".
    pub what: String,
    /// When, as text.
    pub when: String,
}

/// Who the server says the user is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Me {
    /// Login subject.
    pub subject: String,
    /// Shown name.
    pub display_name: String,
    /// Administrator.
    pub admin: bool,
    /// `password`, `oidc` or `development`.
    pub method: String,
}

/// Whether the risk numbers shown were checked against the server's own
/// (two engines must agree, ADR 0033). Follows the confirmed change, so an
/// edit makes it `Checking` until the server has recomputed and agreed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumbersCheck {
    /// The server's numbers for the state on screen are equal to this
    /// computer's.
    Checked,
    /// Waiting for the server's numbers for the state on screen.
    Checking,
    /// This computer's numbers differed once; the server's are shown.
    ServerNumbersShown,
    /// The server runs another engine version, so numbers cannot be
    /// compared. Editing is off in this case.
    NotComparable,
}

/// The server's login switch (job 045), as the administration page shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerSettingsView {
    /// Password login is accepted now.
    pub password_login_enabled: bool,
    /// The server has a company login, so the switch may be turned off.
    pub company_login_available: bool,
}

/// One user on the administration page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserRow {
    /// Login name.
    pub username: String,
    /// Shown name.
    pub display_name: String,
    /// Administrator.
    pub admin: bool,
    /// Disabled.
    pub disabled: bool,
    /// Last login, as text.
    pub last_login: String,
    /// Open sessions.
    pub sessions: u32,
}

/// One project membership row (job 058, ACC-01).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipRow {
    /// The wire member key: `user:<subject>` or `group:<SSO group name>`.
    pub member: String,
    /// The un-prefixed subject or group name, for display.
    pub name: String,
    /// One of the six built-in role keys.
    pub role: String,
}

/// The open project.
pub struct ProjectView {
    /// Which project.
    pub entry: ProjectEntry,
    /// Read-only or editing.
    pub mode: Mode,
    /// Who holds the edit lock, if anyone.
    pub lock_holder: Option<String>,
    /// The server holds a newer version than the one shown. Only a read-only
    /// view sets it; `Command::Reload` loads the newer one.
    pub newer_version: bool,
    /// Save state.
    pub sync: SyncView,
    /// The threat list.
    pub threats: Vec<ThreatRow>,
    /// Position of the selected threat in `threats`.
    pub selected_threat: Option<usize>,
    /// The tree on the canvas.
    pub tree: Option<TreeView>,
    /// The selected node, as a position in the scene.
    pub selected_node: Option<NodeIdx>,
    /// The selected node's details.
    pub inspector: Option<Inspector>,
    /// Whether undo is possible.
    pub can_undo: bool,
    /// Whether redo is possible.
    pub can_redo: bool,
    /// This session's edits, newest first: the history panel's rows (UI-03).
    pub edits: Vec<EditRow>,
    /// The draft being typed, if any (UI-08).
    pub draft: Option<crate::view::DraftKind>,
    /// Changes with every new draft.
    pub draft_seq: u64,
    /// Why the last create was refused; the draft stays with the user's text.
    pub draft_error: Option<String>,
    /// The asset of a threat draft.
    pub draft_asset: Option<aegis_model::AssetId>,
    /// The category of a threat draft.
    pub draft_category: Option<usize>,
    /// The component of an interface draft, or the "from" end of a flow draft.
    pub draft_component: Option<aegis_model::ComponentId>,
    /// The "to" end of a data flow draft.
    pub draft_to: Option<aegis_model::ComponentId>,
    /// The kind of an interface draft.
    pub draft_interface_kind: Option<usize>,
    /// A required field of the draft has no value yet (UI-08.2).
    pub draft_incomplete: bool,
    /// What undo would undo, as "remove asset 'Brake ECU'" (UI-10.2).
    pub undo_label: Option<String>,
    /// What redo would redo.
    pub redo_label: Option<String>,
    /// Whether the residual evaluation is shown.
    pub show_residual: bool,
    /// The description boxes of the page on screen (see
    /// [`crate::view::page_descriptions`]).
    pub descriptions: Vec<crate::view::DescriptionView>,
    /// The languages the project keeps translations in; empty hides the
    /// translation editor.
    pub languages: Vec<String>,
    /// The project has no assets yet (offer the sample).
    pub is_empty: bool,
    /// The last sequence number the server confirmed.
    pub confirmed_seq: u64,
    /// The newest sequence number at which this computer's risk numbers
    /// were found equal to the server's own. `None` until the first check.
    /// May lag behind `confirmed_seq`; `numbers` is what the window shows.
    pub risks_verified_seq: Option<u64>,
    /// Whether the numbers on screen are checked against the server's (G15).
    pub numbers: NumbersCheck,
    /// The page on screen.
    pub page: Page,
    /// The methodology's labels.
    pub vocab: Arc<Vocabulary>,
    /// The page's rows.
    pub data: PageData,
    /// The project's baselines, oldest first (versions page).
    pub baselines: Vec<BaselineRow>,
    /// The comparison with a baseline, when one is chosen.
    pub comparison: Option<Comparison>,
    /// Recent changes, newest first (history page).
    pub history: Vec<HistoryRow>,
    /// The last file written by an export.
    pub last_export: Option<String>,
    /// Per page in `Page::ALL` order: a count for the sidebar, if the page
    /// has one (entities, open problems).
    pub nav_counts: Vec<Option<usize>>,
    /// Pending AI or catalog suggestion items for the open project (job 089).
    pub suggestions: Vec<SuggestionRow>,
    /// Position of the selected suggestion in `suggestions` when a ghost node
    /// is selected on the canvas.
    pub selected_suggestion: Option<usize>,
    /// Attached figures (PNG, JPEG, or sanitized static SVG) on the project or
    /// elements (job 073, ARC-01, RPT-06).
    pub figures: Vec<crate::report::FigureAttachment>,
    /// Current dry-run preview of an Excel TARA import, if any (job 080, INT-02).
    pub excel_preview: Option<crate::excel::ExcelPreview>,
    /// All project elements indexed by visual ID and name for the command palette (FND-07).
    pub palette_elements: Vec<crate::pages::PaletteElement>,
}

impl ProjectView {
    /// The scene to draw, if a tree is shown.
    #[must_use]
    pub fn scene(&self) -> Option<&Arc<Scene>> {
        self.tree.as_ref().map(|t| &t.scene)
    }
}

/// Everything a UI needs to draw itself.
pub struct ViewModel {
    /// Increases with every change, so a UI can skip identical frames.
    pub revision: u64,
    /// Connection state.
    pub connection: ConnectionView,
    /// Known projects.
    pub projects: Vec<ProjectEntry>,
    /// The open project.
    pub project: Option<ProjectView>,
    /// Recent notices, oldest first.
    pub notices: Vec<Notice>,
    /// Who the user is, for display.
    pub user: String,
    /// Who the server says the user is, once known.
    pub me: Option<Me>,
    /// Users, for administrators (password login).
    pub users: Vec<UserRow>,
    /// The last result of an administration action, in words.
    pub admin_text: Option<String>,
    /// The login switch, once an administrator has loaded it.
    pub settings: Option<ServerSettingsView>,
    /// The server's event log in words, newest first (administrators).
    pub events: Vec<crate::audit::EventRow>,
    /// Why the event log is empty when it could not be read.
    pub events_error: Option<String>,
    /// The open sessions, newest first (administrators, password login).
    pub sessions: Vec<crate::audit::SessionRow>,
    /// The open project's membership rows, oldest first (project access, job
    /// 058).
    pub memberships: Vec<MembershipRow>,
    /// Why the membership list is empty when it could not be read.
    pub memberships_error: Option<String>,
    /// The last membership add/change/remove error, shown inline by the
    /// Access tab (for example the last-owner refusal).
    pub membership_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use aegis_proto::LicenseInfo;
    use aegis_proto::license_info::State;

    use super::*;

    fn info(state: State, expires_at: u64, grace_ends_at: u64) -> LicenseInfo {
        LicenseInfo {
            state: state.into(),
            customer: "Example".into(),
            seats: 4,
            seats_used: 1,
            expires_at,
            grace_ends_at,
            features: vec![],
        }
    }

    #[test]
    fn the_license_page_has_dates_and_features() {
        let day = 86_400;
        let mut i = info(State::Active, 20_000 * day, 20_030 * day);
        i.features = vec!["pdf".into(), "baselines".into()];
        let v = LicenseView::from_info(&i, 19_000 * day).expect("view");
        assert_eq!(v.valid_until, "4 Oct 2024 00:00 UTC");
        assert_eq!(v.grace_until, "3 Nov 2024 00:00 UTC");
        assert_eq!(v.features, ["baselines", "pdf"]);
        let lines = v.lines();
        assert!(lines.iter().any(|l| l.contains("Example")), "{lines:?}");
        assert!(
            lines.iter().any(|l| l.contains("1 of 4 seats")),
            "{lines:?}"
        );
        assert!(lines.iter().any(|l| l.contains("4 Oct 2024")), "{lines:?}");
        assert!(
            lines.iter().any(|l| l.contains("baselines, pdf")),
            "features are sorted: {lines:?}"
        );
        // Expired: the page says so and still shows when it ended.
        let over = LicenseView::from_info(
            &info(State::Expired, 20_000 * day, 20_030 * day),
            99_999 * day,
        )
        .expect("view");
        assert!(
            over.lines().iter().any(|l| l.contains("expired")),
            "{:?}",
            over.lines()
        );
    }

    #[test]
    fn days_are_counted_to_the_right_end() {
        let day = 86_400;
        let active = LicenseView::from_info(&info(State::Active, 10 * day, 20 * day), 3 * day)
            .expect("view");
        assert_eq!(
            (active.kind, active.days_left),
            (LicenseKind::Active, Some(7))
        );
        let grace = LicenseView::from_info(&info(State::Grace, 10 * day, 20 * day), 12 * day)
            .expect("view");
        assert_eq!((grace.kind, grace.days_left), (LicenseKind::Grace, Some(8)));
        let over = LicenseView::from_info(&info(State::Expired, 10 * day, 20 * day), 99 * day)
            .expect("view");
        assert_eq!((over.kind, over.days_left), (LicenseKind::Expired, None));
        assert!(LicenseView::from_info(&info(State::Unspecified, 0, 0), 0).is_none());
    }

    #[test]
    fn reminders_start_at_thirty_days_and_get_louder() {
        let day = 86_400;
        let at = |now_days: u64, state| {
            LicenseView::from_info(&info(state, 100 * day, 120 * day), now_days * day)
                .expect("view")
                .reminder()
        };
        assert_eq!(at(10, State::Active), None);
        assert_eq!(at(70, State::Active).map(|r| r.0), Some(Severity::Info));
        assert_eq!(
            at(70, State::Active).expect("reminder").1,
            "The license ends in 30 days. Ask your administrator to renew it."
        );
        assert_eq!(at(69, State::Active), None);
        assert_eq!(at(105, State::Grace).map(|r| r.0), Some(Severity::Warning));
        assert_eq!(at(130, State::Expired).map(|r| r.0), Some(Severity::Error));
    }
}
