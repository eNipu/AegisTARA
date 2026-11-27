Commit: 5e124b3 Evaluate fragment dependency closures and cross-tree why paths (142d)

Files:
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/compat.rs
- crates/aegis-model/tests/shared_fragments.rs
- crates/aegis-model/tests/shared_fragments.proptest-regressions
- crates/aegis-client/tests/engine_compatibility.rs

Done when:
1. `cargo test --locked -p aegis-model --test shared_fragments --test ops_properties --test store --test workspace` — PASS (11, 27, 26 + 1 existing ignored, 24 passed).
2. `cargo test --locked -p aegis-client --test engine_compatibility` — PASS, 5 passed.
3. `scripts/ci.sh crates/aegis-model` — PASS (fmt, clippy, test all ok).
4. `cargo test --locked -p aegis-model --lib compat::tests` — PASS, 5 passed.

Notes:
- `cargo fmt --all` run before final checks. Staged diff reviewed; only scoped code/tests committed with configured identity and co-author trailer. factory/board.json and untracked 142b/142c artifacts left untouched; this build.md is not in the commit.
- Production changed lines vs 4ea7108: project.rs 253+20, compat.rs 6+8 (including inline unit test) = 287, under the 574 cap. No new dependency, FORMAT_VERSION stays 1, eval.rs untouched.
- Evaluation: `closure_evals` collects the requested tree's fragment closure, orders it with 142b `fragment_order` (skipped when the tree has no references), and evaluates each closure tree once per mode, storing its lifted root and lifted tree-local Evaluation. `lifted_leaf` gives a reference step its target root lowered by the host step's own mode-counted ceilings (`entry_local_ceiling`, split out of `entry_step_feasibility_for`); local applicability exclusion is checked first. `reference_root`: TreeNotApplicable (tree-level exclusion or all-excluded root) -> Lifted::Excluded; any other error -> unrated (MissingLeafValue at the host), never a default level. Public Evaluation indices remain tree-local.
- Cache: `rebuild` walks dependency order and reads already-cached target roots (`cached_roots`); `update_tree_mode`/`build_lifted_tree` use the same. In a project with references, any applicability change and every non-metadata op ends in a full rebuild (Use/Unuse always rebuild); the metadata arm returns without rebuilding. Incremental invalidation is left for 142e. Tipping points use closure roots too.
- `Project::fragment_why_path(tree, node, residual)` walks selected derivations iteratively, crossing a reference leaf to the target root (host step then target root in the path), at most MAX_FRAGMENT_DEPTH hops, same mode throughout; generic `Evaluation::why_path` unchanged. It always crosses a reference, even if a host ceiling decided; 142g resolves ceiling causes.
- Docs state `effective_step_feasibility`/`residual_step_feasibility` stay own/shared-only O(leaf) (None for references) and `deciding_step` stays tree-local (None when the chain ends at a reference).
- Not explicitly listed in Do: lint `AttackTreeStepUnrated` now skips reference steps (the fragment's own steps are linted), otherwise every valid reference would be a permanent lint error and the "no lint errors => evaluate succeeds" contract would be wrong the other way. One-condition change in project.rs; reviewer may want to confirm.
- Compat: ENGINE_SEMANTICS_VERSION=2, protocol/minimum 2, FORMAT_VERSION 1. Historical v1 ("0.1.0", 1/1/1) headerless success kept separate; current headerless unit rejection now expects EngineSemanticsMismatch {client:1, server:ENGINE_SEMANTICS_VERSION}; integration still expects IncompatibleClient with updated comment/message. Future-semantics cases in both files remain ENGINE_SEMANTICS_VERSION + 1 (now 3) with protocol/min 1; historical protocol-2 fixtures unchanged.
- Tests: explicit regressions (reference folds root in both modes, unlink/undo, unrated fragment -> MissingLeafValue in live and cached paths, unrelated tree unaffected, lint), cross-tree why paths through two nested references with a planned mitigation changing only the residual path, excluded fragment under OR/AND and a local host condition. Property oracle (64 seeded cases): generated 6-tree DAG (forced diamond 0->2,3->4, two references per target, nested gates, shared steps, conditions, Implemented/Planned/Off mitigations, excluded and unrated fragments, an unrelated always-invalid tree) folded recursively without production order/evaluate/cache; every node and mode is compared for live and cached evaluation after each forward op, its inverse and redo, with incremental cache == fresh cache, then snapshot and workspace reloads equal live values. Mutation checks (dropping host ceilings, treating an excluded fragment as unrated, removing the post-op rebuild) each fail the suite.
- Oracle detail: applicability alone (step conditions) excludes a tree whose root no path reaches, before any rating check; a fragment-excluded reference only acts through the Applicable domain. The committed proptest-regressions seeds came from that oracle bug and now pass.
- No unmet criteria.

NEXT: reviewer
