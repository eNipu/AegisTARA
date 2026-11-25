//! Suggestion branches: proposed operations kept apart from a [`Project`] until
//! accepted or rejected (job 089, FND-05).
//!
//! A [`SuggestionBranch`] holds a bounded list of [`SuggestionItem`]s proposed
//! by an AI model (`ProvenanceSource::Ai`) or catalog expander. None of the
//! operations in a [`SuggestionBranch`] touch the live [`Project`] while the
//! branch is pending:
//!
//! - Canvas and table views inspect the branch's [`GhostPreview`]s (or call
//!   [`SuggestionBranch::preview_tree`] / [`SuggestionBranch::validate`]) to
//!   render ghost nodes on the attack-tree canvas and ghost rows in tables.
//! - Accepting one item ([`SuggestionBranch::accept_one`]) or accepting the
//!   entire branch ([`SuggestionBranch::accept_all`]) rebases any attack-tree
//!   node insertion positions against the current [`Project`], stamps every
//!   emitted [`Op`] with the branch's [`ProvenanceSource`] (defaulting to
//!   [`ProvenanceSource::Ai`]) and the accepting user's actor + timestamp,
//!   verifies that the operations apply cleanly to a scratch clone of the
//!   [`Project`], removes the accepted item(s) from the branch, and returns the
//!   ready-to-apply `Vec<Op>`.
//! - Rejecting one item ([`SuggestionBranch::reject_one`]) or rejecting the
//!   entire branch ([`SuggestionBranch::reject_all`]) drops the proposal(s)
//!   without touching the [`Project`].

use std::sync::Arc;

use crate::description::ElementRef;
use crate::error::{EntityKind, ModelError};
use crate::id::{NodeId, SuggestionId, TreeId};
use crate::methodology::{CatalogRef, FeasibilityLevel};
use crate::op::{Op, StepTarget};
use crate::project::{MAX_NAME_CHARS, Project};
use crate::provenance::{Provenance, ProvenanceSource};
use crate::tree::{AttackTree, Node};

/// Most suggestion items held in one [`SuggestionBranch`] at once (R4).
pub const MAX_SUGGESTIONS_PER_BRANCH: usize = 64;

/// Most citations attached to one [`SuggestionItem`] (R4).
pub const MAX_CITATIONS_PER_ITEM: usize = 16;

/// Most ops carried by a single [`SuggestionItem`] (R4).
pub const MAX_OPS_PER_SUGGESTION: usize = 32;

/// Why a suggestion item or branch operation was refused.
///
/// Carries no customer entity names or label text (rule R6).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SuggestionError {
    /// A suggestion item must carry at least one [`Op`].
    #[error("suggestion item has no operations")]
    EmptyOps,
    /// A suggestion item carries more than [`MAX_OPS_PER_SUGGESTION`] operations.
    #[error("suggestion item exceeds the {MAX_OPS_PER_SUGGESTION}-operation limit")]
    TooManyOps,
    /// A suggestion item's title is empty or blank.
    #[error("suggestion title is empty")]
    EmptyTitle,
    /// A suggestion item's title is longer than [`MAX_NAME_CHARS`] characters.
    #[error("suggestion title exceeds {MAX_NAME_CHARS} characters")]
    TitleTooLong,
    /// A suggestion item carries more than [`MAX_CITATIONS_PER_ITEM`] citations.
    #[error("suggestion item exceeds the {MAX_CITATIONS_PER_ITEM}-citation limit")]
    TooManyCitations,
    /// A citation note is empty or longer than [`MAX_NAME_CHARS`] characters.
    #[error("citation note must be between 1 and {MAX_NAME_CHARS} characters")]
    InvalidCitationNote,
    /// The branch already holds [`MAX_SUGGESTIONS_PER_BRANCH`] items.
    #[error("suggestion branch capacity of {MAX_SUGGESTIONS_PER_BRANCH} items exceeded")]
    BranchCapacityExceeded,
    /// An item with this [`SuggestionId`] is already in the branch.
    #[error("suggestion ID {0} already exists in the branch")]
    DuplicateSuggestion(SuggestionId),
    /// No item with this [`SuggestionId`] exists in the branch.
    #[error("suggestion ID {0} is not in the branch")]
    UnknownSuggestion(SuggestionId),
    /// The domain model rejected the proposed operation(s).
    #[error("{0}")]
    Model(#[from] ModelError),
}

