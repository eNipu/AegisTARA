//! The client's copy of a project and the rules for changing it.
//!
//! A [`Workspace`] is pure state: no network, no tasks, no clock. It holds
//! the project, its incremental evaluation cache (rule A1), the edits that
//! were applied locally but not yet acknowledged by the server, and the undo
//! and redo stacks. Everything the session does to the project goes through
//! it, so the optimistic-edit protocol is testable without a server.
//!
//! # The protocol
//!
//! 1. An edit is applied **at once** to the local project (`apply_cached`,
//!    within the 5 ms budget) and queued as a *pending batch* with its
//!    inverse ops. The user sees the result in the next frame.
//! 2. [`next_submission`](Workspace::next_submission) hands all queued
//!    batches to the session as one `SubmitOps` call; only one call is in
//!    flight at a time, so the server's sequence number is always the
//!    confirmed one and the base check cannot be raced.
//! 3. [`on_accepted`](Workspace::on_accepted) moves the confirmed sequence
//!    forward. [`on_rejected`](Workspace::on_rejected) undoes **every**
//!    pending batch newest first, because later batches may depend on the
//!    rejected one, and clears undo history, which can no longer be trusted.
//!
//! # Undo
//!
//! Undo and redo are ordinary edits: the ops that reverse the last edit are
//! applied and submitted like any other, and their own inverses become the
//! redo entry. So the server's log never rewrites history, and undo works
//! across reconnects.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::undo_label::{Label, describe};
use aegis_model::{
    Methodology, ModelError, Op, Project, ProjectEvalCache, SuggestionBranch, SuggestionError,
    SuggestionId,
};

/// Most edits that may wait for the server at once (R4). Past it, edits are
/// refused until the server catches up.
pub const MAX_PENDING_BATCHES: usize = 1024;
/// Most steps kept for undo.
pub const MAX_UNDO_STEPS: usize = 1000;
/// Most ops sent in one `SubmitOps` call. The server's limit is
/// `ServerConfig::max_batch_ops` (1024); one edit larger than this is
/// refused before it is applied, and queued edits are sent in submissions of
/// at most this many ops.
pub const MAX_SUBMIT_OPS: usize = 1024;

/// Why an operation on the workspace was refused.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum WorkspaceError {
    /// The model refused an op; nothing was changed.
    #[error("the model refused the edit: {0}")]
    Model(#[from] ModelError),
    /// The undo stack is empty.
    #[error("nothing to undo")]
    NothingToUndo,
    /// The redo stack is empty.
    #[error("nothing to redo")]
    NothingToRedo,
    /// Too many edits wait for the server.
    #[error("too many edits are waiting for the server")]
    TooManyPending,
    /// One edit has more ops than one submission may carry.
    #[error("the change is too large to save in one step ({ops} changes, at most {max})")]
    TooLarge {
        /// Ops in the edit.
        ops: usize,
        /// The limit.
        max: usize,
    },
    /// The server's answer does not match what was sent: resync.
    #[error("server sequence is {found}, expected {expected}")]
    Desync {
        /// What the client calculated.
        expected: u64,
        /// What the server said.
        found: u64,
    },
    /// Nothing is waiting to be sent.
    #[error("no edits are pending")]
    NothingPending,
    /// A suggestion operation failed.
    #[error("{0}")]
    Suggestion(#[from] SuggestionError),
}

/// Where an edit came from, which decides what happens to the undo stacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A user edit: clears redo.
    Edit,
    /// An undo: its inverse goes to redo.
    Undo,
    /// A redo: its inverse goes to undo.
    Redo,
}

/// One undo or redo step: the ops that reverse an edit, and what the edit was.
struct Step {
    ops: Vec<Op>,
    label: Label,
}

struct Batch {
    ops: Vec<Op>,
    /// Inverses in the order the ops were applied.
    inverses: Vec<Op>,
}

/// What to send to the server.
#[derive(Clone, Debug, PartialEq)]
pub struct Submission {
    /// The confirmed sequence number the ops build on.
    pub base_seq: u64,
    /// The ops, oldest first.
    pub ops: Vec<Op>,
}

