Commit: 9d84944 Warn when a setting's rules differ from its preset (124e)
Files: crates/aegis-model/src/project.rs, crates/aegis-model/src/lint.rs, crates/aegis-client/src/pages.rs, crates/aegis-model/tests/control_presets.rs, crates/aegis-client/tests/pages.rs
Done when:
1. Warning exactly when rules differ: `cargo test --locked -p aegis-model --test control_presets` -> 9 passed
2. `control_kind_fits` cases: same command -> passed
3. Problems row: `cargo test --locked -p aegis-client --test pages` -> 22 passed
4. Existing lint tests: `--test project --test control_settings` -> 9 and 31 passed
5. Source lines changed: 106 (16 + 11 + 79) -> under 200
Also: `cargo fmt --all` and `scripts/ci.sh crates/aegis-model crates/aegis-client` all passed (includes doctests).
Notes: `SetControlKind` only sets the kind and does not copy rules, so a fresh kinded control with no rules differs (doctest shows this). Tests also cover removing the extra rule and the missing rule.
NEXT: reviewer
