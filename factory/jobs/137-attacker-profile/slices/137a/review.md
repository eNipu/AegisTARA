VERDICT: PASS
## For the owner
What changed:
Added `ScalabilityFilter` and fixed-width `AttackFilter` (`[u16; 16]` + scalability mode) with per-methodology factor/level validation.
Added `Op::SetAttackFilter` (exact old-filter/old-provenance inverse), `ProjectDiff::attack_filter_changed()` (included in `is_empty()`), and `Project` equality/invariants (no applicability exclusion yet).
Added capped JSON persistence for `"attack_filter"` across snapshots, op logs/`OpCodec`, and workspace `aegis.json`, plus unit and property-generator tests.
Look at:
- `crates/aegis-model/src/store/wire.rs:1112` — `LevelListSeed` and `ExcludedLevelsWire` enforce `MAX_LEVELS`/`MAX_FEASIBILITY_FACTORS` caps before duplicate checks and cap keys with `check_key_len`, even for unknown factors.
- `crates/aegis-model/src/store/wire.rs:3542` — `AttackFilterView::serialize` writes deterministic `KeyOrder` factor / sorted level keys and fails with `ser::Error` when `written_bits != total_bits` (foreign indices).
- `crates/aegis-model/src/project.rs:6898` — `SetAttackFilter` validation runs both `check_attack_filter` and `check_provenance`, `commit` swaps `(filter, provenance)` as exact inverse, and `PartialEq`/`diff` ignore provenance.
Risk: Untrusted JSON decode limits, duplicate/unknown vocab keys, foreign-index encoding, blank provenance, and diff/undo invariants could drift across wire formats; checked via `attacker_profile.rs` (5 targeted unit/L09/L15/round-trip tests) and property/store/workspace suites reported passing in `build.md` (note: `gate.md` was absent).
View: git -C factory/worktrees/137-attacker-profile diff c14e472263c9b82bac1e8e13c3faf6f82fb41853..HEAD
## Findings
none
## Lesson
none
NEXT: human