/// The local project plus the machinery around it.
pub struct Workspace {
    project: Project,
    cache: ProjectEvalCache,
    confirmed_seq: u64,
    pending: VecDeque<Batch>,
    /// The first `in_flight` pending batches are with the server.
    in_flight: usize,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// History steps dropped by a rejection since the last
    /// [`take_cleared`](Self::take_cleared), to tell the user (UI-10.7).
    cleared: usize,
    suggestions: Option<SuggestionBranch>,
}

impl Workspace {
    /// A workspace over a project at server sequence `seq`. Builds the
    /// evaluation cache: O(project size).
    #[must_use]
    pub fn new(project: Project, seq: u64) -> Self {
        let cache = project.eval_cache();
        Workspace {
            project,
            cache,
            confirmed_seq: seq,
            pending: VecDeque::new(),
            in_flight: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            cleared: 0,
            suggestions: None,
        }
    }

    /// An empty project of a methodology at sequence 0.
    #[must_use]
    pub fn empty(methodology: Arc<Methodology>) -> Self {
        Self::new(Project::new(methodology), 0)
    }

    /// The project as the user sees it, pending edits included.
    #[must_use]
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// The evaluation cache, current with [`project`](Self::project).
    #[must_use]
    pub fn cache(&self) -> &ProjectEvalCache {
        &self.cache
    }

    /// The last sequence number the server confirmed.
    #[must_use]
    pub fn confirmed_seq(&self) -> u64 {
        self.confirmed_seq
    }

    /// Edits applied locally and not yet confirmed.
    #[must_use]
    pub fn pending_batches(&self) -> usize {
        self.pending.len()
    }

    /// Whether an edit is with the server right now.
    #[must_use]
    pub fn is_submitting(&self) -> bool {
        self.in_flight > 0
    }

    /// Whether there is something to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is something to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What [`undo`](Self::undo) would undo, for the tooltip and the toast.
    #[must_use]
    pub fn undo_label(&self) -> Option<&Label> {
        self.undo.last().map(|s| &s.label)
    }

    /// What [`redo`](Self::redo) would redo.
    #[must_use]
    pub fn redo_label(&self) -> Option<&Label> {
        self.redo.last().map(|s| &s.label)
    }

    /// How many history steps a rejection or a reload dropped since the
    /// last call; resets the count.
    pub fn take_cleared(&mut self) -> usize {
        std::mem::take(&mut self.cleared) + self.undo.len() + self.redo.len()
    }

    /// Applies `ops` as one edit: all or nothing. On success the edit is
    /// queued for the server. Time O(size of the ops) plus the incremental
    /// cache update.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::Model`] if an op is refused (the project is left as
    /// it was), [`WorkspaceError::TooManyPending`].
    pub fn apply(&mut self, ops: Vec<Op>, origin: Origin) -> Result<(), WorkspaceError> {
        self.apply_labeled(ops, origin, None)
    }

    /// [`apply`](Self::apply) with the description of the edit; `None`
    /// describes `ops` against the project as it is now.
    fn apply_labeled(
        &mut self,
        ops: Vec<Op>,
        origin: Origin,
        label: Option<Label>,
    ) -> Result<(), WorkspaceError> {
        if self.pending.len() >= MAX_PENDING_BATCHES {
            return Err(WorkspaceError::TooManyPending);
        }
        if ops.len() > MAX_SUBMIT_OPS {
            return Err(WorkspaceError::TooLarge {
                ops: ops.len(),
                max: MAX_SUBMIT_OPS,
            });
        }
        let label = label.unwrap_or_else(|| describe(&ops, &self.project));
        let mut inverses = Vec::with_capacity(ops.len());
        for op in ops.iter().cloned() {
            match self.project.apply_cached(op, &mut self.cache) {
                Ok(inverse) => inverses.push(inverse),
                Err(e) => {
                    self.undo_applied(inverses);
                    return Err(e.into());
                }
            }
        }
        let step = Step {
            ops: inverses.iter().rev().cloned().collect(),
            label,
        };
        match origin {
            Origin::Edit => {
                self.redo.clear();
                push_bounded(&mut self.undo, step);
            }
            Origin::Undo => push_bounded(&mut self.redo, step),
            Origin::Redo => push_bounded(&mut self.undo, step),
        }
        self.pending.push_back(Batch { ops, inverses });
        if let Some(branch) = &self.suggestions
            && (branch.is_empty() || branch.validate(&self.project).is_err())
        {
            self.suggestions = None;
        }
        Ok(())
    }

