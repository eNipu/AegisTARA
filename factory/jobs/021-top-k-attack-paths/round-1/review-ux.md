VERDICT: PASS

Surface reviewed: top-$k$ attack-path enumeration API (`AttackPath`, `AttackPaths`, `Evaluation::attack_paths`, `Evaluation::top_k_paths`, `Project::top_k_tree_paths`, `Project::top_k_residual_tree_paths`, `Project::top_k_threat_paths`, `Project::top_k_residual_threat_paths`). No UI, CLI, or server surface in this change.

Flow:
- `AttackPath<V>` provides direct, zero-boilerplate accessors for explainability: `steps()` (`&[NodeIdx]` in canonical ascending leaf order with duplicates across shared DAG subtrees deduplicated), `bottleneck()` (`NodeIdx` of the leaf constraining the conjunction), `feasibility()` (`V`), `len()`, `is_empty()`, and `contains(NodeIdx)` ($O(\log L)$ binary search).
- `AttackPaths<'a, V>` is a lazy best-first iterator yielding minimal cut sets in descending `feasibility` order (with deterministic lexicographic `(steps, bottleneck)` tie-breaking). An `Or`-of-`And` DAG with $2^{16} = 65\,536$ minimal cut sets yields its top-10 paths in microseconds without materializing the exponential set (satisfying rule A4).
- `Project::top_k_tree_paths`, `Project::top_k_residual_tree_paths`, `Project::top_k_threat_paths`, and `Project::top_k_residual_threat_paths` let analysts compare inherent and residual top-$k$ paths in a single call; threats using only `direct_feasibility` cleanly return `Ok(Vec::new())` rather than forcing callers to branch on whether a tree is linked.
- Path #1 (`top_k_paths(tree, 1)[0]`) always agrees with `Evaluation::root_value()` and has `bottleneck()` equal to the terminal leaf of `Evaluation::why_path()`, keeping single-path and multi-path explanations consistent.

Errors:
- Reuses existing `ModelError` variants (`TreeNotFound`, `ThreatNotFound`, `Evaluation(EvalError::MissingLeafValue)`), which report only stable IDs and node indices without leaking labels.
