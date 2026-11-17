# Build 134a

Commit: 4e8233a Add the Applicable lifted domain and derived Project::applicability with reasons (134a)

Files:
- crates/aegis-model/src/eval.rs (Lifted<V>, Applicable<D>, private lift_or / lift_and helpers)
- crates/aegis-model/src/applicability.rs (Reason, Applicability, private TreeExclusions, Presence, Project::applicability and two private helpers)
- crates/aegis-model/src/project.rs (one pub(crate) accessor: step_conditions(tree, node))
- crates/aegis-model/src/lib.rs (re-exports Applicable, Lifted, Applicability, Reason)
- crates/aegis-model/tests/applicable_domain.rs (new)
- crates/aegis-model/tests/applicability.rs (new)

Done when:
1. `cargo test --locked -p aegis-model --test applicable_domain`: ok, 5 passed (4 hand-built incl. OR derivation, 1 proptest with 256 cases vs a naive memoized evaluator, trees up to 30 nodes with extra DAG links).
2. `cargo test --locked -p aegis-model --test applicability`: ok, 9 passed (8 hand-built rule tests, 1 proptest with 256 cases vs an upward-walking oracle).
3. `cargo test --locked -p aegis-model --doc`: ok, 762 passed, 0 failed.
4. `scripts/ci.sh crates/aegis-model`: fmt, clippy, test all ok. `cargo fmt --all` run.

Notes:
- Source size: 320 added non-comment, non-blank lines in src (640 with docs); cap 600.
- `condition_link_entries` already existed in project.rs (snapshot use); reused it. Only new accessor is `step_conditions`.
- `node_excluded_at` is `pub(crate)` and unused until 134b, so it carries `#[allow(dead_code)]` with a comment; 134b should drop the attribute.
- Gate evaluation runs only for trees with at least one excluded step; when `evaluate` errors (empty gate) the tree keeps its step reasons and gets no gate reasons. The oracle mirrors this (empty gate anywhere in the tree => no gate reasons).
- Tree owner rule: the tree gets one `OwnerExcluded(Threat)` per live threat using it, only when all are excluded. Owner exclusion of a tree does not mark its nodes (the prompt does not ask for it).
- The random generator in tests/common rarely creates conditions, component hierarchies or bound assets, so the applicability proptest adds a `shape` pass (child components, asset bindings, threat->tree links) and a `salt` pass (three new conditions, random element/step needs and step vias) before random present toggles; refused ops are skipped. Measured coverage over 256 checks before the change: owner reasons in ~27, via reasons in ~8, gate reasons in ~21 cases (the oracle check also runs on the unsalted project).
- Doc examples build small projects through public ops (one per public method).

## Rework 1

Commit: 7197189 Shorten repeated Applicability doc examples to fit the review cap (134a)

Gate: 645 counted lines, cap 600.

Fix: four of the per-method doc examples on `Applicability` (`is_excluded`, `reasons`, `is_node_excluded`, `excluded_nodes`) each rebuilt a whole project with the same component or tree setup as their neighbours. They now use `Applicability::default()` (one assert each, still a doc example per method). The project-based checks they made moved into the two remaining full examples: `excluded_elements` also asserts `is_excluded` on the interface; `node_reasons` also asserts `is_node_excluded` and `excluded_nodes().count() == 2`. `Project::applicability` keeps its full example. No code changed, no `rustfmt::skip`, no compression.

Counted src lines now: 590 (applicability.rs 424+2, eval.rs 149, lib.rs 5+3, project.rs 7).

Checks: `cargo fmt --all`; `scripts/ci.sh crates/aegis-model` all ok; `--test applicable_domain` 5 passed; `--test applicability` 9 passed; doc tests in applicability.rs 12 passed.

NEXT: reviewer
