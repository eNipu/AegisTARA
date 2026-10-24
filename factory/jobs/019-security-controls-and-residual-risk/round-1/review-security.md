VERDICT: PASS

Scope: `019-security-controls-and-residual-risk` (commit 21b5f22), checked against `spec.md`, `build.md`, Q2–Q4, R3/R4/R6, D5, D6, and D7.

No blocking findings.

Checked and OK:
- `src/store/wire.rs` & `src/store/mod.rs`: `StepMitigations` caps outer map entries at `MAX_ATTACK_TREE_NODES` (32) and inner `NodeMitigations` entries at `MAX_STEP_MITIGATIONS` (16) during deserialization before allocation exceeds the cap, rejects empty maps (enforcing canonical omission), rejects duplicate `NodeId`/`ControlId` keys and non-canonical UUID strings, and validates `FeasibilityLevel` keys immediately (R3, R6, Q4). Verified by `caps_step_mitigations` in `tests/store_limits.rs`.
- `src/project.rs`: `Project::validate` enforces `MAX_NAME_CHARS` (256), non-blank control names, `check_provenance`, `slot_limit` capacity on `controls`, `MAX_STEP_MITIGATIONS` (16) per step, and referential integrity across threats, trees, and controls before `Project::commit` mutates state (R3, R4, Q4).
- `src/project.rs`: Referential integrity checks prevent silent data loss on undo: `check_remove_threat` rejects threats with a risk treatment (`ThreatHasRiskTreatment`), `check_remove_tree` rejects trees with step mitigations (`AttackTreeHasMitigations`), and `check_remove_control` rejects controls referenced by any step mitigation (`SecurityControlInUse`) via `RefCounts::controls`.
- `src/project.rs`: `residual_step_feasibility` clamps the mitigated feasibility via `.min(m.feasibility)` in the `FeasibilitySemiLattice` order (`Ord::min` is harder / lower index), so a misconfigured security control can never amplify a step's attack feasibility above its inherent rating.
- `src/error.rs`: All new `ModelError` variants (`DuplicateSecurityControl`, `UnknownSecurityControl`, `AttackTreeHasMitigations`, `SecurityControlInUse`, `ThreatHasRiskTreatment`, `TooManyStepMitigations`) contain only UUIDv7 IDs and `usize` caps, never customer names or actor strings (R3, R6).
- Q2: `#![forbid(unsafe_code)]` remains intact; no `unsafe` added.
- Q3: No `unwrap()`, `expect()`, `panic!`, `todo!`, or `unimplemented!` in library code outside `#[cfg(test)]`.
