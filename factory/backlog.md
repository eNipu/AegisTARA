# Factory backlog

`/factory next` runs the first unchecked item. Add one feature per line.

- [x] Create the `aegis-model` crate: arena-based attack tree with typed `NodeIdx(u32)` IDs and a UUIDv7-to-index map (standards D1–D3) (001-aegis-model)
- [x] Add cycle detection to `aegis-model` with Kahn's algorithm, reporting the cycle path as an error (A3) (002-cycle-detection)
- [x] Add a `cargo-deny` config that enforces the license policy from decision D5 (003-cargo-deny)
- [x] Benchmark storage for linked parents in `aegis-model`: `Vec<NodeIdx>` vs `Box<[NodeIdx]>` vs a sparse `BTreeMap<NodeIdx, Vec<NodeIdx>>` for shared nodes only, at the large and stress reference sizes (memory with `dhat`, `link`/`parents()` time with `criterion` or `divan`); keep all three as a lesson benchmark (P2, L2) and record the result in an ADR (004-parents-storage-bench)
- [x] Attack tree evaluation in `aegis-model`: evaluate AND, OR and sequential AND gates once per node in topological order with memoization (A2), over a pluggable attribute domain (for example min/max feasibility), with leaf values supplied by the caller (005-tree-evaluation)
- [x] Explain-why traces for attack tree evaluation: every computed node value records which children and which gate rule produced it, so any value can be explained step by step (006-explain-why)
- [x] Property tests for attack tree evaluation: random DAGs (proptest) check determinism (P4), that each node is evaluated exactly once, and agreement with a naive recursive evaluator (007-eval-property-tests)
- [x] Workspace CI: a GitHub Actions workflow that runs `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` and `scripts/deny.sh` on every push and pull request (008-workspace-ci)
- [ ] Domain model v0, part 2: `Asset` with security properties, `DamageScenario` with the four impact categories (safety, financial, operational, privacy), `ThreatScenario` with STRIDE and an R155 Annex 5 catalog ID, plus a typed operation API that applies validated changes