/// A grounded reference supporting a [`SuggestionItem`] (FND-06).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Citation {
    /// A catalog reference in the project's methodology (for example UN R155
    /// Annex 5 `5.2` or EU CRA `CRA-1.1`).
    Catalog(CatalogRef),
    /// An existing element in the project.
    Element(ElementRef),
    /// A short grounded rationale or model confidence tag (1..=[`MAX_NAME_CHARS`] chars).
    Note(Arc<str>),
}

impl Citation {
    /// Creates a grounded rationale or confidence note citation after checking
    /// that it is non-blank and at most [`MAX_NAME_CHARS`] characters.
    ///
    /// # Errors
    ///
    /// Returns [`SuggestionError::InvalidCitationNote`] if `note` is blank or
    /// longer than [`MAX_NAME_CHARS`] characters.
    pub fn note(note: impl Into<Arc<str>>) -> Result<Self, SuggestionError> {
        let note = note.into();
        let trimmed = note.trim();
        if trimmed.is_empty() || note.chars().count() > MAX_NAME_CHARS {
            return Err(SuggestionError::InvalidCitationNote);
        }
        Ok(Self::Note(note))
    }
}

/// Summary of how a [`SuggestionItem`] appears as a ghost node on the canvas
/// or a ghost row in a table before it is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GhostPreview {
    /// Adds a step or gate node under `parent` in `tree`.
    TreeNode {
        /// Target attack tree.
        tree: TreeId,
        /// Parent gate in the attack tree.
        parent: NodeId,
        /// Proposed node ID.
        id: NodeId,
        /// Proposed node (gate or step).
        node: Node,
        /// Optional proposed direct feasibility rating on the step.
        feasibility: Option<FeasibilityLevel>,
    },
    /// Adds a top-level entity row to a table.
    Entity {
        /// Kind of entity proposed.
        kind: EntityKind,
        /// Display name of the proposed entity.
        name: Arc<str>,
    },
    /// Modifies an existing element without adding a new node or entity row.
    Modification {
        /// Human-readable summary of the change.
        summary: Arc<str>,
    },
}

/// A single ghost node to overlay onto an [`AttackTree`] canvas scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhostTreeNode {
    /// The suggestion item that proposed this node.
    pub suggestion_id: SuggestionId,
    /// The proposed node's ID in the preview tree.
    pub node_id: NodeId,
    /// The parent gate in the preview tree.
    pub parent_id: NodeId,
    /// Optional direct feasibility rating proposed for this step.
    pub feasibility: Option<FeasibilityLevel>,
}

/// One individually acceptable or rejectable proposal in a [`SuggestionBranch`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionItem {
    id: SuggestionId,
    title: Arc<str>,
    ops: Vec<Op>,
    citations: Vec<Citation>,
    preview: GhostPreview,
}

impl SuggestionItem {
    /// Creates a validated [`SuggestionItem`] and derives its [`GhostPreview`]
    /// from `ops`.
    ///
    /// # Errors
    ///
    /// - [`SuggestionError::EmptyTitle`] if `title` is blank.
    /// - [`SuggestionError::TitleTooLong`] if `title` exceeds [`MAX_NAME_CHARS`].
    /// - [`SuggestionError::EmptyOps`] if `ops` is empty.
    /// - [`SuggestionError::TooManyOps`] if `ops.len() > MAX_OPS_PER_SUGGESTION`.
    /// - [`SuggestionError::TooManyCitations`] if `citations.len() > MAX_CITATIONS_PER_ITEM`.
    /// - [`SuggestionError::InvalidCitationNote`] if any [`Citation::Note`] is
    ///   blank or exceeds [`MAX_NAME_CHARS`].
    pub fn new(
        id: SuggestionId,
        title: impl Into<Arc<str>>,
        ops: Vec<Op>,
        citations: Vec<Citation>,
    ) -> Result<Self, SuggestionError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(SuggestionError::EmptyTitle);
        }
        if title.chars().count() > MAX_NAME_CHARS {
            return Err(SuggestionError::TitleTooLong);
        }
        if ops.is_empty() {
            return Err(SuggestionError::EmptyOps);
        }
        if ops.len() > MAX_OPS_PER_SUGGESTION {
            return Err(SuggestionError::TooManyOps);
        }
        if citations.len() > MAX_CITATIONS_PER_ITEM {
            return Err(SuggestionError::TooManyCitations);
        }
        for c in &citations {
            if let Citation::Note(n) = c
                && (n.trim().is_empty() || n.chars().count() > MAX_NAME_CHARS)
            {
                return Err(SuggestionError::InvalidCitationNote);
            }
        }
        let preview = derive_preview(&title, &ops);
        Ok(Self {
            id,
            title,
            ops,
            citations,
            preview,
        })
    }

    /// Stable ID of this suggestion item.
    #[must_use]
    pub const fn id(&self) -> SuggestionId {
        self.id
    }

    /// Human-readable title summarizing the proposal.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The proposed operations that will be applied if this item is accepted.
    #[must_use]
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// Grounded citations supporting this proposal.
    #[must_use]
    pub fn citations(&self) -> &[Citation] {
        &self.citations
    }

    /// Derived ghost preview for canvas and table views.
    #[must_use]
    pub const fn preview(&self) -> &GhostPreview {
        &self.preview
    }
}

