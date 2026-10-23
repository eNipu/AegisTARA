# Build Log — 020-project-lint

## Summary of Changes

1. **Lint Module (`crates/aegis-model/src/lint.rs`, `crates/aegis-model/src/lib.rs`)**:
   - Added `LintSeverity`, `LintCode` (13 variants across `Warning` and `Error`), and `LintFinding`, re-exported from `aegis-model`.
   - Enforced `size_of::<LintFinding>() <= 40`, `Copy`, and zero user-supplied strings in `Display`/`Debug` (rule R6).
2. **`Project::lint` (`crates/aegis-model/src/project.rs`)**:
   - Implemented `Project::lint(&self) -> Vec<LintFinding>` using `self.refs` for $O(1)$ / $O(\log P)$ reference checks and canonical ID ordering (rule P4).
3. **Verification & Documentation**:
   - Added `lint_all_rules_and_resolutions`, `lint_never_contains_names_or_actors`, and `lint_order_is_independent_of_history` in `crates/aegis-model/tests/project.rs`.
   - Added `lint_soundness_first_methodology` and `lint_soundness_second_methodology` property tests in `crates/aegis-model/tests/ops_properties.rs`.
   - Added `docs/adr/0018-project-lint.md`.
   - Verified with `./scripts/ci.sh` (`fmt`, `clippy`, unit/integration/property/doc tests).
