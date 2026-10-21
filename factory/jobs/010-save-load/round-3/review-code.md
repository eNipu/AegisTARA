VERDICT: PASS

Reviewed `factory/010-save-load` at 9948902 (round 3 diff `82da671..9948902`), from `factory/worktrees/010-save-load/`. Every command in the spec's `## Checks` passes, and the round-2 blocker (the stated worst-case load memory) is fixed: the real worst case is now measured, gated and documented.

## Check results

| Command | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | pass (141 lib, store, store_limits, store_properties, project, 207 doctests; 1 ignored `regenerate_golden_fixtures`, by design) |
| `cargo test --locked` | pass |
| `cargo test --benches` | pass |
| `cargo bench --bench store_memory` (release) | pass, all 15 gates; numbers reproduce build.md |
| `cargo tree --depth 1 -e normal` | exactly serde, serde_json, thiserror, uuid |
| `scripts/ci.sh crates/aegis-model` | pass |
| `scripts/deny.sh licenses bans sources` | pass (aegis-model and aegis-model/fuzz) |
| AC16 awk/grep | prints nothing |
| AC17 neutrality grep | prints nothing |
| ADR exists, `serde_json` and `MAX_DAMAGE_LINKS` greps | pass |
| `rustup run 1.85 cargo check --locked` | pass |

## Round-2 findings

1. Worst-case memory: fixed.
   - `benches/store_memory.rs` has six new single-kind cases (assets, damage scenarios, threats, each over the built-in methodology and the max-vocabulary `short`). Each is gated at its measured factor plus about 35%: 3.3 / 8.3, 3.7 / 4.8, 2.6 / 2.6.
   - My round-2 numbers reproduce exactly. Assets only over the max vocabulary peaks at 6.16x and keeps 5.53x. Damage scenarios only over the max vocabulary peaks at 3.53x. The built-in cases peak at 2.43x and 2.73x.
   - ADR 0009, the `Project::load` doc comment and the bench header now agree with each other and with the measurements: the factor depends on methodology size; the decoder's share is at most about 0.9x; the rest is the in-memory project (job 009's per-asset reference-count rows).
   - The "2.99x / 192 MiB / 3x holds for every valid shape" claims are gone (grep confirms). The 3x budget is restated honestly. The sparse-row follow-up is recorded with numbers, and I checked its arithmetic (5.53x minus 3.3x gives about 2.2x). Not changing job 009's `RefCounts` this round was the right call.
2. Methodology-first caveat: documented in the `load` doc, the format rules and the keys-while-decoding section of the ADR.
3. Not an issue, as noted in round 2.
4. `"version": 0` (and negative or fractional versions) now gives `UnsupportedFormat { version: None }`. Only integers above 1 carry their number and the "newer application" text. The unit test asserts that version 0's message never contains "newer". The `error.rs` and `wire.rs` docs match the code.

## Findings

No bugs, panics in library code, or standards violations (D1, D2, D7, P4, A8, A9, R1) found in the round-3 diff. Only two minor points, neither blocking:

1. `docs/adr/0009-save-load-format.md` (Header checks bullet, "when the header comes first, as `save` writes it). Another methodology version: accepted ...") and the line that begins "2.2x (built-in: the row is 24 bytes, 0.3x). It touches ..." — the lines run past the surrounding wrap width after the edit. Cosmetic. Re-wrap when the ADR is next touched.
2. `benches/store_memory.rs` `single_kind` — assets carry the properties of the first threat category, not zero properties. A valid asset with `"properties":[]` is a few bytes smaller, so slightly denser, and would peak slightly above 6.16x (my estimate is under 7x). The 8.3x gate still covers it. Say "one property set of the first category" in the bench doc if you want the claim "as small as a valid one can be" to be exact.