    /// Pending suggestion branch kept apart from [`project`](Self::project)
    /// until accepted or rejected (job 089, FND-05).
    #[must_use]
    pub fn suggestions(&self) -> Option<&SuggestionBranch> {
        self.suggestions.as_ref().filter(|b| !b.is_empty())
    }

    /// Installs or clears the active [`SuggestionBranch`], validating non-empty
    /// branches against [`project`](Self::project) first.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::Suggestion`] if `branch` is rejected by
    /// [`SuggestionBranch::validate`].
    pub fn set_suggestions(
        &mut self,
        branch: Option<SuggestionBranch>,
    ) -> Result<(), WorkspaceError> {
        match branch {
            Some(b) if !b.is_empty() => {
                b.validate(&self.project)?;
                self.suggestions = Some(b);
            }
            _ => {
                self.suggestions = None;
            }
        }
        Ok(())
    }

    /// Clears any active [`SuggestionBranch`].
    pub fn clear_suggestions(&mut self) {
        self.suggestions = None;
    }

    /// Accepts one pending suggestion item by `id`, stamping its operations
    /// with the branch's [`aegis_model::ProvenanceSource`] and `actor` /
    /// `timestamp_ms`, and applying them as an undoable edit queued for the
    /// server.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::Suggestion`] if `id` is unknown or invalid, or
    /// an error of [`apply`](Self::apply).
    pub fn accept_suggestion(
        &mut self,
        id: SuggestionId,
        actor: &str,
        timestamp_ms: u64,
    ) -> Result<(), WorkspaceError> {
        let mut branch = self
            .suggestions
            .take()
            .ok_or(SuggestionError::UnknownSuggestion(id))?;
        let ops = match branch.accept_one(id, &self.project, actor, timestamp_ms) {
            Ok(ops) => ops,
            Err(e) => {
                self.suggestions = Some(branch);
                return Err(e.into());
            }
        };
        if !branch.is_empty() {
            self.suggestions = Some(branch);
        }
        self.apply(ops, Origin::Edit)
    }

    /// Accepts all remaining items in the active [`SuggestionBranch`] in one
    /// undoable edit queued for the server. Returns the number of accepted
    /// suggestion items.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::Suggestion`] or an error of
    /// [`apply`](Self::apply).
    pub fn accept_all_suggestions(
        &mut self,
        actor: &str,
        timestamp_ms: u64,
    ) -> Result<usize, WorkspaceError> {
        let Some(mut branch) = self.suggestions.take() else {
            return Ok(0);
        };
        let count = branch.len();
        if count == 0 {
            return Ok(0);
        }
        let ops = match branch.accept_all(&self.project, actor, timestamp_ms) {
            Ok(ops) => ops,
            Err(e) => {
                self.suggestions = Some(branch);
                return Err(e.into());
            }
        };
        self.apply(ops, Origin::Edit)?;
        Ok(count)
    }

    /// Rejects one pending suggestion item by `id` without touching the
    /// project.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::Suggestion`] if `id` is not in the active
    /// branch.
    pub fn reject_suggestion(&mut self, id: SuggestionId) -> Result<(), WorkspaceError> {
        let branch = self
            .suggestions
            .as_mut()
            .ok_or(SuggestionError::UnknownSuggestion(id))?;
        if !branch.reject_one(id) {
            return Err(SuggestionError::UnknownSuggestion(id).into());
        }
        if branch.is_empty() {
            self.suggestions = None;
        }
        Ok(())
    }

    /// Rejects all pending suggestion items without touching the project.
    /// Returns how many items were discarded.
    pub fn reject_all_suggestions(&mut self) -> usize {
        self.suggestions.take().map_or(0, |mut b| b.reject_all())
    }

    /// Undoes the last edit by applying its reverse as a new edit.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::NothingToUndo`], or an error of [`apply`](Self::apply)
    /// (the entry stays on the stack if applying fails).
    pub fn undo(&mut self) -> Result<(), WorkspaceError> {
        let step = self.undo.pop().ok_or(WorkspaceError::NothingToUndo)?;
        self.apply_labeled(step.ops.clone(), Origin::Undo, Some(step.label.clone()))
            .inspect_err(|_| self.undo.push(step))
    }

