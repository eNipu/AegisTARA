# 020 — Completeness and consistency linter in `aegis-model`

## Goal

Add a deterministic completeness and consistency linter (`Project::lint`) that inspects a `Project` and returns canonical, machine-readable `LintFinding`s (`LintSeverity::Warning` and `LintSeverity::Error`) without ever exposing entity names, labels, or actor text (rule R6).

## Scope

1. **Lint types (`crates/aegis-model/src/lint.rs`, `lib.rs`)**:
   - `LintSeverity` (`Warning`, `Error`).
   - `LintCode` with 13 completeness and consistency variants covering assets, asset security properties, damage scenarios, threat scenarios, attack trees, and security controls.
   - `LintFinding` wrapping `LintCode` with severity helpers and canonical ordering.
2. **Project linter (`crates/aegis-model/src/project.rs`)**:
   - `Project::lint(&self) -> Vec<LintFinding>` leveraging `self.refs` (`RefCounts`) in linear time and sorting findings in canonical `LintCode` order (rule P4).
3. **Tests and ADR**:
   - Unit tests in `src/lint.rs`, integration tests in `tests/project.rs` (all 13 rules + resolutions, R6 sentinel check, history independence), property tests in `tests/ops_properties.rs` (evaluation soundness when zero `Error` findings exist), and `docs/adr/0018-project-lint.md`.
