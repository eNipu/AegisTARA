VERDICT: PASS

Reviewed commit 37bfc13 on factory/006-explain-why (worktree factory/worktrees/006-explain-why). rework.md only records the owner's answers (all confirmed the spec's defaults); no override.

## Check results (run by me)

From crates/aegis-model/:
- cargo fmt --check -> pass
- cargo clippy --all-targets -- -D warnings -> pass
- cargo test -> pass (61 lib, 3 cycles, 10 eval, 10 explain, 11 parents_storage, 2 properties, 60 doc, 1 bench-crate)
- cargo test --benches -> pass
- cargo bench --bench eval -> pass; large 2.004 ms, stress 21.1 ms (build.md: before 1.95 / 20.3 ms, so +3.6% / +4.3%; far inside 200 ms / 3 s)
- grep -nE '\.unwrap\(|\.expect\(|panic!|\[[a-z_]+\.index\(\)\]' src/eval.rs -> no hits (AC15)
- tests/eval.rs unchanged vs master (AC13)

From repo root:
- scripts/deny.sh licenses bans sources -> pass (ok)

## Acceptance criteria

AC1-AC13 each have a named test that exists and passes (maxmin_derivations, min_time_derivations, ties_pick_first_child, default_kept_is_combined, kept_state_machine, kept_matches_plain, explain_contents, explain_rejects_mismatch, why_path_follows_decisive, matches_naive_trace, trace_deterministic_and_deep, derivation_size, tests/eval.rs). I read the ties and why_path tests: they cover the linked child at its stored position, the shared step reached through a link, parent/child pairs, MinTime stopping at the SAND, and out-of-range. AC14 doc examples pass and each new public item states its cost. AC16 ADR 0006 exists with before/after medians at large and stress, consistent with my bench run. AC17 pass.

## Standards

- D1/D2: arena order Vec<Derivation> indexed by NodeIdx, no new ID types needed. D4/D10: separate struct-of-arrays field, derived data. D7/P4: no maps; Evaluation derives include the trace and a test compares evaluations and clones. A8: every new public fn documents time and space. A9: nothing above O(V + E); why_path is O(depth) lazy via successors, capped at len(). R1: Explanation borrows the tree's child slice, no copies. No new dependencies.
- The plain MaxMin/MinTime methods call the _kept versions, so the two cannot disagree. MaxMin overrides seq_and_kept so SAND traces as Selected.

## Findings (non-blocking)

1. crates/aegis-model/src/eval.rs (evaluate, around `derivations.resize(tree.len(), Derivation::Leaf)`) — the vec is pre-filled in a separate O(V) pass and then overwritten per node, where the spec described filling it in the same loop with `with_capacity`. Cost is inside the measured overhead and the build.md notes explain it. Optional: leave as is.
2. crates/aegis-model/src/eval.rs (`if let Some(slot) = derivations.get_mut(...)`) — silently skips if the index were out of range; unreachable because the order lists every node and the vec is pre-sized. Acceptable for no-panic library code.
3. explain also rejects trees with a different node count (beyond the spec). It is O(1), documented in the doc comment and ADR, and what makes `inputs()` panic-free. Fine; a count-preserving `link` edit is not detected, which is documented.

No bugs, missed edge cases, or unneeded code found.