fn derive_preview(title: &Arc<str>, ops: &[Op]) -> GhostPreview {
    for op in ops {
        match op {
            Op::AddTreeNode {
                tree,
                parent,
                id,
                node,
                ..
            } => {
                let feasibility = ops.iter().find_map(|other| match other {
                    Op::SetStepFeasibility {
                        target: StepTarget::Step(t, n),
                        level: Some(lvl),
                        ..
                    } if t == tree && n == id => Some(*lvl),
                    _ => None,
                });
                return GhostPreview::TreeNode {
                    tree: *tree,
                    parent: *parent,
                    id: *id,
                    node: node.clone(),
                    feasibility,
                };
            }
            Op::AddAsset { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::Asset,
                    name: Arc::clone(name),
                };
            }
            Op::AddDamageScenario { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::DamageScenario,
                    name: Arc::clone(name),
                };
            }
            Op::AddThreatScenario { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::ThreatScenario,
                    name: Arc::clone(name),
                };
            }
            Op::AddAttackTree { tree, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::AttackTree,
                    name: Arc::from(tree.get(tree.root()).map_or("", Node::label)),
                };
            }
            Op::AddSecurityControl { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::SecurityControl,
                    name: Arc::clone(name),
                };
            }
            Op::AddCybersecurityGoal { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::CybersecurityGoal,
                    name: Arc::clone(name),
                };
            }
            Op::AddCybersecurityClaim { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::CybersecurityClaim,
                    name: Arc::clone(name),
                };
            }
            Op::AddComponent { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::Component,
                    name: Arc::clone(name),
                };
            }
            Op::AddInterface { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::Interface,
                    name: Arc::clone(name),
                };
            }
            Op::AddDataFlow { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::DataFlow,
                    name: Arc::clone(name),
                };
            }
            Op::AddTrustBoundary { name, .. } => {
                return GhostPreview::Entity {
                    kind: EntityKind::TrustBoundary,
                    name: Arc::clone(name),
                };
            }
            _ => {}
        }
    }
    GhostPreview::Modification {
        summary: Arc::clone(title),
    }
}

/// Rebases one [`Op`] against `project`'s current state and stamps its
/// provenance with `provenance`.
///
/// Specifically, `Op::AddTreeNode` carries `index` (the tree's current arena
/// length) and `position` (the parent gate's child count when appending). When
/// items in a [`SuggestionBranch`] are accepted out of order or after other
/// edits, rebasing `index` to `tree.len()` and clamping `position` to the
/// parent's current child count ensures that independent sibling proposals
/// apply cleanly in any order.
fn rebase_and_stamp_op(op: &Op, project: &Project, provenance: Provenance) -> Op {
    let mut rebased = op.clone().with_provenance(provenance);
    if let Op::AddTreeNode {
        tree,
        parent,
        index,
        position,
        ..
    } = &mut rebased
        && let Some(t_idx) = project.attack_tree_idx(tree)
        && let Some(t) = project.attack_tree(t_idx)
    {
        *index = u32::try_from(t.len()).unwrap_or(u32::MAX);
        if let Some(p_idx) = t.idx_of(parent) {
            let child_count = u32::try_from(t.children(p_idx).len()).unwrap_or(u32::MAX);
            if *position > child_count {
                *position = child_count;
            }
        }
    }
    rebased
}

