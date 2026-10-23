# Build Log: `022-project-diff`

## Summary of Changes

1. **`crates/aegis-model/src/error.rs`**:
   - Added `ModelError::MethodologyMismatch { expected, expected_version, found, found_version }`.
2. **`crates/aegis-model/src/diff.rs` & `crates/aegis-model/src/lib.rs`**:
   - Implemented and re-exported `EntityDiff<Id>`, `RiskSnapshot`, `RiskChange`, `RiskDelta`, `ThreatRiskDelta`, and `ProjectDiff` with rustdoc, complexity bounds, runnable doctests, and unit tests.
3. **`crates/aegis-model/src/project.rs`**:
   - Implemented `Project::diff(&self, other: &Self) -> Result<ProjectDiff, ModelError>`, `diff_arena`, `diff_risk_deltas`, `is_threat_affected`, and `threat_risk_snapshots`.
4. **Tests & Documentation**:
   - Added `project_diff_tracks_structural_changes_and_risk_deltas` in `crates/aegis-model/tests/project.rs`.
   - Added `diff_soundness_first_methodology` and `diff_soundness_second_methodology` in `crates/aegis-model/tests/ops_properties.rs`.
   - Added `docs/adr/0020-project-diff.md`.

## Verification

- `./scripts/ci.sh` passed all gates.
- `cargo test --manifest-path crates/aegis-model/Cargo.toml --bench store_memory` passed all memory gates.
