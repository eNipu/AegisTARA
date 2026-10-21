## Round 1

Commits:
- 88ba10b Make per-asset property reference counts sparse
- 41bba94 Gate kept load memory at the 3x server budget and add targeted-asset shapes
- ce093fa Record the sparse reference-count decision and corrected load memory numbers

Files:
- crates/aegis-model/src/project.rs
- crates/aegis-model/benches/store_memory.rs
- crates/aegis-model/src/store/snapshot.rs (only the memory paragraph of the `Project::load` doc comment)
- docs/adr/0010-sparse-reference-counts.md (new)
- docs/adr/0009-save-load-format.md
- docs/adr/0008-methodology-neutral-domain-model.md

Open question 1: implemented the owner's answer from rework.md. The 3x budget limits kept (the heap after `load` returns). Every loading case has a kept gate of at most 3.0. The `damages_only_max` peak (3.53x) stays gated at 4.8x and is documented, not held to 3x.

Acceptance criteria:
- [x] AC1 Public API, behaviour and formats unchanged.
  - `git diff --name-only master...HEAD -- crates/aegis-model` lists only `benches/store_memory.rs`, `src/project.rs` and `src/store/snapshot.rs`. Nothing under `tests/` or `fuzz/`, and neither `Cargo.toml` nor `Cargo.lock`.
  - The `pub` diff grep prints nothing.
  - `cargo test --locked` passes: 15 result lines, all ok, including the golden fixtures and the existing integration tests (unchanged).
- [x] AC2 `cargo test --lib refcounts_are_sparse` passes, covering (a) to (f) on `builtin()` and `second()`. For (d), the test picks the pair of categories with different `violates` that share the most properties. In `second()` that pair shares a property, so the "shared keeps its count" path runs; `refcounts_category_pair_shares_a_property_in_second` guards this. In `builtin()` every category violates one property, so (d) checks the move only. (d) also checks that the returned inverse op restores the row.
- [x] AC3 `check_invariants` rebuilds the rows from the threats on its own, through a `BTreeMap<(slot, property), count>` rather than `count_row`, so a bug in the update code cannot hide. It checks canonical form per row (no zero counts; sorted and unique), then compares exactly, then compares the derived `==`. `cargo test --lib check_invariants` passes 2 tests, including the new `check_invariants_detects_bad_property_counts` (wrong count, `threats: 0` entry, two entries out of order, each rejected; restoring the good row passes again). `cargo test --lib project_invariants` passes both 256-case proptests.
- [x] AC4 `cargo bench --bench store_memory` passes, and so does `cargo test --benches`.
  - `SERVER_BUDGET = 3.0`. Every case has a `max_kept`. The bench fails on kept > `max_kept`, and on a loading case with `max_kept > SERVER_BUDGET`.
  - `assets_only_max`: `max_factor` 3.3 (was 8.3), `max_kept` 2.2. Measured 2.41 peak and 1.78 kept.
  - The new cases pass: `targeted_narrow_builtin` (2.53 / 1.72), `targeted_narrow_max` (2.57 / 1.76) and `targeted_wide_max` (1.57 / 1.36).
  - No gate is above its job 010 value. Every peak gate went down or stayed the same; table in ADR 0010.
  - The AC4 grep for 8.3 prints nothing.
  - Bite test 1: set `assets_only_max` `max_kept` to 1.0. `STORE_MEMORY_CASE=assets_only_max cargo bench --bench store_memory` failed with `"assets_only_max: kept 1.78 x file exceeds 1"`. Reverted.
  - Bite test 2: set `assets_only_builtin` `max_kept` to 3.1. The bench failed with `"assets_only_builtin: kept gate 3.1 exceeds the server budget 3"`. Reverted. The file was restored from a copy, and the committed bench matches the version that passed.
- [x] AC5 Medians from `cargo bench --bench store` on the same machine. Master ran 3 times; the third master run used master's `src/project.rs` swapped into the worktree, back to back with a branch run.

  | Bench | master | branch |
  |---|---|---|
  | load | 5.396, 5.346, 5.221 ms | 5.135, 5.157, 5.207 ms |
  | replay_log | 9.807, 9.290, 8.960 ms | 8.936, 8.858, 9.050 ms |

  Worst branch run against best master run: load -0.3%, replay_log +1.0%. Both are within +10%. Recorded in ADR 0010, section "Time (AC5)".
