VERDICT: PASS
## For the owner
What changed: Added `SetAssumptionExcludes`, `LinkClaimAssumption`, and `UnlinkClaimAssumption` ops with `CybersecurityClaim::assumptions` and op-log codecs.
Wired all L30 removal guards across assumptions, claims, attack trees, and excluded leaf steps, plus diff, equality, and invariant checks.
Added model isolation, combined-cap, L30 full-project undo, and `undo_every_op` client round-trip tests.
Look at:
- `crates/aegis-model/src/project.rs:11931` — `commit_set_assumption_excludes` sorts and deduplicates public IDs before resolving `TreeIdx` so `excludes_trees` ordering is slot-independent across projects.
- `crates/aegis-model/src/project.rs:8621` — `check_remove_tree_node` blocks removing an excluded step with `NodeIsRated`, paired with `check_remove_tree`, `check_remove_assumption`, and `check_remove_claim` guards.
- `crates/aegis-model/src/store/op_log.rs:2477` — `op_from_wire` enforces the combined `trees + steps <= MAX_ASSUMPTION_EXCLUDES` cap identically to `within_caps` (`StoreError::LimitExceeded(Limit::ASSUMPTION_EXCLUDES)`).
Risk: Combined exclusion cap (256 across `trees` and `steps`), L15 blank-actor rejection on all three ops, L30 undo equality, and `StepRef` stability after `NodeIdx` compaction were checked in `crates/aegis-model/tests/assumptions.rs` (12 passed) and `crates/aegis-client/tests/undo_every_op.rs` (4 passed); `Limit::CLAIM_ASSUMPTIONS` carries `#[allow(dead_code)]` until snapshot slice `136c`.
View: git -C factory/worktrees/136-assumptions diff dc306bd132e5af64956a1bd481ccf36db03deb1f..HEAD
## Findings
none
## Lesson
none
NEXT: human
