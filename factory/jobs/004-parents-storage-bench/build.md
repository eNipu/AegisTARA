## Round 1
Commits: 72b0e8b Add linked-parents storage benchmarks and ADR 0004
Files:
- crates/aegis-model/Cargo.toml (dev-deps `dhat = "0.3.3"`, `divan = "0.1.21"`; two `[[bench]]` with `harness = false`; `rust-version = "1.85"` unchanged)
- crates/aegis-model/Cargo.lock
- crates/aegis-model/benches/common/mod.rs (Ix, ParentStore, VecStore, BoxStore, SparseStore, build, Shape, shape, shared_target, Case, CASES, SEED, inline SplitMix64)
- crates/aegis-model/benches/parents_time.rs (divan: `link`, `parents_all` over types x 6 cases, `attack_tree_link_1000`)
- crates/aegis-model/benches/parents_memory.rs (dhat testing mode, Markdown table, facts (a)/(b), ExitCode)
- crates/aegis-model/tests/parents_storage.rs
- docs/adr/0004-linked-parents-storage.md
- docs/adr/0002-dag-links-and-cycle-detection.md (one pointer added under Consequences)

Acceptance criteria:
- [x] AC1: `cargo test --test parents_storage stores_agree` runs 5 tests and all pass. Fixed grid n {1,2,50,2000} x share {0,1,10,50,100}, proptest with 128 cases (n <= 500, share 0..=100, any seed), link order vs index order, out-of-range and empty cases, distinct NAMEs.
- [x] AC2: `cargo test --test parents_storage matches_attack_tree` passes. It uses seeds 1, 2, 3 on `shape(2_000, 10, seed)` with all gates. Every `link` returns `Ok` (propagated with `?`), and `parents(idx) == [owner] ++ VecStore.linked(i)` holds for every node. It also asserts that the arena index equals `Ix`.
- [x] AC3: `cargo test --test parents_storage shape_` passes (determinism, seed difference, validity on a fixed grid up to n = 100_000, rounding of `shared_target`, and a 64-case proptest). Validity covers: parent < child, parent != owner, no duplicate pair, 1 to 3 links per shared child, shared count within 1 of target.
- [x] AC4: `cargo test --benches` exits 0 in about 33 s. parents_memory prints all 18 rows and "facts hold", and divan test mode lists every bench.
- [x] AC5: `cargo bench --bench parents_time` finishes in 13.8 s wall time and exits 0. The 36 case rows and `attack_tree_link_1000` are present (`grep -cE '(large|stress) [0-9]+%'` gives 36). See the notes on the store-name grep.
- [x] AC6: `cargo bench --bench parents_memory` exits 0 in about 21 s. It prints a table with 18 data rows and the columns size, nodes, share %, links, store, live bytes, bytes/node, live blocks, and build allocations.
- [x] AC7: `git -C ../.. status --porcelain | grep -c dhat` gives 0 (testing mode writes no file).
- [x] AC8: `../../scripts/deny.sh licenses bans sources` gives `ok aegis-model`. rust-version is still 1.85. Every resolved crate declares a rust-version <= 1.85 or none (checked with cargo metadata). Both divan and dhat are MIT OR Apache-2.0.
- [x] AC9: the ADR section grep prints 7 lines. The Numbers section holds all 18 memory rows and a 36-row timing table (medians plus per-item ns). It also gives the machine (Apple M2 Max, macOS 26.6.2), `rustc 1.99.1`, and base commit 0bce7bb.
- [x] AC10: `grep -n '0004' ../../docs/adr/0002-dag-links-and-cycle-detection.md` finds one line (156).
- [x] AC11: `git -C ../.. diff --stat master -- crates/aegis-model/src spikes docs/decisions.md docs/engineering-standards.md` is empty.
- [x] AC12: `grep -L 'forbid(unsafe_code)' benches/*.rs benches/common/mod.rs tests/parents_storage.rs` prints nothing. The trait, the stores, `build`, `shape`, and `shared_target` have doc comments with time and space costs and a "Teaches:" line.

Checks (from crates/aegis-model):
- cargo fmt --check → pass
- cargo clippy --all-targets -- -D warnings → pass
- cargo test → pass (51 + 3 + 11 + 2 + 32 + doctests)
- cargo test --benches → pass
- cargo bench --bench parents_memory → pass (exit 0, facts hold)
- cargo bench --bench parents_time → pass (exit 0)
- ../../scripts/deny.sh licenses bans sources → pass

Notes:
- AC5 grep. Divan prints each type once, as a tree node, with the six cases nested under it. So `grep -cE 'VecStore|BoxStore|SparseStore'` counts 6 store lines (3 types x 2 benches), not 36. The spec allows "as many as divan's tree output needs". All 36 data rows are there, as `grep -cE '(large|stress) [0-9]+%'` = 36 shows. Getting a literal 36 would mean giving up `types = [...]` (the spec's design) for an enum argument.
- The first `cargo bench --bench parents_time` run showed 15 min wall time but only 14.5 s CPU (1% utilisation). The machine stalled; it was not the bench. An immediate rerun took 13.8 s at 99% CPU. The ADR uses the numbers from the rerun.
- Decision recorded in ADR 0004: keep `Vec<Vec<NodeIdx>>` and create no follow-up job. In short:
  - `VecStore` costs about 1% of the 300 MB budget at `large` and is fastest on `link` and `parents_all`.
  - `SparseStore` lookups are 5 to 20x slower on the recompute pattern.
  - `BoxStore` makes about 4x the allocations.
  - The memory crossover for the sparse map is about 55% share against `VecStore` and about 30% against `BoxStore`.
  - Storage is under 0.5% of a real `AttackTree::link` call (about 2.14 µs).
- Memory-table store order is fixed: VecStore (today), BoxStore, SparseStore within each (size, share) group. It is not alphabetical, so the baseline reads first.
- `#[allow(dead_code)]` appears three times, each with a comment:
  - `ParentStore::NAME`: `parents_time` uses divan's type names instead.
  - `SEED`: the test uses its own seeds.
  - `CASES`: the test does not use it.
- `parents_memory` counts reallocations in "build allocations" (dhat `total_blocks` counts both). That count is how BoxStore's churn of about 2 per link shows up.
- `parents_memory` checks that each store holds every link (`linked` count == L). It runs after both heap readings, so it is not measured.