/// A branch of proposed [`SuggestionItem`]s kept apart from a [`Project`]
/// until accepted or rejected (FND-05).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{
///     AttackTree, GateKind, Methodology, Node, NodeId, Op, Project, Provenance,
///     ProvenanceSource, SuggestionBranch, SuggestionId, SuggestionItem, TreeId,
/// };
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let tree_id = TreeId::new_v7();
/// let root_id = NodeId::new_v7();
/// let tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "Compromise ECU"))?;
/// project.apply(Op::AddAttackTree {
///     id: tree_id,
///     tree,
///     step_feasibility: Vec::new(),
///     provenance: Provenance::human("alice", 1_000),
/// })?;
///
/// let mut branch = SuggestionBranch::new(SuggestionId::new_v7());
/// let item_id = SuggestionId::new_v7();
/// let step_id = NodeId::new_v7();
/// branch.push(SuggestionItem::new(
///     item_id,
///     "Replay diagnostic session",
///     vec![Op::AddTreeNode {
///         tree: tree_id,
///         parent: root_id,
///         id: step_id,
///         node: Node::step("Replay diagnostic session"),
///         index: 1,
///         position: 0,
///         provenance: Provenance::default(),
///     }],
///     Vec::new(),
/// )?)?;
///
/// // Project is untouched while the suggestion is pending.
/// let t_idx = project.attack_tree_idx(&tree_id).ok_or("missing tree")?;
/// assert_eq!(project.attack_tree(t_idx).map(|t| t.len()), Some(1));
///
/// // Accepting stamps `ProvenanceSource::Ai` with the accepting user.
/// let ops = branch.accept_one(item_id, &project, "alice", 2_000)?;
/// for op in ops {
///     project.apply(op)?;
/// }
/// assert_eq!(project.attack_tree(t_idx).map(|t| t.len()), Some(2));
/// assert_eq!(
///     project.attack_tree_provenance(t_idx).map(Provenance::source),
///     Some(ProvenanceSource::Ai)
/// );
/// assert_eq!(
///     project.attack_tree_provenance(t_idx).map(Provenance::actor),
///     Some("alice")
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionBranch {
    id: SuggestionId,
    source: ProvenanceSource,
    items: Vec<SuggestionItem>,
}

impl SuggestionBranch {
    /// Creates an empty [`SuggestionBranch`] with [`ProvenanceSource::Ai`].
    #[must_use]
    pub const fn new(id: SuggestionId) -> Self {
        Self {
            id,
            source: ProvenanceSource::Ai,
            items: Vec::new(),
        }
    }

    /// Creates an empty [`SuggestionBranch`] with an explicit [`ProvenanceSource`].
    #[must_use]
    pub const fn with_source(id: SuggestionId, source: ProvenanceSource) -> Self {
        Self {
            id,
            source,
            items: Vec::new(),
        }
    }

    /// Stable ID of this suggestion branch.
    #[must_use]
    pub const fn id(&self) -> SuggestionId {
        self.id
    }

    /// Provenance source stamped onto operations accepted from this branch.
    #[must_use]
    pub const fn source(&self) -> ProvenanceSource {
        self.source
    }

    /// Remaining pending items in this branch, in insertion order.
    #[must_use]
    pub fn items(&self) -> &[SuggestionItem] {
        &self.items
    }

    /// Number of remaining pending items in this branch.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` when the branch has no pending items left.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Looks up a pending item by its [`SuggestionId`].
    #[must_use]
    pub fn get(&self, id: SuggestionId) -> Option<&SuggestionItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Appends a [`SuggestionItem`] to the branch.
    ///
    /// # Errors
    ///
    /// - [`SuggestionError::BranchCapacityExceeded`] if the branch already
    ///   holds [`MAX_SUGGESTIONS_PER_BRANCH`] items.
    /// - [`SuggestionError::DuplicateSuggestion`] if an item with `item.id()`
    ///   is already in the branch.
    pub fn push(&mut self, item: SuggestionItem) -> Result<(), SuggestionError> {
        if self.items.len() >= MAX_SUGGESTIONS_PER_BRANCH {
            return Err(SuggestionError::BranchCapacityExceeded);
        }
        if self.items.iter().any(|existing| existing.id == item.id) {
            return Err(SuggestionError::DuplicateSuggestion(item.id));
        }
        self.items.push(item);
        Ok(())
    }

