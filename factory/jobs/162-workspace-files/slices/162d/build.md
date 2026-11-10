Commit: 806be34 Fuzz and benchmark workspace files, record ADR 0065 (162d)
Files: crates/aegis-model/Cargo.toml (bench `workspace` registered, harness = false); crates/aegis-model/benches/workspace.rs (new); crates/aegis-model/fuzz/Cargo.toml (bin `workspace`, usage line); crates/aegis-model/fuzz/fuzz_targets/workspace.rs (new); crates/aegis-model/fuzz/seeds/workspace/fixture-sorted and fixture-reversed (new, 2,195 bytes each); docs/adr/0065-workspace-files.md (new); docs/architecture.md (one row for store/workspace.rs in the module inventory). No change under crates/aegis-model/src/ or tests/, no dependency added, Cargo.lock unchanged.
Done when:
- 1: `cargo bench --locked -p aegis-model --bench workspace -- --test` -> exit 0 (read and write each run once).
- 2: `cargo fuzz build workspace` -> skipped: cargo-fuzz is not installed (`cargo fuzz --version`: "no such command: fuzz"). Stand-in: `cargo check --locked --manifest-path crates/aegis-model/fuzz/Cargo.toml --bin workspace` on stable -> exit 0.
- 3: `scripts/ci.sh` -> "ok workspace fmt", "ok workspace clippy", "ok workspace test", "all steps passed in 8 crate(s)". Run after `cargo fmt --all`.
- 4: ADR 0065 "Measurements" and "Fuzz result" sections give every figure with its command, the machine and the sample statistics.
- Before commit: `cargo fmt --all` ran clean; the fuzz target (outside the root workspace) was formatted with `rustfmt --edition 2024`.
Measured: `cargo bench --locked -p aegis-model --bench workspace` and `--bench store`, same session, same machine: Apple M2 Max, 32 GiB, macOS 27.2. Workload: store_kit `typical` (300 assets, 2,000 damage scenarios, 2,000 threats). Median of 100 samples (fastest to slowest).
  - write: `to_workspace_files` 4.02 ms (3.72 to 4.41), 397 MB/s, 1,596,534 bytes in 4 files. Snapshot `save` 1.18 ms (1.09 to 1.33), 933 MB/s, 1,097,749 bytes. Ratio about 3.4x.
  - read: `from_workspace_files` 10.69 ms (10.21 to 11.42), 149 MB/s. Snapshot `load` 5.75 ms (5.43 to 6.10), 191 MB/s. Ratio about 1.9x.
  - Peak memory: not measured (see Notes).
Fuzz: not run. cargo-fuzz not installed; not installed, as the prompt says. Executions: 0. Findings: none (no run). The target compiles on stable (Done when 2).
Notes:
- Gap against the spec (factory/jobs/162-workspace-files/spec.md, item 9): the spec asks for the `large` reference project and for peak memory. The prompt for 162d asks for the store workload (`typical`), which is what was measured. The `large` run and the peak memory are not in this slice and need a planner decision. The 1.0 s open-project budget (engineering standards section 2) is for `large`; the typical read of 10.69 ms only says the reader is far inside that figure for this size, and does not prove it for `large`. No stop condition was hit: the read is not slower than the budget.
- Spec item 10 asks why descriptions and review marks keep their own files, but gives no reason. The ADR gives one drawn from the code: descriptions are one map over eleven element kinds plus project notes, and review marks are keyed by (tree, node). The owner should confirm or replace it.
- The seeds come from `tests/fixtures/project-v1.json` through a temporary example, `crates/aegis-model/examples/workspace_seed.rs`, which I deleted before the commit (it is not in the commit). The fixture gives 4 files, so the two seeds hold the same files in sorted and reversed order and differ only in order. The reversed seed also checks that file order does not matter.
- `fuzz/run.sh` lists only snapshot, op_log and pack, so the no-cargo-fuzz route cannot run `workspace` yet. I did not change it. Running the fuzzer for 600 s needs cargo-fuzz (`cargo +nightly fuzz run workspace corpus/workspace seeds/workspace -- -max_total_time=600` in crates/aegis-model/fuzz).
- Size: the workspace is about 45 % larger than the snapshot (1,596,534 vs 1,097,749 bytes), because of pretty-printing and the envelope on each file. Not a defect; a fact for the record.
- No defect found by the benchmark run. No input is shown to crash anything, because the fuzzer did not run.
NEXT: reviewer
