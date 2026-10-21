VERDICT: CHANGES

Reviewed `factory/010-save-load` at 82da671 (diff `master...factory/010-save-load`, plus round 2 as `94607f4..82da671`), from `factory/worktrees/010-save-load/`.
Every command in the spec's `## Checks` passes, and round 2 fixes the round-1 blocker in the code: keys are now resolved while decoding. What stays open is the documented worst case. I measured two valid shapes that exceed the ADR's "worst case 2.99x, at most 3x for every valid shape" claim and exceed the `minimal_entities` gate of 4.0x. The gate does not cover them.

## Check results

| Command | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | pass (141 lib, store, store_limits, store_properties, project, 207 doctests; 1 ignored `regenerate_golden_fixtures`, by design) |
| `cargo test --locked` | pass |
| `cargo test --benches` | pass (rc 0, includes the store_memory gates in debug) |
| `cargo bench --bench store_memory` (release) | pass, all 9 gates; numbers match build.md |
| `cargo tree --depth 1 -e normal` | exactly serde, serde_json, thiserror, uuid |
| `scripts/ci.sh crates/aegis-model` | pass |
| `scripts/deny.sh licenses bans sources` | pass (aegis-model and aegis-model/fuzz) |
| AC16 awk/grep | prints nothing |
| AC17 neutrality grep | prints nothing |
| ADR exists, `serde_json` and `MAX_DAMAGE_LINKS` greps | pass |
| `rustup run 1.85 cargo check --locked` | pass |

## Round-1 items

- Finding 1 (load memory): fixed in the code.
  - The leaf types (`PropertySetKeys`, `Impacts`, `ThreatCategoryKey`, `ImpactCategoryKey`, `CatalogRefText`) look up each key on the parser's `&str` and keep only indices.
  - A hostile unknown-key file now costs the 64 KiB read buffer: 0.02x, reproduced.
  - The scoped thread-local context is sound.
    - It is installed by an RAII guard and used only through `try_borrow`.
    - A missing context becomes a fault, never a panic.
    - The `FAULT` cell is cleared before each decode and taken once after it.
    - The visitors only see `visit_str`, and `visit_borrowed_str` and `visit_string` forward to it.
    - Internally tagged ops go through serde's `Content`, which still resolves keys in the leaf `Deserialize`.
  - Streaming through a 64 KiB `BufReader` plus `CappedReader` replaces `read_to_end`. The cap is exact and an I/O error maps to `Io`.
- Finding 2 (doc line): fixed. Finding 3 (whitespace leniency documented): done in the `load` doc and the ADR. Finding 4 (`violates` reasoning in the ADR): done.
- `fuzz/run.sh` validates SECONDS and the target names before building: done.
- `UnsupportedFormat { version: Option<u64> }` is fine. The message carries only a number.

## Findings

1. `docs/adr/0009-save-load-format.md:367-376`, `crates/aegis-model/src/store/snapshot.rs` (`Project::load` doc, "at most 2.99x for any shape measured ... at most about 192 MiB at the 64 MiB cap"), `benches/store_memory.rs` (`minimal_entities`, gate 4.0) — the stated worst case is wrong, and the gate does not cover the real worst case. This is the same class of problem as the round-1 blocker: a worst-case number that is stated but not measured on the densest shape.
   - **What the bench measures.** `minimal_entities` mixes assets, damage scenarios and threats at about one third of the bytes each. The densest shape is a file of only one entity kind. The bench also never combines the maximum-size methodology (64 properties, 32 impact categories) with a single-kind file.
   - **My measurement.** I used a scratch copy of the crate (`git archive` into the scratchpad) with two extra cases added to `store_memory.rs`, 4 MiB each, same dhat method:

     | Shape | Methodology | Peak | Kept (the project) |
     |---|---|---|---|
     | assets only: `{"id":..,"name":"a","properties":["p0"]}`, 54,472 assets | `short` (64 properties, as in the bench) | **6.16x** | **5.53x** |
     | damage scenarios only: `{"id":..,"name":"a","impacts":{}}`, 59,919 damages | `short` (32 categories) | **3.53x** | 2.65x |
     | assets only | built-in (6 properties) | 2.46x | 1.84x |
     | damage scenarios only | built-in | 2.73x | 1.85x |

   - **Where the cost comes from.** It is the loaded project, not the decoder: about 475 bytes kept per asset when the methodology has 64 properties (the per-asset-slot reference-count row, stride x 4 bytes, from job 009), and similar per-category rows for damages. Decoding adds under 1x on top. The 6.16x figure is therefore about 394 MiB at the 64 MiB cap, not 192 MiB.
   - **False claims that follow.**
     - "Worst case 2.99x" and "about 192 MiB at the cap".
     - "The server budget 'memory per open project <= 3x its serialized size' holds for every valid shape (kept at most 2.59x)." It does not for a 64-property methodology, which D-level limits allow.
   - **Fix.** Either of these.
     1. Add single-kind shapes to `store_memory.rs` (assets only and damages only, over the max-vocabulary `short` methodology; threats only if it is denser), gate each at its measured factor plus headroom, and correct the ADR table, the `load` doc comment, and the bench header. State plainly that the factor depends on the methodology's size (it is project size, a job 009 property), and that the 3x server budget holds only for methodologies of the built-in size, or restate the budget claim.
     2. Reduce the per-asset cost in the model (for example size the reference-count rows lazily). This is a job 009 design change and is out of scope here, so (1) is enough.
   - Verdict impact: this is what keeps the round at CHANGES. It is a documentation and gate accuracy item, not a bug.

2. `crates/aegis-model/src/store/wire.rs` (`SnapshotWire` field order) and the docs ("The methodology key is checked as soon as it is read, so `MethodologyMismatch` still wins over `UnknownKey`") — this holds only when `methodology` comes before the entity lists in the JSON. `save` and the fixtures always write it first, but the decoder accepts any field order. A hand-edited snapshot with `assets` before `methodology` and a different methodology gives `UnknownKey` first. Harmless, since both are errors with no leak. Say "when the header comes first, as `save` writes it" in the ADR or doc. Low.

3. `crates/aegis-model/src/store/wire.rs` `CatalogRefText::deserialize` — `m.catalog_scheme_key(raw.scheme.0).unwrap_or("")` allocates an empty `Arc<str>` in an impossible branch. Fine, no change needed. Noted only because it is the one place where a lookup failure is turned into an empty string.

No bugs, panics in library code, or standards violations (D1, D2, D7, P4, A8, A9, R1) found in the round-2 diff. Big-O is documented on `load`, the leaf types, and the helpers. The tests pass unchanged apart from the `UnsupportedFormat` patterns, and the error-order changes in build.md match the code.

Fix finding 1, optionally 2, then this passes.
