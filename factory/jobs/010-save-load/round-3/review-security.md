VERDICT: PASS

Scope: `git diff master...factory/010-save-load`, focused on round-3 commit 9948902 (single-kind memory gates, corrected worst-case docs, version-0 classification). Round-1 and round-2 items are already fixed and are not raised again. No new findings.

## Round-3 changes

- **`wire.rs` `FormatVersion` (version 0):** `visit_u64` now uses `cmp` against `FORMAT_VERSION`. Equal is accepted, greater gives `UnsupportedFormat(Some(v))`, and less gives `UnsupportedFormat(None)`. `visit_i64` and `visit_f64` still give `None`. There is no panic path and no new allocation, and the error still exposes only a `u64`. The unit test checks that version 0 maps to `None` and that its message does not say "newer". `cargo test --locked --lib store` passes on an export of the branch (22 tests).
- **Worst-case memory (R3):** the six new `store_memory` cases cover assets only, damages only and threats only. Each runs over both the built-in methodology and the maximum-vocabulary one, and each is gated at its measured factor plus about 35%. The bench builds the smallest valid entities. Assets must have a non-empty property set (`project.rs` rejects an empty one), so one property per asset is the densest valid asset. The worst case is now stated per methodology size: 2.73x peak (about 175 MiB at the cap) for the built-in methodology, and 6.16x (about 394 MiB) for 64 properties. Memory stays bounded by the 64 MiB file cap and is gated in CI (`cargo test --benches`). The caller chooses the methodology, not the file, so a hostile file cannot select the larger factor. Most of the cost is job 009's dense reference-count row, not the decoder (at most about 0.9x). ADR 0009 now states honestly that the 3x server budget fails for that shape and records the follow-up. This is a documented, bounded property of the in-memory model, not a decoder flaw, so it does not block.
- **Docs:** `Project::load`, `StoreError::UnsupportedFormat` and ADR 0009 now note that the methodology key is checked first only when the header comes first. Either way, a foreign file still fails before a project is built, and no key is reinterpreted. This matches what I checked in round 2.

## Checked, no issue

- **Q2/Q3:** `#![forbid(unsafe_code)]` is kept. A grep for `unwrap()`/`expect(`/`unsafe`/`panic!`/`unreachable!` over `src/store/` prints nothing. The new `unwrap_or`/`unwrap_or_default` calls are in bench code only.
- **Q4/R3/R6/R4:** no decoder caps, reader limits or queues changed in this round.
- **D7:** no hash maps are added.
- **D5:** no new dependencies. `Cargo.toml` and `Cargo.lock` are unchanged in this round.
- **Secrets:** the bench shapes, ADR text and tests contain no secrets, tokens or customer data.
