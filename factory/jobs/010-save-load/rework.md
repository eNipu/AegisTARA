## Answers

The owner answered the spec's open questions on 2026-10-21:

1. **Methodology version on load:** confirmed. The methodology key must match, and any version of it is accepted. Files hold stable keys, so a key the loaded version no longer has fails with a typed error and is never silently reinterpreted.
2. **Tamper evidence:** confirmed. No hash chain in this job. v1 records carry contiguous `seq` numbers and the format has a version field, so a chain can come in format v2 with no new dependency now.
3. **Format:** confirmed. JSON through `serde` and `serde_json`: a snapshot (`aegis-project` v1) and a JSON Lines op log (`aegis-oplog` v1), frozen by golden fixture files.

All other decisions stay as the spec settled them.

## Owner change during build (2026-10-21): allow cargo-fuzz, update D5

The owner's words: "If they cargo-fuzz: its libfuzzer-sys are dev dependency then its okay to use them. Update policy."

This overrides the spec's "No `cargo-fuzz`" decision. The owner has approved this change to `docs/decisions.md` D5 explicitly, so it is not an escalation trigger.

1. **Policy (D5 in `docs/decisions.md`):** add a rule for non-shipped tooling. Crates used only to build, test, fuzz or benchmark (dev-dependencies and fuzz crates, never linked into a shipped binary) may also use **NCSA** (the LLVM license, permissive). Each such crate needs a per-crate exception in `deny.toml` that names the crate and version and points to D5. Everything that ships keeps the existing allow list unchanged. Keep the wording short and in the style of the other D entries.
2. **Enforcement (`deny.toml`):** add `libfuzzer-sys` (exact version) as a per-crate `exceptions` entry allowing NCSA, with a comment naming D5. Do **not** add NCSA to the global allow list. Make sure the license check actually covers the fuzz crate (for example by extending `scripts/deny.sh` or running it on the fuzz manifest in CI), so a shipped crate can never pick up NCSA unnoticed. Update ADR 0003 or ADR 0009 to record the exception.
3. **Fuzz targets:** add a `cargo-fuzz` crate for `aegis-model` (for example `crates/aegis-model/fuzz/`, kept out of the `crates/*/` production glob) with at least two targets: the project snapshot decoder and the op-log decoder (header and record lines). Seed the corpora from the golden v1 fixtures. Targets must never panic on any input. Keep the existing proptest hostile-input tests too.
4. **Running it:** cargo-fuzz needs a nightly toolchain. Run each target locally for a short bounded time (for example `-max_total_time=60`) if nightly is available, and record the result in build.md. Do not add nightly to CI in this job; say how to run the fuzzers in the ADR and README.

## Rework 1

Round 1: UI and UX passed. Security and code asked for CHANGES, both for the same blocker: **worst-case load memory**. Every decoded vocabulary key is its own `Arc<str>` heap allocation, and keys are checked against the methodology only after the whole file is decoded. A hostile snapshot peaked at about 790 MB at the 64 MiB cap (security: about 11-12x the file size; code: about 10x). The spec's "no more memory than its JSON text" and ADR 0009's 3.65x are wrong for that shape, and the `store_memory` gate only measures the benign `typical` project.

