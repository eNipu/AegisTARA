## Round 1
Commits:
- 3ad7c60 Add versioned JSON snapshot and op log with stable keys and decode-time caps
- 94607f4 Add fuzz targets for the decoders and allow NCSA for non-shipped tooling

Files:
- crates/aegis-model/Cargo.toml, Cargo.lock (`serde` derive, `serde_json`; benches `store`, `store_memory`)
- crates/aegis-model/src/store/mod.rs (format docs, cap constants, `Limit`, `VocabKind`)
- crates/aegis-model/src/store/wire.rs (fault side channel, `BoundedStr`/`BoundedVec`/`KeySet`/`KeyMap`/`WireId`/`Format`/`FormatVersion`, owned `*Wire` types, key resolution, `KeyOrder`, borrowed `*View` serializers)
- crates/aegis-model/src/store/snapshot.rs (`Project::save`, `Project::load`)
- crates/aegis-model/src/store/op_log.rs (`LoggedOp`, `OpLogWriter`, `OpLogReader`, op encode/decode, bounded line reader)
- crates/aegis-model/src/error.rs (`StoreError`; `ModelError::TooManyDamageLinks`)
- crates/aegis-model/src/threat.rs, src/project.rs, src/op.rs (`MAX_DAMAGE_LINKS = 256`, checked in `validate` for `AddThreatScenario` and `LinkDamage`; invariant check; docs)
- crates/aegis-model/src/lib.rs (`mod store`, re-exports, "Saving and loading" section with doctest)
- crates/aegis-model/tests/store.rs, tests/store_limits.rs, tests/store_properties.rs; tests/project.rs (+`too_many_damage_links`); tests/common/mod.rs (+`reordered`, `accepted`, `save`, `write_log`, `read_log`, `describe`)
- crates/aegis-model/tests/fixtures/{project-v1.json, oplog-v1.jsonl, .gitattributes}
- crates/aegis-model/benches/store.rs (divan), benches/store_memory.rs (dhat), benches/store_kit/mod.rs (shared typical workload)
- crates/aegis-model/fuzz/ (cargo-fuzz crate: `snapshot` and `op_log` targets, seeds from the golden files, `run.sh`, own Cargo.lock and workspace)
- deny.toml (per-crate NCSA exception for `libfuzzer-sys@0.4.13`, `[bans] deny` with `wrappers = ["aegis-model-fuzz"]`), scripts/deny.sh (also checks `crates/*/fuzz/Cargo.toml`), tools/deny-fixtures (`bad-ncsa` case)
- docs/adr/0009-save-load-format.md (new), docs/adr/0003-cargo-deny-policy.md, docs/decisions.md (D5), README.md

