VERDICT: PASS
## For the owner
What changed: Added snapshot wire types, bounded deserialization, canonical serialization, and `systems/main/assumptions.json` workspace placement for assumptions and claim-assumption links. Integrated `Reason::AssumptionHolds(AssumptionId)` into `Project::applicability()` so holding assumptions exclude trees and steps (unless suppressed by an off condition on the assumption itself), updating `ProjectEvalCache` incrementally. Extended `assumptions.rs`, `workspace.rs`, and `common/mod.rs` tests for feasibility/risk toggles, own-condition suppression, proptest cache soundness, round-trips, and L09 decode limits.
Look at:
- `crates/aegis-model/src/applicability.rs:443-511` — fast path requires no holding assumptions, skips assumptions excluded by an off condition, merges whole-tree and step assumptions with `propagate_owners` and `tree_exclusions`.
- `crates/aegis-model/src/store/snapshot.rs:1610-1653` — combined `excludes_trees + excludes_steps <= MAX_ASSUMPTION_EXCLUDES` guard and load ordering (`assumptions`, then `pending_claim_assumptions`, before `condition_links` and `descriptions`).
- `crates/aegis-model/src/store/wire.rs:1591-1607` — `AssumptionWire` with `#[serde(deny_unknown_fields)]`, `#[serde(default)] holds: bool`, and `BoundedVec` caps (`AssumptionsCap`, `AssumptionExcludesCap`, `ClaimAssumptionsCap`).
Risk: none — `Project::may_change_applicability` and `ProjectEvalCache::apply_applicability_change` in `project.rs` already tracked `SetAssumptionHolds`, `SetAssumptionExcludes`, `touched_trees()`, and step-level exclusions; verified by proptests (`shuffled_assumption_edits_match_fresh_eval` and `assumption_and_condition_switch_order_does_not_matter`), exact feasibility/risk assertions, byte-for-byte round-trips, and L09 boundary tests (`130 + 126 = 256` ok, `130 + 127 = 257` rejected).
View: git -C factory/worktrees/136-assumptions diff a360ecbae9443eda40a3bff60d629d3ad9d7ae5b..HEAD
## Findings
## Lesson
none
NEXT: human
