VERDICT: PASS

Scope: dev-only bench and test code (`benches/common/mod.rs`, `benches/parents_time.rs`, `benches/parents_memory.rs`, `tests/parents_storage.rs`), dev-dependencies, and docs. Nothing under `src/` changed, and no shipped code path takes external input. All inputs come from compile-time `CASES`, fixed seeds, or proptest.

Checked, no findings:
- Q2: every new file has `#![forbid(unsafe_code)]`, and there is no `unsafe`.
- Q3: there is no `unwrap` or `expect` on untrusted data. The only `unwrap_or` calls are saturating conversions (`common/mod.rs:37`, `:231`) and lookups with a fallback (`parents_time.rs:39`, `parents_memory.rs:91`). Test-only indexing works on generated data.
- R3/R6: `shape()` clamps `n` to `u32::MAX` and `share_pct` to 100. Out-of-range children in the dense stores are ignored. Bench time is bounded with `sample_size = 1, max_time = 1`.
- R4: there are no queues. The two `OnceLock` caches are fixed-size (`[_; CASES.len()]` plus one context).
- D7: no hash maps. `SparseStore` uses `BTreeMap`, which is deterministic and has no hash-flooding surface.
- D5: `dhat 0.3.3` and `divan 0.1.21` are MIT OR Apache-2.0, pinned to a minor version, and dev-only. The transitive crates in `Cargo.lock` (clap, backtrace/gimli/object, parking_lot, serde_json, and others) are permissive, and build.md reports `deny.sh licenses bans sources` passing. Both crates are needed for the spec'd heap and time measurements.
- dhat runs in testing mode, so no `dhat-heap.json` is written into the repo.
- No secrets, tokens, customer data, or local filesystem paths appear in the code, tests, or ADR 0004. The ADR records only the machine model, OS, and `rustc` version.
