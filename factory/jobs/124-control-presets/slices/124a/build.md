Commit: b0d3953 Carry setting presets in the methodology definition (124a)
Files: crates/aegis-model/src/methodology/mod.rs, src/methodology/pack.rs, src/error.rs, src/lib.rs, tests/control_kinds.rs (new), tests/builtin_pack.rs
Done when:
1. `cargo test --locked -p aegis-model --test control_kinds` - 11 passed
2. Same command - one test per error (7), each asserting the exact error value
3. `cargo test --locked -p aegis-model --test builtin_pack` - 14 passed
4. `cargo test --locked -p aegis-model --doc` - 700 passed
5. `git diff --numstat master -- crates/aegis-model/src` - 337 added lines (cap 400)
Also: `cargo fmt --all`, `scripts/ci.sh crates/aegis-model` all passed.
Notes:
- Unknown attack type, level and interface kind keys in errors are cut with `error_key_text`, like `UnknownBandFeasibilityLevel`.
- Rule keys in the test for 17 rules use 7 extra attack types (10 + 7 attack types, 17 rules from the first 17).
- Also added a `PackTranslationDef` literal field in tests/builtin_pack.rs (two literals needed it).
NEXT: reviewer
