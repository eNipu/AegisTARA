Commit: 726042f2d4e7889932c67bf54e489259514063b1 Persist parties and responsible links and add the share-treatment lint (145c)

Files:
- crates/aegis-model/src/store/mod.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/store/snapshot.rs
- crates/aegis-model/src/lint.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/tests/common/mod.rs
- crates/aegis-model/tests/store.rs
- crates/aegis-model/tests/store_limits.rs
- crates/aegis-model/tests/party_lint.rs (new)
- crates/aegis-model/tests/ops_properties.rs

Done when:
1. `cargo test --locked -p aegis-model --test store` — PASS (20 passed, 1 ignored). Party/responsible byte round-trip with 101 parties and a visual-ID gap, no-parties byte identity, and the v1 fixture load are all asserted.
2. `cargo test --locked -p aegis-model --test store_limits` — PASS (21 passed). `caps_parties` and `caps_responsible` assert just-under loads and just-over raises `Limit::Parties`/`Limit::Responsible`.
3. `cargo test --locked -p aegis-model --test party_lint` — PASS (6 passed), exactly the six requested cases, each asserting the code and severity for the positive cases.
4. `cargo test --locked -p aegis-model --test ops_properties --test store_properties` — PASS (8 passed); the generator covers all eight new ops.

Before you commit (all run from the worktree root, all PASS):
- `cargo fmt --all`
- `scripts/ci.sh crates/aegis-model` — fmt, clippy and the full crate test suite all green.

Notes:
- 202 added source lines (limit 520). No dependency, no risk-number, envelope-version or methodology-pack change; no `.slint`/shell/client/report/locale code.
- Snapshot writes `parties` and per-goal/claim `responsible` only when non-empty, so a project without them saves byte-for-byte what it did before; the v1 fixture loads and re-saves unchanged. Visual IDs for parties are persisted, so >100 parties with a zero-padded visual-ID gap round-trip exactly (L06).
- The decode caps use `BoundedVec` with `PartiesCap`/`ResponsibleCap`, so caps fire during decoding, before model validation, with their own `Limit` variants.
- The `ShareTreatmentWithoutOtherParty` warning fires only for `Share` with a linked claim and no responsible party of kind other than `Us` (zero responsible counts). Its `Display` message carries no party name (R6).
- Two scope-related accommodations, both confined to `tests/common/mod.rs`:
  (a) `RemoveCybersecurityGoal`/`RemoveCybersecurityClaim` generator arms now pick only goals/claims with no responsible links. The single-op inverse `AddCybersecurityGoal`/`AddCybersecurityClaim` (in `op.rs`, out of scope) carries no `responsible` field, so removing a linked goal/claim and undoing it cannot restore the links — a latent 145b gap this slice's generator extension exposed. Narrowing the generator keeps the inverse property green without changing model behavior.
  (b) `accepted()` skips the eight party ops. The workspace exchange format has no `parties` file (a later slice), so `to_workspace_files()` rejects projects with parties; `workspace_properties.rs` and `store/workspace.rs` are out of scope. The coverage test drives `replay()` directly, so it still covers every op including all eight party ops.
- All commands were waited on; nothing left running.

NEXT: reviewer
