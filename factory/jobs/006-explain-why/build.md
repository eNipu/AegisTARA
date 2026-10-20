## Round 1
Commits: 37bfc13 Record explain-why traces in attack tree evaluation
Files:
- crates/aegis-model/src/eval.rs (Kept, Derivation, provided `*_kept` methods, MaxMin/MinTime overrides, `derivations` field, trace in `evaluate`/`fold`, `derivation`/`explain`/`why_path`, `Explanation`, unit tests)
- crates/aegis-model/src/lib.rs (re-exports `Derivation`, `Explanation`, `Kept`; crate docs mention explain-why and ADR 0006)
- crates/aegis-model/tests/explain.rs (new, 10 integration tests)
- crates/aegis-model/benches/eval.rs (header comment only: points to ADR 0006; workload unchanged)
- docs/adr/0006-explain-why-traces.md (new)

Acceptance criteria (all from `crates/aegis-model/`):
- [x] AC1 — `cargo test --test explain maxmin_derivations` → 1 passed
- [x] AC2 — `cargo test --test explain min_time_derivations` → 1 passed (AND = Selected slowest step, SAND = Combined, OR = Selected AND)
- [x] AC3 — `cargo test --test explain ties_pick_first_child` → 1 passed (OR tie between a linked child and a later owned child names the linked one; AND, SAND, root ties; both domains)
- [x] AC4 — `cargo test --test explain default_kept_is_combined` → 1 passed (MinCost with only `or`/`and`, on all 005 fixtures: Combined for k >= 2, Selected(c1) for k = 1, values equal a naive evaluator, plain-method calls = E - G)
- [x] AC5 — `cargo test --test explain kept_state_machine` → 1 passed (9 scripts incl. Acc, Next, Both, Next after Both, Both after Next; all three gate kinds; script fully consumed; value consistent)
- [x] AC6 — `cargo test --lib kept_matches_plain` → 1 passed (all 65,536 u8 pairs for MaxMin; 8x8 u64 samples incl. u64::MAX for MinTime, sum checked against a u128 reference)
- [x] AC7 — `cargo test --test explain explain_contents` → 1 passed
- [x] AC8 — `cargo test --test explain explain_rejects_mismatch` → 1 passed (out of range; step recorded as gate; gate recorded as leaf; different gate kind; same-count trees so the kind checks are exercised, plus an edited tree)
- [x] AC9 — `cargo test --test explain why_path_follows_decisive` → 1 passed (path [root, g2, s] goes through the linked edge g2 -> s, s is owned by g1; pairs are parent/child; MinTime stops at SAND; out of range empty)
- [x] AC10 — `cargo test --test explain matches_naive_trace` → 1 passed (value + derivation per node vs naive recursive evaluator on the 005 fixtures, both domains; plus every why_path is a parent-child chain ending at Leaf/Combined)
- [x] AC11 — `cargo test --test explain trace_deterministic_and_deep` → 1 passed (100,001 items on a 100,000-deep chain, default test thread)
- [x] AC12 — `cargo test --lib derivation_size` → 1 passed; measured size is exactly 8 bytes (Kept is 1 byte)
- [x] AC13 — `cargo test --test eval` → 10 passed; `tests/eval.rs` not modified
- [x] AC14 — `cargo test --doc` → 60 passed; `cargo clippy --all-targets -- -D warnings` → clean. Every new public item (Kept, Derivation, `or_kept`/`and_kept`/`seq_and_kept`, `derivation`, `explain`, `why_path`, `Explanation` and its 6 methods) has a doc example and states time/space cost
- [x] AC15 — `grep -nE '\.unwrap\(|\.expect\(|panic!|\[[a-z_]+\.index\(\)\]' src/eval.rs` → no hits at all (tests module included)
- [x] AC16 — `cargo test --benches` pass; `cargo bench --bench eval` pass; ADR exists. Medians: large before 1.955 / 1.946 ms → after 2.013 / 2.031 / 2.004 ms (+3.6%); stress before 20.14 / 20.41 ms → after 21.14 / 21.18 / 21.23 ms (+4.3%). After = 1.0% of 200 ms and 0.7% of 3 s
- [x] AC17 — `scripts/deny.sh licenses bans sources` (repo root) → ok

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (61 lib, 3 cycles, 10 eval, 10 explain, 11 parents_storage, 2 properties, 60 doc, 1 bench-crate)
- `cargo test --benches` → pass
- `cargo bench --bench eval` → pass (numbers above)
- `scripts/deny.sh licenses bans sources` → pass

Notes:
- `explain` also returns `None` when `tree.len() != self.len()`. This is an O(1) check beyond the spec's kind checks; it rejects any tree edited by `add_child` and is the invariant that lets `inputs()` find a value for every child without a panic path. It cannot detect an edit that keeps the count and kinds (a new `link`); documented on `explain` and in ADR 0006.
- The plain `or`/`and`/`seq_and` methods of `MaxMin` and `MinTime` now call their `_kept` versions (`.0`), so there is one comparison per rule. `MaxMin` overrides `seq_and_kept` as `and_kept` (otherwise its SAND gates would trace as `Combined`). The default `seq_and_kept` calls `seq_and`, not `and_kept`, because a domain may override `seq_and` alone.
- `Derivation::rule`/`decisive` are private helpers; the public way to read them is `Explanation::rule`/`decisive` (keeps the API surface to what the spec lists).
- `Explanation` implements `Clone`/`Copy`/`Debug` by hand so that `V` does not need `Clone` and the debug output does not dump the whole evaluation. `inputs()` uses `+ use<'a, V>` so the iterator does not borrow the `Explanation` itself (MSRV 1.85 supports precise capturing).
- The "before" numbers come from two runs of the base commit `a4c3e21`: one in the worktree before any change, one from a `git archive` copy of the base in the scratchpad, interleaved with the after runs.
- `tests/explain.rs` copies the 005 fixtures (three_level, ladder, random_dag, fixtures) from `tests/eval.rs`, because each integration test is its own crate and the spec asks to leave `tests/eval.rs` unchanged.
