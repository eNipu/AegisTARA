VERDICT: PASS
## For the owner
What changed: Added the `Assumption` entity (`APL-04`) with `AssumptionId`/`AssumptionIdx`, visual prefix `AS`, `StepRef`, `ElementRef::Assumption`, and `EntityKind::Assumption`.
Implemented `AddAssumption`, `RenameAssumption`, `SetAssumptionHolds`, and `RemoveAssumption` with validation guards, exact undo/redo inverses, `ProjectDiff::assumptions`, and op-log wire codecs.
Added minimal `ElementRef::Assumption` handling in `aegis-client`, unit/integration coverage in `assumptions.rs`, and full undo/redo variant coverage in `undo_every_op.rs`.
Look at:
- `crates/aegis-model/src/project.rs:9835` — `check_add_assumption`..`check_remove_assumption` validation guards (L09/L15) and `same_assumption` at `crates/aegis-model/src/project.rs:12735` (L29 public-ID comparison).
- `crates/aegis-model/src/project.rs:6514` — `merge_condition_link_changes` replacing the `_ => {}` wildcard with exhaustive `ElementRef` arms so assumption condition links merge into `diff.assumptions`.
- `crates/aegis-model/src/store/op_log.rs:783` — `within_caps` name byte bounds matching `OpWire`/`OpView` codecs for assumption ops.
Risk: Data loss on undo, stale diff/eval state, or op-log cap mismatch on new ops/targets; checked by L30 full-`Project` undo equality, `ProjectDiff::assumptions` lifecycle/description/condition-link tests, L09 single-guard isolation, L15 blank-actor checks on all three provenance-carrying ops, `OpCodec`/`OpLogWriter`/`OpLogReader` round-trips, and `undo_every_op` variant exhaustiveness. Snapshot serialization (`set_assumption_loaded_visual`, `#[allow(dead_code)]`) is intentionally deferred to slice `136c`.
View: git -C factory/worktrees/136-assumptions diff 352f90be359287b7bc4e88d6fd6811cf8a331283..HEAD
## Findings
none
## Lesson
none
NEXT: human