    /// Redoes the last undone edit.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::NothingToRedo`] or an error of [`apply`](Self::apply).
    pub fn redo(&mut self) -> Result<(), WorkspaceError> {
        let step = self.redo.pop().ok_or(WorkspaceError::NothingToRedo)?;
        self.apply_labeled(step.ops.clone(), Origin::Redo, Some(step.label.clone()))
            .inspect_err(|_| self.redo.push(step))
    }

    /// Takes the oldest edits not yet sent, as many whole edits as fit in
    /// [`MAX_SUBMIT_OPS`], as one submission; `None` if a call is already in
    /// flight or nothing waits. The rest follow after the answer.
    pub fn next_submission(&mut self) -> Option<Submission> {
        if self.in_flight > 0 || self.pending.is_empty() {
            return None;
        }
        let mut ops = Vec::new();
        let mut batches = 0;
        for batch in &self.pending {
            // Every batch fits on its own (checked in `apply`).
            if batches > 0 && ops.len() + batch.ops.len() > MAX_SUBMIT_OPS {
                break;
            }
            ops.extend(batch.ops.iter().cloned());
            batches += 1;
        }
        self.in_flight = batches;
        Some(Submission {
            base_seq: self.confirmed_seq,
            ops,
        })
    }

    /// The submission that is in flight, again, for a retry after a network
    /// failure. If the first attempt did reach the server, the retry is
    /// refused as stale and the session resyncs, which is safe.
    #[must_use]
    pub fn in_flight_submission(&self) -> Option<Submission> {
        (self.in_flight > 0).then(|| Submission {
            base_seq: self.confirmed_seq,
            ops: self
                .pending
                .iter()
                .take(self.in_flight)
                .flat_map(|b| b.ops.iter().cloned())
                .collect(),
        })
    }

    /// The server accepted the in-flight submission; `last_seq` is its answer.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::NothingPending`], or [`WorkspaceError::Desync`] if
    /// `last_seq` is not the confirmed sequence plus the number of ops sent
    /// (the caller should resync).
    pub fn on_accepted(&mut self, last_seq: u64) -> Result<(), WorkspaceError> {
        if self.in_flight == 0 {
            return Err(WorkspaceError::NothingPending);
        }
        let sent: usize = self
            .pending
            .iter()
            .take(self.in_flight)
            .map(|b| b.ops.len())
            .sum();
        let expected = self.confirmed_seq + sent as u64;
        if last_seq != expected {
            return Err(WorkspaceError::Desync {
                expected,
                found: last_seq,
            });
        }
        self.pending.drain(..self.in_flight);
        self.in_flight = 0;
        self.confirmed_seq = last_seq;
        Ok(())
    }

    /// The server refused the submission (or it can no longer be trusted):
    /// undoes every pending edit, newest first, and clears undo history.
    /// Returns how many edits were rolled back. Time O(pending ops).
    pub fn on_rejected(&mut self) -> usize {
        let count = self.pending.len();
        while let Some(batch) = self.pending.pop_back() {
            self.undo_applied(batch.inverses);
        }
        self.in_flight = 0;
        self.cleared += self.undo.len() + self.redo.len();
        self.undo.clear();
        self.redo.clear();
        self.suggestions = None;
        count
    }

    fn undo_applied(&mut self, inverses: Vec<Op>) {
        for inverse in inverses.into_iter().rev() {
            // An inverse the model refuses means the local copy is damaged;
            // the caller resyncs from the server, which is the cure either
            // way, so the error is not propagated from here.
            if self.project.apply_cached(inverse, &mut self.cache).is_err() {
                self.cache = self.project.eval_cache();
            }
        }
    }

    /// Replaces everything with a fresh copy from the server.
    pub fn reset(&mut self, project: Project, seq: u64) {
        *self = Workspace::new(project, seq);
    }
}

fn push_bounded(stack: &mut Vec<Step>, entry: Step) {
    if stack.len() >= MAX_UNDO_STEPS {
        stack.remove(0);
    }
    stack.push(entry);
}

#[cfg(test)]
mod tests {
    use aegis_model::{AssetId, Provenance};
    use proptest::prelude::*;

