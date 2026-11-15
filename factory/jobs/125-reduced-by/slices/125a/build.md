Commit: 6fcc1fe Find the deciding step of a tree and what caps it (125a)
Files: crates/aegis-model/src/control.rs, crates/aegis-model/src/project.rs, crates/aegis-model/src/lib.rs, crates/aegis-model/tests/reduced_by.rs
Done when:
1. `cargo test --locked -p aegis-model --doc deciding_step`: 1 passed. Type and method exported from crate root. (`cargo doc` was not run separately; `scripts/ci.sh` passed.)
2. `cargo test --locked -p aegis-model --test reduced_by`: 6 passed.
3. `scripts/ci.sh crates/aegis-model`: fmt, clippy, test all ok.
Notes: Non-test changes are about 165 lines, mostly docs and the doc test. lib.rs and the project.rs import list were reflowed by rustfmt after adding `DecidingStep`. No behaviour, op or snapshot change.
NEXT: reviewer