The owner prefers the right design over the quick one (see job 009's guiding principle), so make the real fix, not only the documentation:

1. **Resolve keys while decoding.** Decode snapshot and op-log records with a `DeserializeSeed` (or an equivalent) that holds `&Methodology` and turns each vocabulary key into its typed index as it is read. Then no per-key `String`/`Arc<str>` is kept, and an unknown key fails at once with the same typed `StoreError::UnknownKey` (key capped at 64 bytes, escaped, as now). Entity names still need owned strings, under the existing byte caps.
2. **Gate the worst case.** Extend `benches/store_memory.rs` with hostile shapes that each fill the snapshot cap pattern at a size you can run in CI. Cover at least: many distinct short keys per entity (impacts and properties), and many entities with maximum-length names. Fail if the peak heap factor exceeds a bound you set and justify. Keep the `typical` gate as well.
3. **Correct the docs.** Record the measured typical and worst-case factors in ADR 0009 and in the `Project::load` doc comment. Fix any wording that claims "no more memory than its JSON text" where it is not true. Note that the snapshot accepts JSON whitespace and a missing final `\n` (code-review minor point).
4. **Minor, from the reviews:**
   - Read the snapshot into a buffer sized from the known length or the cap, not by doubling growth (security minor point: about 128 MiB reserved near the cap).
   - Fix the over-wide doc line in `src/threat.rs`.
   - Record in ADR 0009 why the AC4 `reordered` helper reverses `violates` lists instead of prepending to them.
   - `fuzz/run.sh`: validate the target name and `SECONDS` before building (UX point 4).
5. **Optional, UX points 1-3:** split `UnsupportedFormat` into "not an AegisTARA file" and "newer format version", and point the docs to `OpLogReader::line()` for error locations, as long as errors still never quote entity names.

Keep every other round-1 choice. Re-run every check in the spec plus the new memory gates, and append `## Round 2` to build.md with before and after numbers.

## Rework 2

Round 2: security, UX and UI passed. Code asked for CHANGES. The round-1 decoder blocker is fixed in the code, and every check passes. The remaining blocker is that the stated worst case is wrong and is not gated.

- `docs/adr/0009-save-load-format.md` (worst-case table and 3x budget claim), the `Project::load` doc comment and the `store_memory` bench header say the worst case is 2.99x (about 192 MiB at the cap), and that the project keeps at most 2.59x.
- The code reviewer measured single-kind files over the largest allowed methodology, where the cost is the **loaded project itself** (mostly job 009's per-asset reference-count rows), not the decoder:

  | Shape | Peak | Kept |
  |---|---|---|
  | assets only, 1 property each, 64-property methodology | 6.16x | 5.53x |
  | damage scenarios only, 32-category methodology | 3.53x | 2.65x |
  | assets only, built-in methodology | 2.46x | 1.84x |
  | damage scenarios only, built-in methodology | 2.73x | 1.85x |

Do this:

1. **Gate the real worst case.** Add single-kind shapes to `benches/store_memory.rs`: assets only, damage scenarios only, and threats only if denser. Run each over both the built-in methodology and a maximum-vocabulary methodology. Gate each one at its measured factor plus headroom, as for the existing nine. Keep the mixed `minimal_entities` case.
2. **Correct the docs** (ADR 0009, the `Project::load` doc, the bench header). Report worst-case peak and kept factors per methodology size. Restate the 3x budget claim honestly: say the factor depends on methodology size, give the built-in methodology's worst case and the maximum-vocabulary worst case, and say which part is the decoder and which is the in-memory project.
3. **Optional, only if small and safe:** if the per-asset reference-count rows can be made sparse or narrower without changing job 009's public API or behaviour, do it and show the before and after numbers. Otherwise record it in ADR 0009 as a follow-up for the in-memory model, with the numbers. Do not risk this round on it.
4. **Minor:**
   - Document that "methodology key checked first" holds when `methodology` comes before the entity lists in the JSON, as `save` writes it.
   - UX: `"version": 0` must not produce the "a newer application may read this file" message. Treat 0 (and any version below 1) as "not a supported format", not "newer".

Keep every other round-2 choice. Re-run every check and all memory gates, and append `## Round 3` to build.md.

## Owner decision on the escalation (2026-10-21)

The approver escalated on two points (see decision.md). The owner answered directly:

1. **D5 / NCSA:** approved as written: the D5 "Non-shipped tooling" text, the `deny.toml` exception for `libfuzzer-sys@0.4.13`, the ban that limits it to `aegis-model-fuzz`, and the fuzz-crate check in `scripts/deny.sh`.
2. **3x server-memory budget:** the breach is accepted for now (up to 5.53x kept and 6.16x peak, for assets-only files under a 64-property methodology). The fix, sparse per-asset reference counts, is the next factory job. The CI gate at 8.3x stops it from getting worse in the meantime.

Merged by `/factory approve 010-save-load`.