Acceptance criteria:
- [x] AC1 — `cargo tree --depth 1 -e normal` → exactly serde, serde_json, thiserror, uuid. `cargo test --locked` → pass. `scripts/deny.sh licenses bans sources` → ok (both aegis-model and aegis-model/fuzz); full `scripts/deny.sh` incl. advisories → ok.
- [x] AC2 — `cargo test --test store_properties snapshot_round_trip` → 2 passed (256 cases each, built-in and `second()`; load == p and re-save bytes equal after every accepted op).
- [x] AC3 — `cargo test --test store save_is_deterministic` → passed (two op orders with a removed asset and a removed damage scenario, ratings and refs added in different orders; identical bytes; saving twice and load-then-save identical).
- [x] AC4 — `cargo test --test store reordered_methodology_keeps_meaning` → passed (snapshot: `describe()` per entity equal, re-save identical; log: reads under the reordered methodology, ops differ by index, re-encode identical bytes; indices asserted to differ).
- [x] AC5 — `cargo test --test store only_keys_on_disk` → passed (snapshot + log for both methodologies; numbers only under `version` and `seq`).
- [x] AC6 — `cargo test --test store golden_v1` → 3 passed (snapshot loads, re-saves byte for byte, equals the in-code golden build; log reads, has seq 1..24, covers all 16 op variants, replays to the snapshot's project, re-encodes byte for byte; both LF-only and ending in `\n`).
- [x] AC7 — `cargo test --test store_properties op_round_trip` → 2 passed (256 cases each; every accepted op and its inverse).
- [x] AC8 — `cargo test --test store_properties replay` → 2 passed (256 cases each; (a) full replay == final; (b) snapshot after a random step + log tail == final).
- [x] AC9 — `cargo test --test store op_log_sequencing` → 4 passed (1,2,3; resume continues at 3 and resume alone writes nothing; gap/repeat/step back → `Sequence { expected: 3, found }` after two Ok records, then fused; a log starting at 0; five unencodable ops (foreign property/threat category/impact category/catalog ref, repeated impact category) → `UnencodableOp`, bytes identical, writer still usable). The writer's public API is `create`, `resume`, `append`, `last_seq`, `into_inner`: nothing rewrites or truncates.
- [x] AC10 — `cargo test --test store framing_and_header_errors` → 3 passed (truncated last record after an Ok record; blank line, `\r\n`, trailing data; empty input; header without `\n`; wrong format, `version: 2`, snapshot fed to the reader and log fed to `load`; methodology mismatch; unknown key of all 5 `VocabKind`s in log and snapshot; v4/malformed/uppercase IDs; bad ref text; duplicate in property set and impact map, log and snapshot; unknown field; missing asset → `Model(UnknownAsset)`; different methodology version accepted for log and snapshot, `methodology_version()` reports it).
- [x] AC11 — `cargo test --test store_limits` → 11 passed; each cap gives `LimitExceeded` with the exact `Limit` and `max`, in log records and in snapshots (17 refs, 257 damages, 33 impacts, 65 properties, 1,025-byte name, 65-byte key incl. header key, 65-byte ref text, 257-byte version, 64 KiB + 1 line incl. unterminated and header-only, 64 MiB + 1 snapshot from `io::repeat(b' ').take(..)`).
- [x] AC12 — `cargo test --test store_limits limits_admit_every_valid_project` → passed (methodology with 64-byte keys, 32 categories x 16 levels, 64 properties, 64 threat categories, 16 schemes with 39-byte refs, 256-byte version; threat with 16 refs and 256 links, names of 256 four-byte chars, every category rated; saves, loads, re-saves identically, logs and reads back; longest record 13,261 bytes). `cargo test --test project too_many_damage_links` → passed (257 on add and the 257th `LinkDamage` → `TooManyDamageLinks`, validate == apply, project unchanged; unlink makes room). `ops_properties` and lib `project_invariants` still pass.
- [x] AC13 — `cargo test --test store_properties hostile_input_never_panics` → 2 passed (512 cases: random bytes, and golden files with 1–8 flips/inserts/deletes/truncations; both decoders, both methodologies; Ok loads re-save and reload equal; reader ends).
- [x] AC14 — `cargo test --test store errors_have_no_labels` → passed (sentinel in a too-long name, a name before an unknown key, a name before a syntax error, an op tag, a field name, an ID, a ref text, `seq`, the format string, a snapshot name before a missing asset, a snapshot ID; Display and Debug never contain it; stages LimitExceeded, UnknownKey, Decode, InvalidId, InvalidCatalogRef, UnsupportedFormat, Model all hit).
- [x] AC15 — `cargo test --doc` → 206 passed; `cargo clippy --all-targets -- -D warnings` → clean; `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` → clean. Every new public item (types, methods, constants incl. `MAX_DAMAGE_LINKS`) has a compiling example; save/load/append/reader state time and space.
- [x] AC16 — the awk/grep prints nothing; `#![forbid(unsafe_code)]` stays (lib.rs:171).
- [x] AC17 — the neutrality grep prints nothing (src unit tests use the second methodology).
- [x] AC18 — `cargo test --benches` → pass; `cargo bench --bench store` and `--bench store_memory` run; ADR check → pass.
- [x] AC19 — `cargo test` → 444 passed, 0 failed (existing test files only gained new tests/helpers); `cargo test --benches` → pass.

Owner change (rework.md, "allow cargo-fuzz, update D5"):
- [x] D5 in docs/decisions.md: new "Non-shipped tooling" line (NCSA allowed for build/test/fuzz/bench-only crates, per-crate exception naming D5; shipped crates unchanged).
- [x] deny.toml: `{ crate = "libfuzzer-sys@0.4.13", allow = ["NCSA"] }` (NCSA not in the global allow list) plus `[bans] deny = [{ crate = "libfuzzer-sys", wrappers = ["aegis-model-fuzz"] }]`. Verified by hand in a scratch dir: a crate depending on `libfuzzer-sys` fails `check bans` (error[banned]); the fuzz crate without the exception fails `check licenses` (error[rejected]). `scripts/deny.sh` now also checks `crates/*/fuzz/Cargo.toml` (CI's deny job runs it). `tools/deny-fixtures/run.sh` → all 9 cases match, incl. new `bad-ncsa` → fail. ADR 0003 records the first exception.
- [x] Fuzz crate `crates/aegis-model/fuzz/` (outside the `crates/*/` glob, own `[workspace]` and Cargo.lock): targets `snapshot` (load; if Ok, save+load equal) and `op_log` (read all, replay, re-encode every op and read back equal); seeds = golden v1 files. Proptest hostile tests kept.
- [x] Run: cargo-fuzz is not installed and the local nightlies (2023-10-17, 2024-05-20) predate edition 2024, so I did not install anything global. `crates/aegis-model/fuzz/run.sh` builds the same libFuzzer binaries on stable with cargo-fuzz's coverage flags (no sanitizer). 60 s per target, twice: `snapshot` 7.9 M and 7.7 M runs (cov 2,763, corpus 1,992), `op_log` 2.9 M and 3.1 M runs (cov 3,306, corpus 1,272), no crashes, no artifacts. No nightly in CI; ADR 0009 and README say how to run (stable script, or `cargo +nightly fuzz run ...`).

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (444)
- `cargo test --locked` → pass
- `cargo test --benches` → pass
- `cargo tree --depth 1 -e normal` → serde, serde_json, thiserror, uuid
- `scripts/ci.sh crates/aegis-model` → all steps passed
- `scripts/deny.sh licenses bans sources` → ok aegis-model, ok aegis-model/fuzz
- AC16 grep → prints nothing
- AC17 grep → prints nothing
- `test -f docs/adr/0009-save-load-format.md` → pass
- `rustup run 1.85 cargo check --locked` → pass
- also: `tools/deny-fixtures/run.sh` → 9/9; fuzz crate `cargo fmt --check` and `cargo clippy --locked -- -D warnings` → pass

Notes:
- **Typed errors out of serde visitors:** a thread-local `Fault` side channel (`wire::raise` / `wire::decode`), cleared before every decode and read once after; the serde error itself carries only fixed text. Faults: Limit, DuplicateEntry, InvalidId, UnsupportedFormat.
- **Format/version checked as they are read**, inside their visitors, so a v2 file or another format fails with `UnsupportedFormat` even when the rest of it would not decode as v1. The methodology key is checked right after decoding, before any key lookup.
- **IDs must be canonical text** (36 bytes, lowercase, hyphenated) before `FromStr`; uppercase/simple/braced UUID forms give `InvalidId`. Keeps files byte-canonical.
- **Writer refuses what the reader would refuse** (`UnencodableOp`, nothing written): foreign indices, a catalog ref failing its scheme's syntax, a repeated impact category, a name over 1,024 bytes, >16 refs, >256 damages, a record over 64 KiB, or `seq` overflow. The spec named only foreign indices; the rest keeps "every written record reads back" true.
- **`reordered()` interpretation (AC4):** the unused item is prepended to the four vocabularies and the scheme list, not to each `violates` list: adding a property to a category's `violates` would change what it means and make valid threats fail to load. `violates` lists are reversed only.
- **Load memory:** peak 3.65x file bytes on the typical snapshot (dhat, `benches/store_memory.rs`, fails above 5x); project kept 0.83x. Bench numbers are in ADR 0009 (save 1.07 ms, load 4.31 ms, append 10k 1.95 ms, read 10k 8.21 ms; 0.4% of the 1.0 s open budget). Measured on this Apple Silicon machine, not the reference laptop.
- **Additions beyond the spec's file table:** `benches/store_memory.rs` and `benches/store_kit/` (memory factor measurement, shared workload, following the `parents_memory` precedent); public constants `MAX_NAME_BYTES`, `MAX_KEY_BYTES`, `MAX_METHODOLOGY_VERSION_BYTES`, `MAX_CATALOG_REF_BYTES` alongside the two the spec names; `VocabKind` (needed by `StoreError::UnknownKey`); `FusedIterator` for the reader.
- **Snapshot leniency:** whitespace between tokens and a missing final `\n` are accepted in a snapshot (serde_json `end()`); the op log is strict about framing as specified.
- Read throughput of op records is about half of snapshot entities per byte because serde buffers internally tagged enums into `Content`; recorded in the ADR as an L2 lesson, not optimized.

## Round 2
Commits: 82da671 Resolve keys while decoding and stream snapshots to bound hostile load memory
Files:
- crates/aegis-model/src/store/wire.rs (decode context and resolving leaf types; `Fault` carries UnknownKey / InvalidCatalogRef / MethodologyMismatch / UnsupportedFormat(version); old `KeySet`/`KeyMap`/`resolve_*` removed; unit tests rewritten)
- crates/aegis-model/src/store/snapshot.rs (streaming `CappedReader` + 64 KiB `BufReader` instead of `read_to_end`; `load` doc with measured factors and whitespace leniency)
- crates/aegis-model/src/store/op_log.rs (`op_from_wire` takes resolved values; header methodology checked while decoding)
- crates/aegis-model/src/store/mod.rs (module doc: keys resolved while decoding, error order within a list)
- crates/aegis-model/src/error.rs (`UnsupportedFormat { version: Option<u64> }`; docs point to `OpLogReader::line()`; `UnknownKey` hint for apps)
- crates/aegis-model/src/threat.rs (doc line reflowed)
- crates/aegis-model/benches/store_memory.rs (nine shapes with gates)
- crates/aegis-model/tests/store.rs (header tests for the new `UnsupportedFormat` shape)
- crates/aegis-model/fuzz/run.sh (validates SECONDS and target names before building)
- docs/adr/0009-save-load-format.md (new "Keys: resolved while decoding" option table, rules, memory table, AC4 `violates` reasoning, whitespace leniency, bench numbers, round-2 fuzz run)

Review items:
- [x] Security 1 / code 1 (worst-case load memory, the blocker): fixed by design, not only documented.
  - Every vocabulary key is resolved to its index the moment it is read. The leaf types (`PropertySetKeys`, `Impacts`, `ThreatCategoryKey`, `ImpactCategoryKey`, `CatalogRefText`) do the lookup on the parser's `&str`, through a `DeserializeSeed`, against the methodology that `wire::decode` installs in a scoped thread-local context. An RAII guard restores the previous context, and a leaf outside `decode` fails with a fault.
  - Decoded entities now hold names and 1-byte indices only. The one key still kept as a string is a `SetImpact` level, once per op-log record, because it needs its category first.
  - An unknown key fails inside the entity that holds it. Within a set or impact map, only the first unknown key is kept (one allocation of at most 64 bytes) and it is raised when the list ends, so the list's count cap is still reported first (AC11 unchanged).
  - The methodology key is checked as soon as it is read, so `MethodologyMismatch` still wins over `UnknownKey`.
  - I chose a scoped context over threading a `DeserializeSeed` through every struct so that derive keeps the strict field handling for all 20+ shapes, including the internally tagged ops. ADR 0009 has the option table.
- [x] Rework 2 (worst-case gates): `benches/store_memory.rs` measures nine shapes of 4 MiB each (typical 1.1 MB) with dhat, restarting the profiler per case, and fails if any peak exceeds its gate. Each gate is the measured factor plus about 35% headroom; the table with "what dominates" is in the bench header and the ADR. It runs under `cargo test --benches` (CI) and passes there in debug too.
- [x] Rework 3 (docs): measured typical and worst-case factors are in ADR 0009 (Numbers) and the `Project::load` doc comment. Round 1's 3.65x/O(B) wording is gone. My docs never claimed "no more memory than its JSON text" (that sentence is in the spec only); the ADR now states the real per-shape figures. The ADR format rules and the `load` doc say the snapshot accepts JSON whitespace and a missing final `\n`, while the op log is strict.
- [x] Security minor (buffer doubling near the cap): removed instead of sized. `load` no longer reads the file into a buffer at all: it streams through a 64 KiB `BufReader` and a `CappedReader` that fails one byte past 64 MiB and flags it (→ `LimitExceeded(ProjectFileBytes)`). I/O errors from the source now come out as `StoreError::Io` (new unit test).
- [x] Code 2: over-wide doc line in `src/threat.rs` reflowed (the other long lines there are pre-existing doc examples).
- [x] Code 4: ADR 0009 now explains why `reordered` reverses `violates` lists instead of prepending to them.
- [x] UX 4: `fuzz/run.sh` validates SECONDS (`abc` → exit 2 with usage) and each target (`snapshots` → "unknown target 'snapshots'; expected snapshot or op_log", exit 2) before building.
- [x] UX 1 (optional, done): `UnsupportedFormat { version: Option<u64> }`. `Some(n)` means the file's format version is n (likely a newer file: "a newer application may read it"); `None` means the wrong kind of file. It is still the `UnsupportedFormat` variant, so AC10 holds. The number is decoded data but numeric, not text.
- [x] UX 2–3 (optional, docs only): `StoreError` docs say to use `OpLogReader::line()` for the failing record's line, and that snapshot errors other than `Decode` carry no position. `UnknownKey` docs give the likely causes and the hint an app should show. No entity names were added to errors.

Memory, peak heap during `Project::load` as a factor of file size (`cargo bench --bench store_memory`, release, dhat). The "before" column is the round-1 code measured with the same bench in a temporary worktree:

| Shape | Before (round 1) | After | Kept (project) | Gate |
|---|---|---|---|---|
| typical (1,097,749 B) | 3.65x | 1.03x | 0.83x | 1.5x |
| short property keys (64/asset) | 8.01x | 1.47x | 1.32x | 2.0x |
| short impact keys (32/damage) | 9.04x | 0.76x | 0.54x | 1.5x |
| unknown property keys | 8.01x | 0.02x | 0 | 0.1x |
| unknown impact keys | 9.04x | 0.02x | 0 | 0.1x |
| 62 one-byte unknown keys/asset (reviewers' shape) | 10.14x | 0.02x | 0 | 0.1x |
| long names (1,024 B) | 3.08x | 1.31x | 1.28x | 2.0x |
| one huge string (fails name cap) | 2.00x | 1.02x | 0 | 2.5x |
| minimal valid entities | 3.49x | 2.99x | 2.59x | 4.0x |

The worst case at the 64 MiB cap is now about 192 MiB (minimal entities, mostly the loaded project itself), down from about 650 MB. Hostile unknown-key files now cost about 64 KiB.

Time (`cargo bench --bench store`, medians, round 1 → round 2):
- save 1.07 → 1.05 ms
- load 4.31 → 5.08 ms (streaming parse costs about 18%; still 0.5% of the 1.0 s open budget)
- append_10k 1.95 → 1.93 ms
- read_10k 8.21 → 7.28 ms
- replay_log 9.87 → 8.92 ms

Fuzzing after the decoder change (`fuzz/run.sh 60`): snapshot 6.83 M runs, op_log 3.02 M runs, no crashes, no artifacts.

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (447)
- `cargo test --locked` → pass
- `cargo test --benches` → pass (includes the store_memory gates)
- `cargo tree --depth 1 -e normal` → serde, serde_json, thiserror, uuid
- `scripts/ci.sh crates/aegis-model` → all steps passed
- `scripts/deny.sh licenses bans sources` → ok aegis-model, ok aegis-model/fuzz
- AC16 grep → prints nothing
- AC17 grep → prints nothing
- `test -f docs/adr/0009-save-load-format.md` (+ greps) → pass
- `rustup run 1.85 cargo check --locked` → pass
- also: `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` → clean; `tools/deny-fixtures/run.sh` → 9/9; fuzz crate fmt + clippy → pass; `cargo bench --bench store_memory` → all 9 gates pass

Notes:
- **Error order changed in two places.**
  - In an op-log record, decoding (including key lookups) now comes before the `seq` check. A record with both a bad key and a wrong `seq` reports `UnknownKey`, not `Sequence`.
  - Within one list, a cap or a repeat is reported before an unknown key. A repeat of a known key can be reported before an earlier unknown key in the same list.
  - All AC tests are unchanged except the `UnsupportedFormat` patterns.
- **The scoped context replaces an explicit seed.** I added no `unsafe` code, no `unwrap`, and no `panic`. `RefCell` access goes through `try_borrow`/`try_borrow_mut`, so a failure means "no context", which surfaces as a fault, never a panic.
- **The 64 MiB cap test** now streams 64 MiB of spaces through the parser and takes about 1.3 s in debug (`store_limits`).

## Round 3
Commits: 9948902 "Gate single-kind worst-case load memory and correct the documented factors"
Files:
- crates/aegis-model/benches/store_memory.rs (`single_kind()` builder; six new gated cases; header table with the decoder vs project split)
- crates/aegis-model/src/store/wire.rs (`FormatVersion`: only versions above 1 carry their number; unit test that version 0 never says "newer")
- crates/aegis-model/src/error.rs (`UnsupportedFormat` docs)
- crates/aegis-model/src/store/snapshot.rs (`Project::load` doc: worst case per methodology size; methodology-first caveat)
- docs/adr/0009-save-load-format.md (single-kind table, honest 3x budget statement, methodology-first caveat, version-0 rule, follow-up for the reference-count rows)

Review items (Rework 2):
- [x] 1. **Real worst case gated.** `store_memory` now has six single-kind cases: assets only, damage scenarios only, and threats only, each over the built-in methodology and over `short` (64 properties, 32 impact categories). Each entity is as small as a valid one can be. Each case is gated at its measured factor plus about 35% headroom, the same rule as the other nine, and `minimal_entities` is kept. The reviewer's numbers reproduce exactly.

  | Shape | Methodology | Peak | Kept | Decoder (peak - kept) | Gate |
  |---|---|---|---|---|---|
  | assets only | built-in | 2.43x | 1.81x | 0.62x | 3.3x |
  | assets only | max vocabulary | 6.16x | 5.53x | 0.63x | 8.3x |
  | damages only | built-in | 2.73x | 1.85x | 0.88x | 3.7x |
  | damages only | max vocabulary | 3.53x | 2.65x | 0.88x | 4.8x |
  | threats only | built-in | 1.91x | 1.10x | 0.81x | 2.6x |
  | threats only | max vocabulary | 1.93x | 1.12x | 0.81x | 2.6x |

  Threats-only is less dense than the other two, because each threat needs an asset ID, a category and two lists in its JSON.
- [x] 2. **Docs corrected** in ADR 0009, the `Project::load` doc and the bench header.
  - The worst case now depends on the methodology's size. Built-in: 2.73x peak (damage scenarios only), about 175 MiB at the 64 MiB cap, kept at most 1.85x. Max vocabulary: 6.16x peak (assets only), about 394 MiB at the cap, kept 5.53x.
  - The decoder's share is at most about 0.9x on top of the project. The rest is the in-memory project, mostly job 009's per-asset reference-count row: 4 bytes per methodology property, 256 bytes per asset at 64 properties against a 77-byte minimal asset in the file.
  - The 3x server budget is restated. It holds for built-in-size methodologies (kept at most 1.85x) and for realistic shapes (typical 0.83x). It does not hold for minimal-asset-only files over a methodology with many properties. The "2.99x / 192 MiB" claims are removed.
- [x] 3. **Optional sparse reference counts: not done.** It changes job 009's `RefCounts` invariants and every op that updates them, which is too risky for the last round. ADR 0009 records it as a follow-up with numbers: a sparse layout would cut assets-only over the max vocabulary from 5.53x kept to about 2.2x; the built-in row is only 24 bytes (0.3x).
- [x] 4a. The ADR (format rules and the round-2 keys section) and the `load` doc now say that "methodology checked first" holds when `methodology` comes before the entity lists, as `save` writes it. With the lists first, a foreign file may fail with `UnknownKey` instead. It still fails before any project is built, and no key is reinterpreted.
- [x] 4b. `"version": 0`, negative or fractional versions now give `UnsupportedFormat { version: None }`, the "not a supported AegisTARA file" message. Only integers above 1 carry their number and the "a newer application may read it" text. Unit test `format_and_version_checks` asserts that version 0's message never contains "newer".

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (447)
- `cargo test --locked` → pass
- `cargo test --benches` → pass (all 15 store_memory gates run here too)
- `cargo tree --depth 1 -e normal` → serde, serde_json, thiserror, uuid
- `scripts/ci.sh crates/aegis-model` → all steps passed
- `scripts/deny.sh licenses bans sources` → ok aegis-model, ok aegis-model/fuzz
- AC16 grep → prints nothing
- AC17 grep → prints nothing
- `test -f docs/adr/0009-save-load-format.md` (+ greps) → pass
- `rustup run 1.85 cargo check --locked` → pass
- also: `cargo bench --bench store_memory` (release) → all 15 gates pass (numbers above; the other nine are unchanged from round 2); `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` → clean; `tools/deny-fixtures/run.sh` → 9/9; fuzz crate fmt + clippy → pass

Notes:
- **No decoder or format behaviour changed this round** except the version-0 classification. The fuzz targets and golden files are unchanged, so I did not re-run the fuzzers.
