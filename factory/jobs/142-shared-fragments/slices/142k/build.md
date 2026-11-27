Commit: 0bcc4e8b3a452a41146707ecc93235eff3832f20 Unshare fragment references by materializing structure and overlays as one undo batch (142k)
Files: crates/aegis-client/src/commands.rs, crates/aegis-client/src/fragments.rs, crates/aegis-client/src/undo_label.rs, crates/aegis-client/locales/en/main.ftl, crates/aegis-client/locales/ja/main.ftl, crates/aegis-client/locales/pseudo/main.ftl, crates/aegis-client/tests/shared_fragments.rs
Done when:
1. All specified regression/property/workflow cases pass — `cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded`: PASS (`remove_loaded`: 3 passed; `shared_fragments`: 17 passed; `undo_every_op`: 6 passed).
2. Scoped CI passes — `scripts/ci.sh crates/aegis-client`: PASS (`ok aegis-client fmt`, `ok aegis-client clippy`, `ok aegis-client test`).
Notes:
- Production line delta vs `e9f51b4b0148f0ec0052cc2a13be9f5ee51cdcf7`: 188 insertions / 8 deletions (196 changed lines across `commands.rs`, `fragments.rs`, `undo_label.rs`, and `locales/{en,ja,pseudo}/main.ftl`, within the 480 cap).
- `Edit::Unshare { tree, node }` keeps `unshare_step_ops` for linked shared steps and routes fragment references through `unshare_fragment_ops` using `CopyMode::Materialize` in one `Vec<Op>`, keeping `Edit::UnuseFragment` as a 1-op detach and leaving source fragments/shared steps untouched even when unsharing their last user.
- Materialization resolves nested fragments and shared-step raw fields (`feasibility`, `factors`, `via`, `attack_type`), distributes outer reference overlays, source-fragment tree conditions, and assumption exclusions onto copied leaves (merging equal-control ceilings with `.min()` and ORing review marks), clears and restores/distributes `SetAssumptionExcludes` around `SetTreeNodeKind` when converting a step into a gate, and preflights node-growth and merged `step_conditions` / `step_mitigations` / `assumption_excludes` caps before mutation.
- Added `unshare-fragment` Fluent labels in `en`, `ja`, and `pseudo`, plus `undo_label::describe` recognition for multi-op fragment unshare (`unshare fragment '<name>'`, `is_removal() == false`, `Selection::TreeNode(tree, node)`). Note: unsharing an unrated single-step fragment with zero local/source metadata emits the same single `Op::UnuseFragment` as `Edit::UnuseFragment`, so `describe` labels that 1-op case identically to `UnuseFragment`.
NEXT: reviewer

## Rework 1
Commit: 4b73f3764ca87c216443c05618f78fe397eae406 Isolate unrated unshare cases and assert merged overlays and exclusions (142k)
Files: crates/aegis-client/tests/shared_fragments.rs
Done when:
1. All specified regression/property/workflow cases pass — `cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded`: PASS (`remove_loaded`: 3 passed; `shared_fragments`: 17 passed; `undo_every_op`: 6 passed).
2. Scoped CI passes — `scripts/ci.sh crates/aegis-client`: PASS (`ok aegis-client fmt`, `ok aegis-client clippy`, `ok aegis-client test`).
Notes:
- Moved `ExUnrated` out of `frag_excl_id` into its own multi-leaf fragment `frag_unrated_multi_id` (`unrated_multi_host_id`) so `excl_host_id` evaluates to `(FoldOutcome::Value(low), FoldOutcome::Value(low))` before and after Unshare (proving `cond_off`, `comp_off_id`, `flevel_excl`, and `assump_frag_tree` all survive), explicitly asserted `assump_frag_tree` excludes the copied `ExAssump` leaf (and source `fe_assump`), and verified lifting `assump_frag_tree` transitions `excl_host_id` to `(FoldOutcome::Value(med), FoldOutcome::Value(med))`.
- Isolated `sh_unrated` in `unrated_single_host_id` and separated `sh_shared` into `single_shared_host_id` from `sh_plain` in `single_host_id` so `IndependentFold` verifies `(Value(low), Value(low))` on `sh_plain`, `(Value(high), Value(low))` on `sh_shared`, and `(Unrated, Unrated)` on `sh_unrated`, directly asserted `ctrl_same == low`, `{cond_host_local, cond_frag_tree}`, and `review_marked == true` on `sh_plain`, and verified `cond_frag_tree`/`cond_host_local` exclusion transitions via `IndependentFold`.
- Production line delta vs `e9f51b4b0148f0ec0052cc2a13be9f5ee51cdcf7` remains 188 insertions / 8 deletions (196 changed lines, within the 480 cap).
NEXT: reviewer
