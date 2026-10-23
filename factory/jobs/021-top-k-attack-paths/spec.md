# 021 — Lazy top-k attack paths in `aegis-model`

## Goal

Enumerate the top-$k$ easiest minimal attack paths (minimal leaf-step sets realizing the root goal) lazily over an `AttackTree` and `Evaluation` — as well as inherent and residual evaluations on a `Project` — without materializing all $2^{\Omega(d)}$ paths in an AND/OR tree (rule A4).

## Scope

1. **Path types and best-first iterator (`crates/aegis-model/src/paths.rs`, `lib.rs`)**:
   - `AttackPath<V>` with `steps()`, `bottleneck()`, `feasibility()`, `len()`, `is_empty()`, and `contains(NodeIdx)`.
   - `AttackPaths<'a, V>` lazy best-first iterator ordered by exact `Evaluation` upper bounds and lexicographic `Or`-branch rank choices, with intrinsic cut-set minimality verification and canonical step-set deduplication on shared DAGs.
2. **Evaluation and Project APIs (`crates/aegis-model/src/eval.rs`, `project.rs`)**:
   - `Evaluation::attack_paths` and `Evaluation::top_k_paths`.
   - `Project::top_k_tree_paths`, `Project::top_k_residual_tree_paths`, `Project::top_k_threat_paths`, and `Project::top_k_residual_threat_paths`.
3. **Tests and ADR**:
   - Unit tests in `src/paths.rs`, integration tests in `tests/explain.rs` (including a $2^{20}$-path `AND`-of-`OR` tree) and `tests/project.rs` (inherent vs. residual path ranking shift under `SetStepMitigation`), property tests in `tests/eval_properties.rs` (agreement with an independent power-set minimal-cut-set oracle on random DAGs), and `docs/adr/0019-top-k-attack-paths.md`.
