## Round 1
Commits: 18be839 Add attack tree evaluation over pluggable attribute domains
Files:
- crates/aegis-model/src/eval.rs (new: `AttributeDomain`, `evaluate`, `Evaluation`, `MaxMin`, `MinTime`, unit tests)
- crates/aegis-model/src/error.rs (`EmptyGate(NodeId)`, `MissingLeafValue(NodeId)` with doctests)
- crates/aegis-model/src/lib.rs (module, re-exports, crate docs pointing to ADR 0005)
- crates/aegis-model/tests/eval.rs (new: 10 integration tests, naive recursive evaluator)
- crates/aegis-model/benches/eval.rs (new: divan bench at 100,000 and 1,000,000 nodes)
- crates/aegis-model/Cargo.toml (`[[bench]] name = "eval" harness = false`)
- docs/adr/0005-tree-evaluation.md (new)

Acceptance criteria (all `cargo` commands from `crates/aegis-model/`):
- [x] AC1 — `cargo test --test eval maxmin_values` → 1 passed. Checks every step, the AND (min), the SAND (min), and the root (max).
- [x] AC2 — `cargo test --test eval min_time_values` → 1 passed. AND and SAND over the same three linked steps: AND = 10 (max), SAND = 17 (sum); with the first step set to `u64::MAX - 1`, SAND = `u64::MAX` (saturated), no panic. Mutation check: dispatching SAND to `and` makes this test (and `seq_and_order`, `matches_naive`) fail.
- [x] AC3 — `cargo test --test eval seq_and_order` → 1 passed. String domain records the fold; root SAND children `[x, a, b, c]` with `c` added by `link` give `"((c|d)|(b&a))>a>b>c"`; a gate whose children are both links (`b`, then `a`) gives `"(b&a)"`.
- [x] AC4 — `cargo test --test eval evaluates_each_node_once` → 1 passed. Diamond: leaf closure called exactly once per step (0 for gates), domain calls == sum(children - 1) == 3, both gates see the shared step's value 9.
- [x] AC5 — `cargo test --test eval ladder_is_linear` → 1 passed (0.00 s). 64 levels x 2 gates, each linked to both next-level gates, 2 shared steps: (steps, gates, edges) = (2, 129, 258); leaf calls = 2, combine calls = 129 = E - G.
- [x] AC6 — `cargo test --test eval eval_errors` → 1 passed. `EmptyGate(g_id)` and `MissingLeafValue(a_id)`; Display contains the ID and not the label. Display text also pinned exactly in the doctests on both variants.
- [x] AC7 — `cargo test --test eval eval_deterministic` → 1 passed. On all 8 fixtures: two runs and a run on `tree.clone()` are `==`, have equal `Debug` text, and equal `iter()` output.
- [x] AC8 — `cargo test --test eval matches_naive` → 1 passed. 8 fixtures (single step, three-level tree, 6-level ladder, shared gate subtree under AND/OR/SAND, 4 seeded random 200-node DAGs with links), both built-in domains, every node compared with a recursive evaluator written from the domain definitions (MinTime leaves include `u64::MAX / 2` so SAND saturates).
- [x] AC9 — `cargo test --test eval deep_chain_eval` → 1 passed (0.22 s, debug). 100,000 nested gates (mixed kinds) over one step on the default test thread.
- [x] AC10 — `cargo test --test eval evaluation_accessors` → 1 passed. `get` is `None` for every index of a larger tree beyond the small tree; `Some(root_value()) == get(root)`; `iter()` order == arena order.
- [x] AC11 — `cargo test --benches` → exit 0; `cargo bench --bench eval` → exit 0. Medians in ADR 0005: `large` 1.948 ms (budget 200 ms), `stress` 20.56 ms (budget 3 s). A rerun gave 2.03 ms / 22.2 ms.
- [x] AC12 — `cargo test --doc` → 45 passed (+1 compile_fail); `cargo clippy --all-targets -- -D warnings` → exit 0. Every new public item (trait + methods, `evaluate`, `Evaluation` + 5 methods, `MaxMin` + `new`, `MinTime`, both error variants) has a compiling example and a time/space statement.
- [x] AC13 — `grep -nE '\.unwrap\(|\.expect\(|panic!|\[[a-z_]+\.index\(\)\]' src/eval.rs` → no hits at all (tests module included).
- [x] AC14 — `test -f ../../docs/adr/0005-tree-evaluation.md` → ok. Has context, options (trait shape A/B/C, empty-gate policy, callback vs map, dispatch), decision, `Evaluation` layout, complexity, the shared-subtree limitation, numbers, and what they teach.
- [x] AC15 — `scripts/deny.sh licenses bans sources` (repo root) → `ok aegis-model`, exit 0.

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (55 unit, 3 cycles, 10 eval, 11 parents_storage, 2 properties, 46 doc)
- `cargo test --benches` → pass
- `cargo bench --bench eval` → pass
- `scripts/deny.sh licenses bans sources` (repo root) → pass

Notes:
- `Evaluation<V>` layout: `Vec<Option<V>>` in arena order plus the root's value moved out of its slot (`Option::take`, no clone) into a field, so `root_value()` returns `&V` with no panic path. `get` special-cases the root index. Alternative (convert to `Vec<V>`) is discussed in the ADR.
- Invariant breaks that cannot happen after a successful topological order (a child without a value, a node in the order that is not in the tree, an empty root slot) return `ModelError::UnknownNode(idx)` instead of panicking; no new variant was added for them because the spec lists only two. Each site has an `INVARIANT` comment.
- `MaxMin` keeps the accumulator on ties (first child wins), tested with a type whose `Ord` ignores part of the value. It is implemented with manual `Clone`/`Copy`/`Default`/`Debug` so no bounds leak onto `T`; it holds `PhantomData<fn() -> T>` so it is `Send + Sync` for any `T`.
- Bench workload: 3% of the node count are link attempts; those that would point to a smaller index or repeat an edge are skipped, so 1.9% become real links (1,905 at `large`, 18,612 at `stress`). All links go from lower to higher index, so none can close a cycle. A context bench `topological_order_only` shows Kahn's order is about two thirds of `evaluate`.
- The bench prints a one-line fixture summary to stderr inside divan's table; cosmetic only.
- No change to `src/tree.rs`; no new dependencies.
