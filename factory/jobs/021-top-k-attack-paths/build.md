# Build Log — 021-top-k-attack-paths

## Summary of Changes

1. **Best-First Minimal Cut-Set Enumerator (`crates/aegis-model/src/paths.rs`, `crates/aegis-model/src/lib.rs`)**:
   - Implemented `AttackPath<V>` and `AttackPaths<'a, V>`, re-exported from `aegis-model`.
   - Used `BinaryHeap<SearchState<V>>` ordered by `(bound, Reverse(choices))` with per-state `expanded` tracking, intrinsic minimal-cut-set verification (`is_minimal_cut_set`) on DAGs with shared nodes, and canonical step-set deduplication.
2. **`Evaluation` and `Project` APIs (`crates/aegis-model/src/eval.rs`, `crates/aegis-model/src/project.rs`)**:
   - Added `Evaluation::attack_paths` and `Evaluation::top_k_paths`.
   - Added `Project::top_k_tree_paths`, `Project::top_k_residual_tree_paths`, `Project::top_k_threat_paths`, and `Project::top_k_residual_threat_paths`.
3. **Verification & Documentation**:
   - Added unit tests in `src/paths.rs`, integration tests in `tests/explain.rs` and `tests/project.rs`, and property test `random_dag_attack_paths_match_minimal_cut_set_oracle` in `tests/eval_properties.rs`.
   - Added `docs/adr/0019-top-k-attack-paths.md`.
   - Verified with `./scripts/ci.sh`.