- [x] AC6
  - (a) ADR 0010 exists. It has the context, the options table with bytes, the decision, a before/after peak and kept table for all 18 cases plus the times, and the complexity statement. Check: a-ok.
  - (b) ADR 0009 has an "ADR 0010 peak / kept" column in both tables and the new budget statement: kept within 3x for every shape and every methodology size measured, with the 3.53x peak explained. The Follow-up paragraph is replaced by a pointer to ADR 0010. Check: b-ok (no `does **not** hold`).
  - (c) ADR 0008's section has a "Changed by ADR 0010" note. Check: c-ok.
  - (d) The `Project::load` doc states built-in 2.73x peak / 1.85x kept, and max vocabulary 3.53x peak / 2.65x kept. Check: d-ok.
  - (e) The `RefCounts` doc comment states space and time costs.
- [x] AC7 The awk/grep prints nothing. `forbid(unsafe_code)` is present in lib.rs. Every new non-test access uses `get` or `get_mut`.
- [x] AC8 The neutrality grep prints nothing.
- [x] AC9 All of these pass: `cargo test`, `cargo test --benches`, clippy with `-D warnings`, `cargo fmt --check`, `scripts/ci.sh crates/aegis-model` ("all steps passed") and `scripts/deny.sh licenses bans sources` (ok for aegis-model and the fuzz crate).

Checks:
- cargo fmt --check → pass
- cargo clippy --all-targets -- -D warnings → pass
- cargo test → pass
- cargo test --locked → pass
- cargo test --benches → pass
- cargo bench --bench store_memory → pass
- cargo bench --bench store → pass (numbers above)
- scripts/ci.sh crates/aegis-model → pass
- scripts/deny.sh licenses bans sources → pass
- git diff --name-only master...HEAD -- crates/aegis-model → only allowed files
- AC1 pub grep → empty
- AC7 grep → empty
- AC8 grep → empty
- test -f docs/adr/0010-sparse-reference-counts.md → pass
- 1.85 toolchain: `cargo +1.85` is not routed through rustup on this machine ("no such command: +1.85"), so I ran `rustup run 1.85 cargo check --locked` and `--all-targets` instead → pass

Notes:
- **A second breach found and fixed.** On master, the new `targeted_narrow_max` (one minimal threat per minimal asset, 64-property methodology) kept **3.60x** (peak 4.41x). The job 010 bench did not cover that shape. After this job: 1.76x kept.
- **One shape got worse, as designed.** In `targeted_wide_max` every asset has a full 64-entry row. An entry costs 8 bytes (u8 + u32, padded) against the dense row's 4. Kept rose from 0.98x to 1.36x, still well under 3x. The ADR records this as the trade-off, and names the `Option<Box<{PropertySet, Box<[u32]>}>>` layout as the way to halve it if it ever matters.
- **`t63` change.** For `targeted_wide_max`, the `short` methodology's last category `t63` now violates all 64 properties instead of only `p63`. No other case uses `t63`. The `short_keys` doc and the bench header say so.
- **Update algorithm.** `count_row` first checks whether the row's set of properties changes, which takes O(r) with a `PropertySet` built from the row. If it does not change, it updates in place with no allocation. If it does, it merges into an exact-size `Vec` over the union bitset and calls `into_boxed_slice`, which does not reallocate because capacity equals length. `SetThreatCategory` counts the new category in before the old one out.
- `Arena::slot_count` is now used only by test code, so it is `#[cfg(test)]` to keep clippy's dead-code lint quiet.
- `check_invariants`' doc now says O(n log n + L + m), because the independent recount uses a `BTreeMap`. It is test-only.
- The flat-index overflow check in `check_add_asset` is removed, as the spec settled. The slot-limit check stays.
- More unit tests beyond those the spec named: `count_row_keeps_canonical_form` (in-place and merge paths, inserts before, between and after, a missing property ignored on count-out, emptying the row), `row_count_looks_up_by_property`, `refcounts_ignore_missing_asset_slots` and `refcounts_category_pair_shares_a_property_in_second`.
- dhat gives the same numbers in debug (`cargo test --benches`) and release.