    /// Validates that every pending item in this branch applies cleanly to a
    /// scratch clone of `project`, returning the resulting preview [`Project`].
    /// `project` itself is never modified.
    ///
    /// # Errors
    ///
    /// Returns [`SuggestionError::Model`] if any operation in the branch is
    /// rejected by [`Project::apply`].
    pub fn validate(&self, project: &Project) -> Result<Project, SuggestionError> {
        let mut scratch = project.clone();
        let prov = Provenance::new("preview", 0, self.source);
        for item in &self.items {
            for op in &item.ops {
                let rebased = rebase_and_stamp_op(op, &scratch, prov.clone());
                scratch.apply(rebased)?;
            }
        }
        Ok(scratch)
    }

    /// Builds a preview [`AttackTree`] for `tree_id` containing both the
    /// committed nodes in `project` and any ghost nodes proposed for `tree_id`
    /// in this branch. Returns `None` if `tree_id` does not exist in `project`
    /// or if no pending items in this branch add nodes to `tree_id`.
    #[must_use]
    pub fn preview_tree(
        &self,
        project: &Project,
        tree_id: TreeId,
    ) -> Option<(AttackTree, Vec<GhostTreeNode>)> {
        let t_idx = project.attack_tree_idx(&tree_id)?;
        let mut tree = project.attack_tree(t_idx)?.clone();
        let mut ghosts = Vec::new();
        for item in &self.items {
            if let GhostPreview::TreeNode {
                tree: target_tree,
                parent,
                id,
                node,
                feasibility,
            } = &item.preview
                && *target_tree == tree_id
                && let Some(parent_idx) = tree.idx_of(parent)
                && tree.add_child(parent_idx, *id, node.clone()).is_ok()
            {
                ghosts.push(GhostTreeNode {
                    suggestion_id: item.id,
                    node_id: *id,
                    parent_id: *parent,
                    feasibility: *feasibility,
                });
            }
        }
        if ghosts.is_empty() {
            None
        } else {
            Some((tree, ghosts))
        }
    }

    /// Accepts one pending [`SuggestionItem`] by `id` against `project`,
    /// stamping every emitted [`Op`] with `self.source()` and the accepting
    /// user (`actor`, `timestamp_ms`), verifying the ops on a scratch clone of
    /// `project`, removing the item from the branch, and returning the
    /// verified `Vec<Op>`.
    ///
    /// # Errors
    ///
    /// - [`SuggestionError::UnknownSuggestion`] if `id` is not in the branch.
    /// - [`SuggestionError::Model`] if the item's operations are rejected by
    ///   `project` (in which case the branch is left unchanged).
    pub fn accept_one(
        &mut self,
        id: SuggestionId,
        project: &Project,
        actor: &str,
        timestamp_ms: u64,
    ) -> Result<Vec<Op>, SuggestionError> {
        let pos = self
            .items
            .iter()
            .position(|item| item.id == id)
            .ok_or(SuggestionError::UnknownSuggestion(id))?;
        let provenance = Provenance::new(actor, timestamp_ms, self.source);
        let mut scratch = project.clone();
        let mut stamped = Vec::with_capacity(self.items[pos].ops.len());
        for op in &self.items[pos].ops {
            let rebased = rebase_and_stamp_op(op, &scratch, provenance.clone());
            scratch.apply(rebased.clone())?;
            stamped.push(rebased);
        }
        self.items.remove(pos);
        Ok(stamped)
    }

    /// Accepts all remaining [`SuggestionItem`]s in the branch in order
    /// against `project`, stamping every emitted [`Op`] with `self.source()`
    /// and the accepting user (`actor`, `timestamp_ms`), verifying the batch
    /// on a scratch clone of `project`, clearing the branch, and returning the
    /// verified `Vec<Op>`.
    ///
    /// # Errors
    ///
    /// Returns [`SuggestionError::Model`] if any operation in the batch is
    /// rejected by `project` (in which case the branch is left unchanged).
    pub fn accept_all(
        &mut self,
        project: &Project,
        actor: &str,
        timestamp_ms: u64,
    ) -> Result<Vec<Op>, SuggestionError> {
        let provenance = Provenance::new(actor, timestamp_ms, self.source);
        let mut scratch = project.clone();
        let total_ops: usize = self.items.iter().map(|i| i.ops.len()).sum();
        let mut stamped = Vec::with_capacity(total_ops);
        for item in &self.items {
            for op in &item.ops {
                let rebased = rebase_and_stamp_op(op, &scratch, provenance.clone());
                scratch.apply(rebased.clone())?;
                stamped.push(rebased);
            }
        }
        self.items.clear();
        Ok(stamped)
    }

