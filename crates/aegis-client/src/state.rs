//! The state the UI renders: one immutable snapshot per change.

use std::sync::Arc;

use aegis_canvas::Scene;
use aegis_model::NodeIdx;

use crate::pages::{
    AssetDetail, AssetRow, ClaimRow, Comparison, ControlDetail, ControlRow, DamageDetail,
    DamageRow, GoalDetail, GoalRowOf, Overview, ProblemRow, RiskRow, ThreatDetail, TreeRow,
    Vocabulary,
};
use crate::view::{Inspector, Page, ThreatRow, TreeView};

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
        })
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

/// A message for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// Unique, to dismiss it.
    pub id: u64,
    /// Severity.
    pub severity: Severity,
    /// The text.
    pub text: String,
}

/// The data of the page on screen. Only the fields of that page are
/// filled; the rest stay empty (the session builds one page per change).
#[derive(Clone, Debug, Default)]
pub struct PageData {
    /// Overview page.
    pub overview: Option<Overview>,
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

/// The open project.
pub struct ProjectView {
    /// Which project.
    pub entry: ProjectEntry,
    /// Read-only or editing.
    pub mode: Mode,
    /// Who holds the edit lock, if anyone.
    pub lock_holder: Option<String>,
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
    /// Whether the residual evaluation is shown.
    pub show_residual: bool,
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
