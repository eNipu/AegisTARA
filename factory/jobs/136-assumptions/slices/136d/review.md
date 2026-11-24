VERDICT: PASS
## For the owner
What changed: Added `LintCode::AssumptionWithoutClaim`, suppressed `CybersecurityClaimUnlinked` when a claim links an assumption, and wired `Selection::Assumption`, `ElementKind::Assumption`, and all three assumption relations (`claim-assumptions`, `assumption-conditions`, `assumption-excludes`) into the client.
Implemented the six assumption `Edit` variants with full L30 cleanup before removals, `tree_diff.rs` outline step-move and branch exclusion preservation, `undo_label.rs` descriptions, and the hidden `Claims` `"assumptions"` column.
Exposed assumption fields/relations and reverse `"Excluded by assumptions"` causes in `InspectorView` with trilingual Fluent keys (`en`, `ja`, `pseudo`) and temporary `Page::Configuration` selection fallback until `136e`.
Look at: `crates/aegis-client/src/commands.rs:1213-1320` — `clear_descriptions_before_removal` maintains a live per-assumption exclusion map so multi-node and whole-tree removals emit deduplicated `SetAssumptionExcludes` ops before `RemoveTreeNode`/`RemoveAttackTree`.
`crates/aegis-client/src/tree_diff.rs:602-648` and `696-718` — `sync_excludes` preserves exclusions across moved/renamed outline steps, remaps branched steps onto the new child step, and drops deleted or gate-converted steps.
`crates/aegis-model/src/project.rs:6181-6207` — `lint_claims` suppresses `CybersecurityClaimUnlinked` when `!claim.assumptions().is_empty()` and `lint_assumptions` flags unlinked assumptions.
Risk: If `Edit::Batch` ever combined two `RemoveTreeNode` edits on steps excluded by the same assumption, the outer pass would see the second sub-edit's stale pre-computed `SetAssumptionExcludes`; UI never batches node removals, and single-edit multi-node removals (`RemoveTree`, `ReplaceTree`) plus `BranchStepToGate` are verified by full `Project` equality round-trip tests in `crates/aegis-client/tests/undo_every_op.rs:990-1196`.
View: git -C factory/worktrees/136-assumptions diff 9f93d93d645f64d5124c2f31b7be09376437a306..HEAD
## Findings
none
## Lesson
none
NEXT: human
