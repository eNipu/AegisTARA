//! What the user can ask for.
//!
//! [`Command`] is the whole interface between a UI and the session. Edits
//! refer to dropdown *positions* (the index into the list the view model
//! offered), never to model internals; [`Edit::to_ops`] resolves them against
//! the methodology and builds the typed `Op`s.

use aegis_model::{NodeId, Op, Project, Provenance, ThreatId, TreeId};

/// A change to the project, as the UI states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Sets or clears the direct feasibility rating of a step.
    SetStepFeasibility {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// Position in the feasibility levels, or `None` for unrated.
        level: Option<usize>,
    },
    /// Sets or clears one attack potential factor of a step.
    SetStepFactor {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// Position in the methodology's factors.
        factor: usize,
        /// Position in that factor's levels, or `None` for unrated.
        level: Option<usize>,
    },
    /// Renames a threat scenario.
    RenameThreat {
        /// The threat.
        threat: ThreatId,
        /// The new name.
        name: String,
    },
}

/// Why an edit could not be turned into ops.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// The feasibility level position does not exist.
    #[error("unknown feasibility level")]
    UnknownLevel,
    /// The factor position does not exist.
    #[error("unknown factor")]
    UnknownFactor,
    /// The factor level position does not exist.
    #[error("unknown factor level")]
    UnknownFactorLevel,
}

impl Edit {
    /// Builds the ops for this edit against `project`'s methodology. The
    /// provenance records who made the change and when.
    ///
    /// # Errors
    ///
    /// [`EditError`] if a position does not exist in the methodology.
    pub fn to_ops(
        &self,
        project: &Project,
        actor: &str,
        now_ms: u64,
    ) -> Result<Vec<Op>, EditError> {
        let m = project.methodology();
        let provenance = Provenance::human(actor, now_ms);
        Ok(match self {
            Edit::SetStepFeasibility { tree, node, level } => {
                let level = match level {
                    Some(i) => Some(
                        m.feasibility_levels()
                            .nth(*i)
                            .ok_or(EditError::UnknownLevel)?
                            .0,
                    ),
                    None => None,
                };
                vec![Op::SetStepFeasibility {
                    tree: *tree,
                    node: *node,
                    level,
                    provenance,
                }]
            }
            Edit::SetStepFactor {
                tree,
                node,
                factor,
                level,
            } => {
                let f = m
                    .feasibility_factors()
                    .nth(*factor)
                    .ok_or(EditError::UnknownFactor)?;
                let level = match level {
                    Some(i) => Some(
                        m.factor_levels(f)
                            .nth(*i)
                            .ok_or(EditError::UnknownFactorLevel)?,
                    ),
                    None => None,
                };
                vec![Op::SetStepFactor {
                    tree: *tree,
                    node: *node,
                    factor: f,
                    level,
                    provenance,
                }]
            }
            Edit::RenameThreat { threat, name } => vec![Op::RenameThreatScenario {
                id: *threat,
                name: name.as_str().into(),
                provenance,
            }],
        })
    }
}

/// Everything a UI can tell the session to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Connect again (after a login, or after the server was unreachable).
    Reconnect,
    /// Reload the project list.
    Refresh,
    /// Create an empty project and open it.
    CreateProject {
        /// Display name.
        name: String,
        /// Methodology key.
        methodology_key: String,
    },
    /// Open a project (read-only until [`Command::BeginEdit`]).
    OpenProject {
        /// The project ID as listed.
        id: String,
    },
    /// Close the open project.
    CloseProject,
    /// Show the tree of the threat at this position in the threat list.
    SelectThreat(usize),
    /// Select the node at this position in the scene, or none.
    SelectNode(Option<usize>),
    /// Show the residual (after controls) evaluation instead of the inherent one.
    ShowResidual(bool),
    /// Take the edit lock.
    BeginEdit,
    /// Finish pending edits and give the edit lock up.
    EndEdit,
    /// Change the project.
    Edit(Edit),
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Fill an empty project with a small example TARA.
    CreateSample,
    /// Remove a notice from the list.
    DismissNotice(u64),
    /// Stop the session.
    Shutdown,
}
