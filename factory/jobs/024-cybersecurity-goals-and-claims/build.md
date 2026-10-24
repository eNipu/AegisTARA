# Build Report — 024-cybersecurity-goals-and-claims

## Summary

Implemented ISO/SAE 21434 §15.4 cybersecurity goals and cybersecurity claims as first-class arena-backed project entities with full operation lifecycle, referential integrity, lint diagnostics, project diffing, and canonical snapshot/NDJSON persistence.

## Files Changed

- `docs/adr/0022-cybersecurity-goals-and-claims.md` — Accepted ADR documenting arena-backed `CybersecurityGoal` and `CybersecurityClaim`, referential integrity rules, lint diagnostics, diffing, and canonical wire persistence.
- `crates/aegis-model/src/id.rs` — Added `GoalIdx` / `GoalId` and `ClaimIdx` / `ClaimId` via `entity_ids!`, plus unit tests in `entity_ids`.
- `crates/aegis-model/src/control.rs` — Added `MAX_GOAL_CONTROLS: usize = 16`, `CybersecurityGoal` (`name`, `controls: Vec<ControlIdx>`, `provenance`), and `CybersecurityClaim` (`name`, `provenance`) with rustdoc examples.
- `crates/aegis-model/src/threat.rs` — Added `goal: Option<GoalIdx>` and `claim: Option<ClaimIdx>` to `ThreatScenario`, plus `ThreatScenario::goal(&self)` and `ThreatScenario::claim(&self)` accessors.
- `crates/aegis-model/src/error.rs` — Added `EntityKind::CybersecurityGoal` and `EntityKind::CybersecurityClaim`, plus 12 `ModelError` variants (`DuplicateCybersecurityGoal`, `DuplicateCybersecurityClaim`, `UnknownCybersecurityGoal`, `UnknownCybersecurityClaim`, `SecurityControlInGoal`, `CybersecurityGoalInUse`, `CybersecurityClaimInUse`, `ThreatHasGoal`, `ThreatHasClaim`, `DuplicateGoalControl`, `GoalControlNotLinked`, `TooManyGoalControls`) with `Display` and unit test coverage.
- `crates/aegis-model/src/op.rs` — Added 10 `Op` variants (`AddCybersecurityGoal`, `RenameCybersecurityGoal`, `LinkGoalControl`, `UnlinkGoalControl`, `RemoveCybersecurityGoal`, `AddCybersecurityClaim`, `RenameCybersecurityClaim`, `RemoveCybersecurityClaim`, `SetThreatGoal`, `SetThreatClaim`) and updated `Op::provenance`.
- `crates/aegis-model/src/lint.rs` — Added 7 `LintSeverity::Warning` variants to `LintCode` (`ReduceTreatmentWithoutGoal`, `RetainOrShareTreatmentWithoutClaim`, `GoalOnNonReduceThreat`, `ClaimOnNonRetainOrShareThreat`, `CybersecurityGoalWithoutControls`, `CybersecurityGoalUnlinked`, `CybersecurityClaimUnlinked`) with `Display` and unit test coverage.
- `crates/aegis-model/src/diff.rs` — Added `cybersecurity_goals: EntityDiff<GoalId>` and `cybersecurity_claims: EntityDiff<ClaimId>` to `ProjectDiff`, accessors, and `is_empty()` check.
- `crates/aegis-model/src/project.rs` — Added `goals` and `claims` arenas, `RefCounts` fields (`control_goals`, `goals`, `claims`), accessors, `validate`/`commit` for all 10 `Op` variants, removal guards (`SecurityControlInGoal`, `ThreatHasGoal`, `ThreatHasClaim`, `CybersecurityGoalInUse`, `CybersecurityClaimInUse`), `lint_threats`/`lint_goals`/`lint_claims`, `Project::diff` support, `PartialEq`, and `check_invariants`.
- `crates/aegis-model/src/lib.rs` — Re-exported `CybersecurityClaim`, `CybersecurityGoal`, `MAX_GOAL_CONTROLS`, `ClaimId`, `ClaimIdx`, `GoalId`, `GoalIdx`.
- `crates/aegis-model/src/store/mod.rs` — Added `Limit::GoalControls { max: usize }` (`Limit::GOAL_CONTROLS`) and `Display`.
- `crates/aegis-model/src/store/wire.rs` — Added `GoalControlsCap`, `GoalWire`, `ClaimWire`, `cybersecurity_goals` and `cybersecurity_claims` on `SnapshotWire`, `goal` and `claim` on `ThreatWire`, and all 10 `OpWire` variants.
- `crates/aegis-model/src/store/snapshot.rs` — Added `Goals`, `Claims`, `GoalView`, `ClaimView`, `LinkedControls`, `goal` and `claim` on `ThreatView`, and updated `Project::save` and `Project::load`.
- `crates/aegis-model/src/store/op_log.rs` — Added 10 `OpView` variants, `within_caps` checks, `op_view`, and `op_from_wire`.
- `crates/aegis-model/tests/common/mod.rs` — Added `goal_id`, `claim_id`, `Gen::goal`, `Gen::claim`, expanded `seed()` to cover all 37 `Op` variants, updated `describe` and `second_every_op`.
- `crates/aegis-model/tests/ops_properties.rs` — Added `cybersecurity_goals` and `cybersecurity_claims` diff symmetry assertions and updated `ops_generator_covers_every_op_and_most_errors` to verify all 37 `Op` variants.
- `crates/aegis-model/tests/project.rs` — Added `cybersecurity_goals_claims_lifecycle_lint_and_diff` and updated `lint_all_rules_and_resolutions` and `lint_never_contains_names_or_actors` (all 22 `LintCode` variants).
- `crates/aegis-model/tests/store.rs` and `crates/aegis-model/tests/store_limits.rs` — Added `caps_goal_controls`, max-sized goal/claim op tests, and `UnknownCybersecurityGoal`/`UnknownCybersecurityClaim` snapshot tests.

## Verification

- `./scripts/ci.sh` — `ok aegis-model fmt`, `ok aegis-model clippy`, `ok aegis-model test` (all unit, integration, property, and doc tests pass).
- `./scripts/deny.sh` — `ok aegis-model`, `ok aegis-model/fuzz`.
