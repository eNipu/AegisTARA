# Build 134b

Commit: 14d7573 Let attack paths skip excluded nodes without a value (134b)
(on top of 4958cc6 Leave excluded steps out of tree and risk evaluation; add not-applicable errors (134b))

Files:
- crates/aegis-model/src/error.rs (ThreatNotApplicable, TreeNotApplicable)
- crates/aegis-model/src/eval.rs (Evaluation::from_lifted, sync_lifted_nodes)
- crates/aegis-model/src/applicability.rs (dropped the dead_code allow; added pub(crate) `same_for_threat` for the diff, see Notes)
- crates/aegis-model/src/project.rs (uncached + cached evaluation, sensitivity, diff risk deltas)
- crates/aegis-model/src/paths.rs (step 7: `matches_tree` no longer requires a value on every node; doc updated)
- crates/aegis-model/tests/applicability_eval.rs (new, 9 tests)
- crates/aegis-model/tests/ops_properties.rs (lint check accepts not-applicable; tipping-point oracle uses the lifted domain)

Done when:
1. `cargo test --locked -p aegis-model --test applicability_eval` -> ok, 9 passed.
2. `cargo test --locked -p aegis-model --test ops_properties` -> ok, 11 passed (4 back-to-back runs, all green; the earlier flaky choke-point failure is gone).
3. `cargo test --locked -p aegis-model --test project --test eval --test explain` -> ok (10, 12, 31 passed).
4. `scripts/ci.sh crates/aegis-model` -> all steps passed (fmt, clippy, test). `cargo fmt --all` clean.

## Notes
- Uncached: each public call derives `applicability()` once and passes `&app` to `evaluate_tree_with` / `evaluate_threat_with` / `evaluate_risk_with` and to `entry_tipping_points`. The leaf is `Project::lifted_leaf` (Excluded first, then the rating), so an excluded unrated step never gives `MissingLeafValue`. Error order: unknown ID first, then threat excluded (`ThreatNotApplicable`, before `NoAttackTree`), then tree excluded (`TreeNotApplicable`), then the evaluation errors. A risk or sensitivity maps `TreeNotApplicable` to `ThreatNotApplicable(id)` through `not_applicable_threat`. `tree_tipping_points` and `residual_tree_tipping_points` now return `TreeNotApplicable` for an excluded tree, and they skip excluded steps.
- Cache: `CachedTreeEval` holds `Result<LiftedTree, _>` (`cache` + `view`). `update_tree_mode` checks "all rated" with the lifted leaf, runs `recompute_steps_recording` on the lifted cache, then `view.sync_lifted_nodes(..)`. If the cache did a full rebuild, the recompute failed, or the sync returned false, the slot is rebuilt with `build_lifted_tree`, which is the same code that `rebuild` uses. The old code stored the recompute error as-is; the rebuild now gives the same error that `Project` gives. `update` rebuilds the whole cache when `project.applicability()` differs from the cached one (134c makes this incremental).
- One change the prompt did not list but correctness needs: `Project::diff` risk deltas (`diff_risk_deltas`) only marked a threat as affected through entity diffs, so toggling a condition changed risks without a delta. ops_properties `diff_soundness_*` failed on this. A threat now also counts as affected when the two projects' applicability differs for the threat, its tree or the tree's nodes (`Applicability::same_for_threat`, pub(crate)). Both files are in the allowed list.
- ops_properties: `check_lint_invariants` now accepts `TreeNotApplicable` / `ThreatNotApplicable` where it required `Ok`, because lint does not report an excluded element. The tree tipping-point oracle now evaluates with `Applicable(MaxMin)` and excludes the same steps. Otherwise its plain `MaxMin` gave `MissingLeafValue` for an excluded unrated step. Every assertion keeps its meaning.
- `tests/common/mod.rs` unchanged.

## Rework 1
- Gate failure (`nested_or_over_and_drops_the_excluded_branch`: "no paths") fixed by step 7: removed the per-node `eval.get(idx).is_none()` check from `matches_tree` in `paths.rs`; node count, root value and derivation checks remain. Doc comment now says the root must have a value and other (excluded, APL-02) nodes may have none and are never on a path. `mismatched_tree_returns_none` unit test unchanged and passing.
- Nested test now asserts both `attack_paths` and `top_k_paths(tree, 10)` are non-empty and equal exactly `[[c]]` (step vectors), so it cannot pass vacuously.
- The earlier blocker section is resolved by the updated prompt.

NEXT: reviewer