    use super::*;

    fn methodology() -> Arc<Methodology> {
        Arc::new(Methodology::iso21434_r155().expect("builtin"))
    }

    /// Sixteen fixed asset IDs, so tests can name assets by number.
    fn id(n: u128) -> AssetId {
        static IDS: std::sync::OnceLock<Vec<AssetId>> = std::sync::OnceLock::new();
        let ids = IDS.get_or_init(|| (0..16).map(|_| AssetId::new_v7()).collect());
        ids[usize::try_from(n).expect("small") % ids.len()]
    }

    fn add(n: u128, name: &str) -> Op {
        Op::AddAsset {
            id: id(n),
            name: name.into(),
            properties: methodology().all_properties(),
            provenance: Provenance::default(),
        }
    }

    fn rename(n: u128, name: &str) -> Op {
        Op::RenameAsset {
            id: id(n),
            name: name.into(),
            provenance: Provenance::default(),
        }
    }

    fn name_of(p: &Project, n: u128) -> Option<String> {
        let idx = p.asset_idx(&id(n))?;
        p.asset(idx).map(|a| a.name().to_owned())
    }

    #[test]
    fn an_edit_is_visible_at_once_and_confirmed_later() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "ECU")], Origin::Edit)
            .expect("applies");
        assert_eq!(
            name_of(ws.project(), 1).as_deref(),
            Some("ECU"),
            "visible before any server answer"
        );
        assert_eq!((ws.confirmed_seq(), ws.pending_batches()), (0, 1));

        let sub = ws.next_submission().expect("something to send");
        assert_eq!(sub.base_seq, 0);
        assert_eq!(sub.ops, vec![add(1, "ECU")]);
        assert!(ws.is_submitting());
        assert!(
            ws.next_submission().is_none(),
            "one call in flight at a time"
        );

        ws.on_accepted(1).expect("accepted");
        assert_eq!((ws.confirmed_seq(), ws.pending_batches()), (1, 0));
        assert!(!ws.is_submitting());
    }

    #[test]
    fn edits_made_while_one_is_in_flight_go_out_together_next() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "A")], Origin::Edit).expect("applies");
        let first = ws.next_submission().expect("first");
        ws.apply(vec![add(2, "B")], Origin::Edit).expect("applies");
        ws.apply(vec![rename(2, "B2"), rename(1, "A2")], Origin::Edit)
            .expect("applies");
        assert_eq!(first.ops.len(), 1);
        ws.on_accepted(1).expect("accepted");
        let second = ws.next_submission().expect("the rest");
        assert_eq!(second.base_seq, 1);
        assert_eq!(second.ops.len(), 3, "two batches, three ops, one call");
        ws.on_accepted(4).expect("accepted");
        assert_eq!(ws.confirmed_seq(), 4);
    }

    #[test]
    fn a_batch_is_all_or_nothing_locally() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "A")], Origin::Edit).expect("applies");
        let before = name_of(ws.project(), 1);
        // Second op names an asset that does not exist.
        let err = ws.apply(vec![rename(1, "A2"), rename(9, "ghost")], Origin::Edit);
        assert!(matches!(err, Err(WorkspaceError::Model(_))));
        assert_eq!(name_of(ws.project(), 1), before, "the first op was undone");
        assert_eq!(ws.pending_batches(), 1, "nothing was queued");
    }

    #[test]
    fn a_rejection_rolls_back_every_pending_edit_and_clears_history() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "A")], Origin::Edit).expect("applies");
        ws.next_submission().expect("sent");
        ws.on_accepted(1).expect("accepted");
        ws.apply(vec![rename(1, "A2")], Origin::Edit)
            .expect("applies");
        ws.next_submission().expect("sent");
        ws.apply(vec![add(2, "B")], Origin::Edit).expect("applies");
        assert_eq!(ws.on_rejected(), 2);
        assert_eq!(
            name_of(ws.project(), 1).as_deref(),
            Some("A"),
            "back to the confirmed state"
        );
        assert_eq!(name_of(ws.project(), 2), None);
        assert!(!ws.can_undo() && !ws.can_redo());
        assert_eq!(
            (ws.pending_batches(), ws.is_submitting(), ws.confirmed_seq()),
            (0, false, 1)
        );
        // And editing continues normally.
        ws.apply(vec![rename(1, "A3")], Origin::Edit)
            .expect("applies");
    }

    #[test]
    fn undo_and_redo_are_edits_that_reverse_each_other() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "A")], Origin::Edit).expect("applies");
        ws.apply(vec![rename(1, "A2")], Origin::Edit)
            .expect("applies");
        assert!(ws.can_undo() && !ws.can_redo());

        ws.undo().expect("undo");
        assert_eq!(name_of(ws.project(), 1).as_deref(), Some("A"));
        assert!(ws.can_redo());
        ws.redo().expect("redo");
        assert_eq!(name_of(ws.project(), 1).as_deref(), Some("A2"));
        ws.undo().expect("undo");
        ws.undo().expect("undo the add");
        assert_eq!(name_of(ws.project(), 1), None);
        assert_eq!(ws.undo(), Err(WorkspaceError::NothingToUndo));

        // They all went to the server as ordinary edits.
        let sub = ws.next_submission().expect("queue");
        assert_eq!(sub.ops.len(), 2 + 4);
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "A")], Origin::Edit).expect("applies");
        ws.undo().expect("undo");
        assert!(ws.can_redo());
        ws.apply(vec![add(2, "B")], Origin::Edit).expect("applies");
        assert!(!ws.can_redo());
        assert_eq!(ws.redo(), Err(WorkspaceError::NothingToRedo));
    }

    #[test]
    fn a_wrong_server_answer_is_a_desync() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "A")], Origin::Edit).expect("applies");
        ws.next_submission().expect("sent");
        assert_eq!(
            ws.on_accepted(7),
            Err(WorkspaceError::Desync {
                expected: 1,
                found: 7
            })
        );
        assert_eq!(
            ws.on_accepted(1),
            Ok(()),
            "the state was not changed by the bad answer"
        );
        assert_eq!(ws.on_accepted(2), Err(WorkspaceError::NothingPending));
    }

    #[test]
    fn the_pending_queue_is_bounded() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "A")], Origin::Edit).expect("applies");
        for i in 0..MAX_PENDING_BATCHES - 1 {
            ws.apply(vec![rename(1, &format!("n{i}"))], Origin::Edit)
                .expect("applies");
        }
        assert_eq!(
            ws.apply(vec![rename(1, "one too many")], Origin::Edit),
            Err(WorkspaceError::TooManyPending)
        );
    }

    #[test]
    fn queued_edits_are_sent_in_submissions_the_server_accepts() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "a")], Origin::Edit).expect("add");
        // Three edits of 400 renames each: 1 + 400 fit, then 400 + 400, ...
        for round in 0..3 {
            let ops = (0..400)
                .map(|i| rename(1, &format!("n{round}-{i}")))
                .collect();
            ws.apply(ops, Origin::Edit).expect("rename batch");
        }
        let mut seq = 0;
        let mut sizes = Vec::new();
        while let Some(sub) = ws.next_submission() {
            assert!(sub.ops.len() <= MAX_SUBMIT_OPS);
            sizes.push(sub.ops.len());
            seq += sub.ops.len() as u64;
            ws.on_accepted(seq).expect("accepted");
        }
        assert_eq!(sizes, [801, 400]);
        assert_eq!(ws.pending_batches(), 0);
        let too_big = (0..=MAX_SUBMIT_OPS).map(|_| rename(1, "x")).collect();
        assert!(matches!(
            ws.apply(too_big, Origin::Edit),
            Err(WorkspaceError::TooLarge { .. })
        ));
    }

    #[test]
    fn the_undo_stack_is_bounded() {
        let mut ws = Workspace::empty(methodology());
        ws.apply(vec![add(1, "A")], Origin::Edit).expect("applies");
        for i in 0..MAX_UNDO_STEPS + 50 {
            ws.apply(vec![rename(1, &format!("n{i}"))], Origin::Edit)
                .expect("applies");
            if i % 500 == 499 {
                // Let the server catch up so the pending queue stays small.
                let _ = ws.next_submission();
                let n = ws.pending.iter().map(|b| b.ops.len()).sum::<usize>() as u64;
                ws.on_accepted(ws.confirmed_seq() + n).expect("accepted");
            }
        }
        assert!(ws.undo.len() <= MAX_UNDO_STEPS);
    }

    // -----------------------------------------------------------------
    // Model-based test: the workspace against a "server" that replays the
    // accepted ops. Whatever the interleaving of edits, undo, redo,
    // acceptances, and rejections, the workspace must agree with the server
    // whenever nothing is pending, and its confirmed sequence must always
    // equal the server's.
    // -----------------------------------------------------------------

    #[derive(Clone, Debug)]
    enum Action {
        Rename(u8, u8),
        Undo,
        Redo,
        Send,
        Accept,
        Reject,
    }

    fn action() -> impl Strategy<Value = Action> {
        prop_oneof![
            4 => (0u8..3, any::<u8>()).prop_map(|(a, n)| Action::Rename(a, n)),
            2 => Just(Action::Undo),
            1 => Just(Action::Redo),
            2 => Just(Action::Send),
            2 => Just(Action::Accept),
            1 => Just(Action::Reject),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn the_workspace_always_agrees_with_the_server(actions in prop::collection::vec(action(), 1..80)) {
            let m = methodology();
            let seed: Vec<Op> = (0..3u128).map(|i| add(i, &format!("asset {i}"))).collect();
            let mut server = Project::new(Arc::clone(&m));
            let mut ws = Workspace::empty(Arc::clone(&m));
            for op in &seed {
                server.apply(op.clone()).expect("seed");
            }
            ws.reset(server.clone(), seed.len() as u64);
            let mut server_seq = seed.len() as u64;
            let mut in_flight: Option<Submission> = None;

            for a in actions {
                match a {
                    Action::Rename(asset, n) => {
                        let _ = ws.apply(vec![rename(u128::from(asset), &format!("v{n}"))], Origin::Edit);
                    }
                    Action::Undo => { let _ = ws.undo(); }
                    Action::Redo => { let _ = ws.redo(); }
                    Action::Send => {
                        if let Some(sub) = ws.next_submission() {
                            prop_assert_eq!(sub.base_seq, server_seq);
                            in_flight = Some(sub);
                        }
                    }
                    Action::Accept => {
                        if let Some(sub) = in_flight.take() {
                            for op in &sub.ops {
                                server.apply(op.clone()).expect("a valid edit");
                                server_seq += 1;
                            }
                            prop_assert_eq!(ws.on_accepted(server_seq), Ok(()));
                        }
                    }
                    Action::Reject => {
                        if in_flight.take().is_some() {
                            ws.on_rejected();
                        }
                    }
                }
                prop_assert_eq!(ws.confirmed_seq(), server_seq);
                if ws.pending_batches() == 0 {
                    prop_assert!(ws.project() == &server, "workspace and server differ with nothing pending");
                }
            }
            // Flush: send and accept whatever is left; then they must be equal.
            if let Some(sub) = in_flight.take() {
                for op in &sub.ops { server.apply(op.clone()).expect("valid"); server_seq += 1; }
                prop_assert_eq!(ws.on_accepted(server_seq), Ok(()));
            }
            while let Some(sub) = ws.next_submission() {
                for op in &sub.ops { server.apply(op.clone()).expect("valid"); server_seq += 1; }
                prop_assert_eq!(ws.on_accepted(server_seq), Ok(()));
            }
            prop_assert!(ws.project() == &server, "workspace and server differ after the flush");
        }

        /// Undoing every edit of a session restores the starting project.
        #[test]
        fn undoing_everything_restores_the_start(names in prop::collection::vec(any::<u8>(), 1..30)) {
            let m = methodology();
            let mut ws = Workspace::empty(Arc::clone(&m));
            ws.apply(vec![add(1, "start")], Origin::Edit).expect("applies");
            ws.next_submission();
            ws.on_accepted(1).expect("accepted");
            let start = ws.project().clone();
            for n in &names {
                ws.apply(vec![rename(1, &format!("v{n}"))], Origin::Edit).expect("applies");
            }
            for _ in &names {
                ws.undo().expect("undo");
            }
            prop_assert!(ws.project() == &start, "undo did not restore the start");
            for _ in &names {
                ws.redo().expect("redo");
            }
            prop_assert_eq!(name_of(ws.project(), 1), Some(format!("v{}", names[names.len() - 1])));
        }
    }
}