    /// Rejects and removes one pending [`SuggestionItem`] by `id` without
    /// touching any [`Project`]. Returns `true` if the item was present.
    pub fn reject_one(&mut self, id: SuggestionId) -> bool {
        if let Some(pos) = self.items.iter().position(|item| item.id == id) {
            self.items.remove(pos);
            true
        } else {
            false
        }
    }

    /// Rejects and removes all remaining pending [`SuggestionItem`]s in the
    /// branch without touching any [`Project`], returning how many items were
    /// discarded.
    pub fn reject_all(&mut self) -> usize {
        let count = self.items.len();
        self.items.clear();
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::AssetId;
    use crate::methodology::Methodology;
    use crate::tree::GateKind;

    fn sample_project_with_tree() -> (Project, TreeId, NodeId) {
        let m = Arc::new(Methodology::iso21434_r155().expect("builtin pack"));
        let mut project = Project::new(m);
        let tree_id = TreeId::new_v7();
        let root_id = NodeId::new_v7();
        let tree =
            AttackTree::new(root_id, Node::gate(GateKind::Or, "Unlock doors")).expect("valid root");
        project
            .apply(Op::AddAttackTree {
                id: tree_id,
                tree,
                step_feasibility: Vec::new(),
                provenance: Provenance::human("author", 100),
            })
            .expect("add tree");
        (project, tree_id, root_id)
    }

    #[test]
    fn suggestion_branch_keeps_ops_apart_until_accepted_one_by_one() {
        let (mut project, tree_id, root_id) = sample_project_with_tree();
        let high = project
            .methodology()
            .feasibility_level("high")
            .expect("high feasibility");
        let low = project
            .methodology()
            .feasibility_level("low")
            .expect("low feasibility");

        let mut branch = SuggestionBranch::new(SuggestionId::new_v7());
        let s1 = SuggestionId::new_v7();
        let s2 = SuggestionId::new_v7();
        let n1 = NodeId::new_v7();
        let n2 = NodeId::new_v7();

        branch
            .push(
                SuggestionItem::new(
                    s1,
                    "Relay key fob signal",
                    vec![
                        Op::AddTreeNode {
                            tree: tree_id,
                            parent: root_id,
                            id: n1,
                            node: Node::step("Relay key fob signal"),
                            index: 1,
                            position: 0,
                            provenance: Provenance::default(),
                        },
                        Op::SetStepFeasibility {
                            target: StepTarget::Step(tree_id, n1),
                            level: Some(high),
                            provenance: Provenance::default(),
                        },
                    ],
                    vec![
                        Citation::Element(ElementRef::Tree(tree_id)),
                        Citation::note("Gear 1 top-1 completion (p=0.94)").expect("valid note"),
                    ],
                )
                .expect("valid item 1"),
            )
            .expect("push 1");

        branch
            .push(
                SuggestionItem::new(
                    s2,
                    "Inject CAN unlock frame",
                    vec![
                        Op::AddTreeNode {
                            tree: tree_id,
                            parent: root_id,
                            id: n2,
                            node: Node::step("Inject CAN unlock frame"),
                            index: 2,
                            position: 1,
                            provenance: Provenance::default(),
                        },
                        Op::SetStepFeasibility {
                            target: StepTarget::Step(tree_id, n2),
                            level: Some(low),
                            provenance: Provenance::default(),
                        },
                    ],
                    Vec::new(),
                )
                .expect("valid item 2"),
            )
            .expect("push 2");

        // Live project is untouched while branch is pending.
        let t_idx = project.attack_tree_idx(&tree_id).expect("tree exists");
        assert_eq!(project.attack_tree(t_idx).expect("tree").len(), 1);

        // Preview tree includes both ghost nodes without mutating `project`.
        let (preview_tree, ghosts) = branch
            .preview_tree(&project, tree_id)
            .expect("preview has ghosts");
        assert_eq!(preview_tree.len(), 3);
        assert_eq!(ghosts.len(), 2);
        assert_eq!(ghosts[0].node_id, n1);
        assert_eq!(ghosts[0].feasibility, Some(high));
        assert_eq!(ghosts[1].node_id, n2);
        assert_eq!(ghosts[1].feasibility, Some(low));
        assert_eq!(project.attack_tree(t_idx).expect("tree").len(), 1);

        // Accept the second item FIRST (out of order): rebasing adjusts index & position!
        let accepted_ops = branch
            .accept_one(s2, &project, "reviewer", 5_000)
            .expect("accept second item first");
        assert_eq!(branch.len(), 1);
        for op in accepted_ops {
            assert_eq!(op.provenance(), Some(&Provenance::ai("reviewer", 5_000)));
            project.apply(op).expect("apply accepted op");
        }
        assert_eq!(project.attack_tree(t_idx).expect("tree").len(), 2);

        // Reject the remaining first item: project stays at 2 nodes.
        assert!(branch.reject_one(s1));
        assert!(branch.is_empty());
        assert_eq!(project.attack_tree(t_idx).expect("tree").len(), 2);
    }

    #[test]
    fn suggestion_branch_accept_all_and_reject_all() {
        let (mut project, tree_id, root_id) = sample_project_with_tree();
        let mut branch = SuggestionBranch::new(SuggestionId::new_v7());
        for i in 0..3 {
            let step_id = NodeId::new_v7();
            branch
                .push(
                    SuggestionItem::new(
                        SuggestionId::new_v7(),
                        format!("Suggested step {i}"),
                        vec![Op::AddTreeNode {
                            tree: tree_id,
                            parent: root_id,
                            id: step_id,
                            node: Node::step(format!("Suggested step {i}")),
                            index: 1 + i,
                            position: i,
                            provenance: Provenance::default(),
                        }],
                        Vec::new(),
                    )
                    .expect("valid item"),
                )
                .expect("push");
        }

        let preview = branch.validate(&project).expect("valid branch");
        let t_idx = project.attack_tree_idx(&tree_id).expect("tree");
        assert_eq!(preview.attack_tree(t_idx).expect("tree").len(), 4);
        assert_eq!(project.attack_tree(t_idx).expect("tree").len(), 1);

        let ops = branch
            .accept_all(&project, "bob", 9_000)
            .expect("accept all");
        assert!(branch.is_empty());
        for op in ops {
            assert_eq!(op.provenance(), Some(&Provenance::ai("bob", 9_000)));
            project.apply(op).expect("apply");
        }
        assert_eq!(project.attack_tree(t_idx).expect("tree").len(), 4);

        // Reject-all on a second branch discards all items without touching project.
        let mut branch2 = SuggestionBranch::new(SuggestionId::new_v7());
        branch2
            .push(
                SuggestionItem::new(
                    SuggestionId::new_v7(),
                    "Add Telematics Asset",
                    vec![Op::AddAsset {
                        id: AssetId::new_v7(),
                        name: "Telematics Gateway".into(),
                        properties: project.methodology().all_properties(),
                        provenance: Provenance::default(),
                    }],
                    Vec::new(),
                )
                .expect("valid asset suggestion"),
            )
            .expect("push asset suggestion");
        assert_eq!(branch2.reject_all(), 1);
        assert!(branch2.is_empty());
        assert_eq!(project.assets().count(), 0);
    }

    #[test]
    fn suggestion_item_enforces_limits_and_rejects_duplicates() {
        assert_eq!(
            SuggestionItem::new(SuggestionId::new_v7(), "   ", Vec::new(), Vec::new()),
            Err(SuggestionError::EmptyTitle)
        );
        assert_eq!(
            SuggestionItem::new(
                SuggestionId::new_v7(),
                "Valid title",
                Vec::new(),
                Vec::new()
            ),
            Err(SuggestionError::EmptyOps)
        );

        let dup_id = SuggestionId::new_v7();
        let item = SuggestionItem::new(
            dup_id,
            "Add asset",
            vec![Op::AddAsset {
                id: AssetId::new_v7(),
                name: "Gateway".into(),
                properties: crate::asset::PropertySet::empty(),
                provenance: Provenance::default(),
            }],
            Vec::new(),
        )
        .expect("valid item");

        let mut branch = SuggestionBranch::new(SuggestionId::new_v7());
        branch.push(item.clone()).expect("first push");
        assert_eq!(
            branch.push(item),
            Err(SuggestionError::DuplicateSuggestion(dup_id))
        );
    }
}
