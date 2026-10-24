# Build Report: 025-incremental-evaluation-cache

## Summary

Implemented the incremental evaluation cache for both standalone `AttackTree` DAGs (`TreeEvalCache<V>`) and full `Project` graphs (`ProjectEvalCache`), fulfilling engineering standards rules D10 (derived data is a separate, rebuildable cache), A1 (incremental recomputation of changed nodes, ancestors, and cross-module dependents in $O(\text{depth} \times \text{fan-out})$ time), A2 (each affected node visited at most once via rank-ordered max-heap propagation), and the $\le 5\text{ ms}$ performance budget on a `100,000`-node project.

## Files Changed

- `docs/adr/0023-incremental-evaluation-cache.md` — Recorded the architectural design, rank-ordered bottom-up propagation with value-change cutoff, in-place cross-module `ThreatRisk` synchronization, and `divan` benchmark measurements (`122.7 ns` at `100,000` nodes vs `2.118 ms` full evaluation; `124.7 ns` at `1,000,000` nodes vs `22.05 ms` full evaluation).
- `crates/aegis-model/src/tree.rs` — Added `link_count: u32` (stored in the existing 4-byte padding next to `root: NodeIdx` with zero increase in `size_of::<AttackTree>()`) and `AttackTree::edge_count` ($O(1)$), plus invariant verification in `AttackTree::check_invariants`.
- `crates/aegis-model/src/eval.rs` — Implemented `TreeEvalCache<V>` (`new`, `evaluation`, `into_evaluation`, `recompute_steps`, `recompute_steps_recording`) and internal helpers `Evaluation::set_node` and `Evaluation::sync_nodes`, plus unit tests including the `100,000`-node (`large`) performance test (`tree_eval_cache_100k_nodes_single_step_under_5ms`).
- `crates/aegis-model/src/project.rs` — Implemented `ProjectEvalCache` (`new`, `rebuild`, `update`, `evaluate_tree`, `evaluate_residual_tree`, `evaluate_threat`, `evaluate_residual_threat`, `evaluate_risk`, `evaluate_residual_risk`, `recomputed_nodes`, `recomputed_threats`, `reset_counters`), `Project::eval_cache`, and `Project::apply_cached`.
- `crates/aegis-model/src/lib.rs` — Re-exported `TreeEvalCache` and `ProjectEvalCache` at the crate root.
- `crates/aegis-model/benches/eval.rs` — Added `incremental_single_step` benchmark at `100,000` (`large`) and `1,000,000` (`stress`) nodes.
- `crates/aegis-model/tests/eval_properties.rs` — Added property test `random_dag_incremental_cache_matches_full_evaluate` verifying `TreeEvalCache::recompute_steps` against fresh `evaluate` across random DAGs and all three domains (`MaxMin<u8>`, `MinTime`, `Poly`).
- `crates/aegis-model/tests/ops_properties.rs` — Added property tests `eval_cache_soundness_first_methodology` and `eval_cache_soundness_second_methodology` verifying `ProjectEvalCache::update` (both forward `Op` and inverse `Op`) and `Project::apply_cached` undo sequences against `Project::evaluate_*`.
- `crates/aegis-model/tests/project.rs` — Added `project_eval_cache_incremental_updates_and_counters` verifying incremental node and threat recomputation counts across all operation categories.

## Verification

- `cargo fmt --manifest-path crates/aegis-model/Cargo.toml` — passed.
- `./scripts/ci.sh` — passed (`fmt`, `clippy --all-targets -- -D warnings`, all unit, integration, property, and doc tests).
- `./scripts/deny.sh` — passed (`advisories`, `bans`, `licenses`, `sources`).
- `cargo bench --manifest-path crates/aegis-model/Cargo.toml --bench eval` — passed (`incremental_single_step` median `122.7 ns` at `100,000` nodes and `124.7 ns` at `1,000,000` nodes).
