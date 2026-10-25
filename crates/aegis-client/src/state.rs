//! The state the UI renders: one immutable snapshot per change.

use std::sync::Arc;

use aegis_canvas::Scene;
use aegis_model::NodeIdx;

use crate::view::{Inspector, ThreatRow, TreeView};

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
    },
    /// Not reachable; the session keeps retrying.
    Offline {
        /// Why, in words for the user.
        reason: String,
    },
    /// The server needs a login.
    LoginRequired,
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
}
