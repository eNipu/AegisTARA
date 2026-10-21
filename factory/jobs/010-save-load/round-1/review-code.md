VERDICT: CHANGES

Reviewed `factory/010-save-load` at 94607f4 (diff `master...factory/010-save-load`), from `factory/worktrees/010-save-load/`.
Every command in the spec's `## Checks` passes. The one finding that needs work is the worst-case load memory (finding 1). It is a missing measurement and a false premise in the spec and ADR, not a failing check.

## Check results

| Command | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | pass (138 lib, plus store, store_limits, store_properties, project, 207 doctests; 1 ignored: `regenerate_golden_fixtures`, by design) |
| `cargo test --locked` | pass |
| `cargo test --benches` | pass (rc 0) |
| `cargo tree --depth 1 -e normal` | exactly serde, serde_json, thiserror, uuid (AC1) |
| `scripts/ci.sh crates/aegis-model` | pass (fmt, clippy, test) |
| `scripts/deny.sh licenses bans sources` | pass (aegis-model and aegis-model/fuzz) |
| AC16 awk/grep | prints nothing |
| AC17 neutrality grep | prints nothing |
| `test -f docs/adr/0009-save-load-format.md`, `serde_json` and `MAX_DAMAGE_LINKS` greps | pass |
| `rustup run 1.85 cargo check --locked` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` | pass |
| fuzz crate `cargo fmt --check`, `cargo clippy --locked -- -D warnings` | pass |
| `fuzz/run.sh 10` (extra) | pass: snapshot 1.37 M runs, op_log 0.40 M runs, no crash |

## Acceptance criteria

AC1-AC19 are met and tested. I read the code behind each one, not just the test names. The rework items are also done:
- D5 wording in `docs/decisions.md`.
- Per-crate NCSA exception plus a `[bans] deny` entry with `wrappers`.
- `scripts/deny.sh` also covers the fuzz manifest.
- Two fuzz targets seeded from the golden files.
- ADR 0003 and ADR 0009 updated.

I checked these in the source:
- Caps fire inside the serde visitors: `BoundedStr`, `BoundedVec`, `KeySet`, `KeyMap`.
- A bounded line reader stops at 64 KiB + 1.
- The snapshot is read through `take(MAX + 1)`.
- Fault side channel: it is cleared before each decode and `take`n after, and the serde error text is fixed. It is single-threaded and has no `.await` inside.
- Errors carry only keys, which are capped at 64 bytes and printed with `{:?}`.
- IDs must be canonical lowercase hyphenated text before `FromStr`.
- The writer refuses anything the reader would refuse, so every written record reads back.
- `load` goes only through `apply`.
- `TooManyDamageLinks` is enforced in `validate` for `AddThreatScenario` and `LinkDamage`, and in the invariant check.
- Big-O is documented on every public function that takes project-sized input.
- No O(n) per op. The only quadratic spots are the insertion sorts in `KeySet` and `KeyMap`, which are capped at 64 and 32 items.

## Findings

1. `crates/aegis-model/src/store/wire.rs:211-275` (`KeySet`, `KeyMap`) and `docs/adr/0009-save-load-format.md:290-296` — the spec's memory premise is false, and the ADR measures only the typical shape.
   - **Premise.** The spec says "each wire entity takes no more memory than its JSON text". Each decoded key is its own `Arc<str>`: a 16-byte `Arc` header, rounded up by the allocator, plus a 16-byte fat pointer. A 2-char key (5 bytes of JSON, `"aa",`) costs about 48 bytes or more on the heap.
   - **Measurement.**
     - I built a hostile 30 MB snapshot: assets with 64 distinct 2-char property keys each, all unknown to the methodology.
     - `Project::load` returned `UnknownKey` after decoding it.
     - Peak RSS was 308 MB, about 9-10x the file. That includes the 30 MB `String` the test itself holds.
     - At the 64 MiB cap that is about 650 MB peak for one `load`.
   - **Why it matters.** The typical-shape factor in the ADR is 3.65x, with a 5x bench guard on the typical workload only. Memory is still O(B), as the doc comment says. But the "memory stays bounded" claim in the Goal is only useful if the constant is stated. A server decoding several uploads at once makes this an amplification factor.
   - **Fix, either of these.** (a) State the measured worst case in the ADR (shape, bytes, factor, and the resulting peak at 64 MiB) and in the `Project::load` doc, and drop the "no more than its JSON text" sentence. (b) Reduce it: resolve keys during decoding with `DeserializeSeed`, which the ADR already names, or store keys as byte ranges into the input. Option (a) is enough if the owner accepts about 10x. Say so in build.md.

2. `crates/aegis-model/src/threat.rs` (doc of `ThreatScenario`, "scenario's public ID, unique, and at most [`MAX_DAMAGE_LINKS`] long. Neither order depends on edit history.") — a doc line of about 110 columns that rustfmt does not wrap — reflow it to the file's 80-column wrap. Cosmetic.

3. `crates/aegis-model/src/store/snapshot.rs:262-264` — a snapshot with whitespace between tokens, or without the final `\n`, loads (serde_json `end()`). The spec's strict-decoding rule lists "trailing data after the value" as an error, and says nothing for or against whitespace. build.md states this. It is harmless: the re-save is canonical, and the proptest `Ok`-then-reload check holds. Either add one line to the ADR format rules saying a snapshot is lenient about JSON whitespace (the op log is strict), or reject it. No action needed beyond the one-line doc.

4. `crates/aegis-model/tests/common/mod.rs` (`reordered`) — the spec's AC4 says "prepends one unused item to each of those lists" including each `violates` list. The builder prepends to the four vocabularies and the scheme list, and only reverses `violates`. Their reason is sound: adding a property to `violates` changes what a category means and makes valid threats fail to load. It is documented in build.md. Accepted; please repeat the reason in the ADR's AC4 paragraph so the deviation is traceable. Low.

No bugs, panics in library code, or standards violations (D1, D2, D7, P4, A8, A9, R1) found in the diff. The one avoidable clone is the `Arc::clone` into the `seen` vector in `KeyMap`, which is bounded at 32 entries and not worth changing.

Fix finding 1 (document or reduce), optionally 2-4, then this passes.
