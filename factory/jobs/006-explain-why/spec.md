# 006-explain-why: Explain-why traces for attack tree evaluation

## Open questions

1. **How the domain reports "which child decided".** A generic fold cannot tell which child produced a gate's value. This spec adds three *provided* methods to `AttributeDomain` (`or_kept`, `and_kept`, `seq_and_kept`) that return the combined value plus a `Kept` tag (`Acc`, `Next`, `Both`). The default calls the plain method and returns `Both` ("all inputs contributed"), which is always truthful, so existing domains keep compiling and stay correct. Alternatives: a separate `TracedDomain` trait (two code paths in `evaluate`), or guessing by value equality (wrong for sums that happen to equal a child). Confirm.
2. **Always on or opt-in.** This spec records the trace on every `evaluate` call (one 8-byte record per node, about 0.8 MB at `large`), because the product promise is "every number explains itself" and the full-recompute budget has 99% headroom (ADR 0005: 1.9 ms of 200 ms). The alternative is a separate `evaluate_traced`. Confirm.
3. **Ties.** When two children have equal value, the trace names the earlier child in stored order (matching `MaxMin`'s "keep `acc` on a tie"). It does not list all tied children. Confirm.
4. **`Both` granularity.** After a `Both` step, the trace says `Combined` (several children contributed) and the explanation lists all children with their values. It does not track which prefix of children contributed when a later `Acc` follows. Confirm this is precise enough for v1.
5. **Text rendering.** This job delivers the data and a typed explanation API, not human-readable sentences ("Feasibility High because ..."). Wording, labels, and localisation belong to the UI and report jobs. Confirm.

## Goal

After an evaluation, any node's value can be explained step by step. For every node the result records how its value was produced: supplied by the caller (a leaf step), selected from one named child by the gate's rule (for example "OR takes the most feasible child: child S7"), or combined from all children by the gate's rule (for example "SAND sums the times of its children"). A caller can ask for the explanation of any node (its value, its rule, every input child with its value, and the deciding child) and can follow the chain of deciding children from the root down to the step that dominates the result ("Feasibility High comes from path P3"). This is the engine side of the "Why?" button and the report appendix. No UI yet.

## Scope

In:
- `Kept` enum and three provided `*_kept` methods on `AttributeDomain`, overridden by `MaxMin` and `MinTime` (selection rules report `Acc` or `Next`; `MinTime`'s SAND sum reports `Both`).
- `Derivation` enum, one per node, stored in `Evaluation<V>` (new field, D10: still derived data).
- `evaluate` uses the `*_kept` methods and records a `Derivation` for every node in the same single pass.
- Read API on `Evaluation`: `derivation(idx)`, `explain(tree, idx) -> Option<Explanation>`, `why_path(idx)` (lazy iterator of deciding children).
- Unit tests, integration tests in a new `tests/explain.rs`, doctests, a re-run of `benches/eval.rs` with before and after numbers.
- ADR `docs/adr/0006-explain-why-traces.md` (L1).

Out:
- Human-readable text, labels in explanations, the "Why?" button, report appendix (UI and report jobs).
- Traces for formulas, risk matrices, attack potential factors (later engine jobs; this job covers gate folds only).
- Incremental recompute (A1), Pareto sets, top-k paths (A4), BDDs, serialization of traces.
- The random-DAG proptest suite (next backlog item). Do not add proptest strategies here.
- Moving code to an `aegis-engine` crate.

## Design notes

Crate: `crates/aegis-model/` only. Files: `src/eval.rs` (types, trait methods, `evaluate`, accessors), `src/lib.rs` (re-exports, crate docs mention explain-why and ADR 0006), new `tests/explain.rs`, new `docs/adr/0006-explain-why-traces.md`. `tests/eval.rs` and `benches/eval.rs` must keep passing unchanged (small additions allowed, no weakened asserts). Do not touch `src/tree.rs` except doc links. No new dependencies.

**Trait addition** (provided methods, no breaking change):
```rust
/// Which inputs of one fold step the result came from.
pub enum Kept { Acc, Next, Both }

// in AttributeDomain:
fn or_kept(&self, acc: Self::Value, next: &Self::Value) -> (Self::Value, Kept) {
    (self.or(acc, next), Kept::Both)
}
// same for and_kept, seq_and_kept
```
Contract (doc comment): the value MUST equal what the plain method returns; return `Acc` only if the result is `acc` unchanged, `Next` only if it is `next`'s value alone, otherwise `Both`. `MaxMin`: OR and AND report `Next` when `next` replaced `acc`, else `Acc` (tie keeps `Acc`). `MinTime`: OR (min) and AND (max) report `Acc`/`Next` the same way (tie keeps `Acc`), SAND (saturating sum) reports `Both`. Override the plain methods to share one comparison with the `_kept` version (or the reverse), so the two cannot disagree.

**Per-node record** (`Copy`, `Debug`, `PartialEq`, `Eq`; at most 8 bytes, checked by a test):
```rust
pub enum Derivation {
    Leaf,                                         // value from the caller's leaf closure
    Selected { rule: GateKind, child: NodeIdx },  // value is exactly `child`'s value
    Combined { rule: GateKind },                  // value combines several children
}
```
Fold state machine per gate, children in stored order: start `Selected(c1)` (the first child's value is cloned). For each next child `ci`: `Acc` keeps the state; `Next` sets `Selected(ci)`; `Both` sets `Combined`. A later `Next` after `Combined` returns to `Selected(ci)` (the value was fully replaced). A one-child gate is `Selected(c1)`.

**Storage:** `Evaluation<V>` gains `derivations: Vec<Derivation>` in arena order, filled in the same loop as `values`, `with_capacity(tree.len())` (R2). Struct-of-arrays with the values (D4): explanation reads do not touch values unless asked. The existing derives (`Clone`, `Debug`, `PartialEq`, `Eq`) include the new field, so two equal evaluations also have equal traces (P4).

**Read API** on `Evaluation<V>` (each doc comment states its cost, A8):
- `derivation(idx) -> Option<Derivation>`: O(1); `None` for an index outside the evaluation.
- `explain<'a>(&'a self, tree: &'a AttackTree, idx) -> Option<Explanation<'a, V>>`: O(1) to build. `None` if `idx` is outside the evaluation, or the tree's node at `idx` does not match the record (a step recorded as a gate, a gate recorded as a leaf, or a gate kind different from `rule`), which catches use with a different or edited tree. Borrowing the tree avoids copying child lists (R1).
- `Explanation<'a, V>`: `node()`, `value() -> &V`, `derivation()`, `rule() -> Option<GateKind>`, `decisive() -> Option<NodeIdx>` (the `Selected` child), and `inputs() -> impl Iterator<Item = (NodeIdx, &V)>` over the gate's children in fold order with their values (empty for a leaf). `inputs` is O(k) for k children, no allocation.
- `why_path(idx) -> impl Iterator<Item = NodeIdx>`: yields `idx`, then follows `Selected` children until it reaches a `Leaf` or a `Combined` node (which it yields last). Lazy, O(depth) time, O(1) space, no allocation, no recursion. Defense in depth: it yields at most `len()` items. Empty if `idx` is outside the evaluation. Does not need the tree.

**`evaluate`:** same signature, same errors, same single pass, same counts (each leaf closure once, E - G combine calls; the default `*_kept` calls the plain method exactly once, so existing counting tests stay valid). Extra time O(1) per fold step; extra space 8 bytes per node. Update its doc comment: time O(V + E), extra space O(V) for order, values, and derivations.

Project-sized operations: `evaluate` O(V + E) time, O(V) space; `why_path` O(depth); `explain` O(1) plus O(k) for `inputs`. Nothing O(V^2) (A9).

**ADR 0006:** context (brainstorm section 8.2, G6), options from Open questions 1 and 2 with the trade-offs, the `Derivation` layout and its measured size, the state machine, tie rule, the shared-subtree limit from ADR 0005 (a `why_path` through a shared step is still exact for selection domains), and `cargo bench --bench eval` medians at `large` and `stress` before (base commit) and after this job, with the overhead in percent against the 200 ms / 3 s budgets.

## Acceptance criteria

All commands run from `crates/aegis-model/` unless noted.

- [ ] AC1 On a hand-built three-level tree (root OR over an AND and a SAND, each over steps) with `MaxMin<u8>`, every node's `derivation` is checked: steps are `Leaf`, each gate is `Selected` with the expected rule and child. Test: `cargo test --test explain maxmin_derivations`.
- [ ] AC2 With `MinTime` on the same children, AND is `Selected { rule: And, child: slowest }` and SAND is `Combined { rule: SeqAnd }`. Test: `cargo test --test explain min_time_derivations`.
- [ ] AC3 Ties name the earlier child in stored order, for both built-in domains, including a child added by `link`. Test: `cargo test --test explain ties_pick_first_child`.
- [ ] AC4 A custom domain that implements only `or`, `and` (no `*_kept`) gets `Combined` for every gate with two or more children and `Selected(c1)` for one-child gates, and its values equal the 005 results. Test: `cargo test --test explain default_kept_is_combined`.
- [ ] AC5 State machine: a test domain whose `*_kept` returns a scripted sequence of `Kept` values covers `Acc`, `Next`, `Both`, and `Next` after `Both`, and the recorded `Derivation` matches the rules in Design notes. Test: `cargo test --test explain kept_state_machine`.
- [ ] AC6 Built-in `*_kept` values equal the plain methods for all `u8` pairs (`MaxMin<u8>`) and a fixed set of `u64` pairs including `u64::MAX` (`MinTime`), and the tag is right. Test: `cargo test --lib kept_matches_plain`.
- [ ] AC7 `explain` returns the node's value, rule, decisive child, and `inputs()` in stored child order with each child's value; a leaf has no rule, no decisive child, and no inputs. Test: `cargo test --test explain explain_contents`.
- [ ] AC8 `explain` returns `None` for an index outside the evaluation and for a tree whose node at that index has a different kind or gate kind than recorded. Test: `cargo test --test explain explain_rejects_mismatch`.
- [ ] AC9 `why_path` from the root of a `MaxMin<u8>` DAG with a shared step yields the chain of deciding children down to the dominating step, and each consecutive pair is a parent and child in the tree; it stops at a `Combined` node with `MinTime`; it is empty for an out-of-range index. Test: `cargo test --test explain why_path_follows_decisive`.
- [ ] AC10 Traces agree with a naive recursive evaluator written in the test file (which also computes the deciding child) on every node of the 005 fixtures, for both built-in domains. Test: `cargo test --test explain matches_naive_trace`.
- [ ] AC11 Determinism and depth: evaluating twice and on a `clone()` gives equal `Evaluation`s including derivations; `why_path` on a 100,000-deep chain runs on the default test thread and yields 100,001 items. Test: `cargo test --test explain trace_deterministic_and_deep`.
- [ ] AC12 `Derivation` is at most 8 bytes. Test: `cargo test --lib derivation_size`.
- [ ] AC13 All 005 tests still pass, including the exact call counts. Test: `cargo test --test eval`.
- [ ] AC14 Every new public item has a doc comment with a compiling example and states its time and space cost (Q5, A8). Check: `cargo test --doc` and `cargo clippy --all-targets -- -D warnings`.
- [ ] AC15 No `unwrap`, `expect`, `panic!`, or panicking indexing in `src/eval.rs` outside `#[cfg(test)]`. Check: `grep -nE '\.unwrap\(|\.expect\(|panic!|\[[a-z_]+\.index\(\)\]' src/eval.rs` shows hits only inside test modules.
- [ ] AC16 Benchmark still meets the budgets, and ADR 0006 reports before and after medians at `large` and `stress`. Check: `cargo test --benches`, `cargo bench --bench eval`, `test -f ../../docs/adr/0006-explain-why-traces.md`.
- [ ] AC17 License policy still passes. Check (from the repo root): `scripts/deny.sh licenses bans sources`.

## Checks

From `crates/aegis-model/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --benches
cargo bench --bench eval
```

From the repo root:

```sh
scripts/deny.sh licenses bans sources
```
