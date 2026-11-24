Commit: a360ecbae9443eda40a3bff60d629d3ad9d7ae5b Wire assumption exclusions, claim links, and L30 removal guards (136b)
Files:
- crates/aegis-model/src/control.rs
- crates/aegis-model/src/error.rs
- crates/aegis-model/src/op.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/store/mod.rs
- crates/aegis-model/src/store/op_log.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/tests/assumptions.rs
- crates/aegis-client/tests/undo_every_op.rs
Done when:
1. `cargo test --locked -p aegis-model --test assumptions` -> exit 0 (12 passed, 0 failed; covers L15 blank-actor rejections for all 3 ops, L09 single-guard isolation including unrated step `NodeIsRated` and `130 + 127 = 257` combined overflow in model, `within_caps`/`OpLogWriter`, `OpCodec`, and `OpLogReader`, `StepRef` stability after sibling step removal, op-log roundtrips, `MAX_CLAIM_ASSUMPTIONS` 64/65 cap, `Project::diff`, and L30 full-`Project` undo equality).
2. `cargo test --locked -p aegis-client --test undo_every_op` -> exit 0 (4 passed, 0 failed; `every_op_kind_is_undone_and_redone_exactly` passes with `assert_eq!(seen, declared_variants())`).
Checks:
- `cargo fmt --all` -> exit 0.
- `scripts/ci.sh crates/aegis-model crates/aegis-client` -> exit 0 (`aegis-model` fmt/clippy/test and `aegis-client` fmt/clippy/test all passed).
Notes:
- `StoreError` defines `StoreError::LimitExceeded(Limit)` (no `StoreError::Limit` variant exists), so `op_from_wire` returns `Err(StoreError::LimitExceeded(Limit::ASSUMPTION_EXCLUDES))` when `trees.len() + steps.len() > MAX_ASSUMPTION_EXCLUDES`.
- `Limit::CLAIM_ASSUMPTIONS` carries `#[allow(dead_code)]` in `store/mod.rs` until `ClaimWire` uses it in the snapshot slice (`136c`), matching `136a`'s convention in `project.rs`.
- Source line delta in `crates/aegis-model/src/` (including doc comments and the `op_log.rs` unit test addition): 503 insertions, 8 deletions (under `max_lines: 560`).
NEXT: reviewer
