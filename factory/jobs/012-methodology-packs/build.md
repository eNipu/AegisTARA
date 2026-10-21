## Round 1
Commits: 928bbe5 Load methodologies from pack files and ship ISO/SAE 21434 + R155 as a pack
Files:
- new: `crates/aegis-model/src/methodology/pack.rs`, `crates/aegis-model/packs/iso21434-r155.json`, `crates/aegis-model/packs/.gitattributes`, `crates/aegis-model/tests/builtin_pack.rs`, `crates/aegis-model/tests/pack.rs`, `crates/aegis-model/tests/pack_limits.rs`, `crates/aegis-model/tests/pack_properties.rs`, `crates/aegis-model/tests/fixtures/packs/med-device-demo.json`, `crates/aegis-model/benches/pack.rs`, `crates/aegis-model/fuzz/fuzz_targets/pack.rs`, `crates/aegis-model/fuzz/seeds/pack/{iso21434-r155,med-device-demo}.json`, `docs/adr/0011-methodology-pack-files.md`
- changed: `crates/aegis-model/src/methodology/iso21434.rs` (now `include_str!` + `read_pack` only), `src/methodology/mod.rs` (`mod pack`, length-first `check_key`, `error_key_text` helper, new unit tests), `src/store/mod.rs` (8 new `Limit` variants, `MAX_PACK_FILE_BYTES`, `MAX_LABEL_BYTES`, `pub(crate) mod wire`), `src/store/wire.rs` (shared leaves `pub(crate)`, `CappedReader` moved here and parameterized, `decode_pack` entry), `src/store/snapshot.rs` (uses shared `CappedReader`), `src/error.rs` (`PackError`, bounded-key docs), `src/lib.rs` (re-exports, pack doctest), doctests in `src/asset.rs`, `src/project.rs`, `src/store/op_log.rs`, `tests/common/mod.rs` (`pack_bytes`, `key_of_len`, `max_def`, `second_every_op`), `tests/second_methodology.rs` (every-op walk moved to the kit), `Cargo.toml` (`[[bench]] pack` only), `fuzz/Cargo.toml` (`[[bin]] pack`), `fuzz/run.sh` (accepts `pack`; default runs all three), `README.md` (Fuzzing section), `docs/adr/0008-...md` (one pointer note to ADR 0011)

Acceptance criteria:
- [x] AC1 — `cargo test --test builtin_pack` → 9 passed (pack decodes to the old hand-written `expected()`, equals `Methodology::new(expected)`, identity, impacts/levels, properties, STRIDE mapping, `r155-annex5` syntax and order).
- [x] AC2 — the `grep -nE 'negligible|confidentiality|spoofing|tampering|r155-annex5|Annex 5' src/methodology/iso21434.rs | grep -vE '^[0-9]+:\s*//'` prints nothing.
- [x] AC3 — neutrality grep prints only `src/methodology/mod.rs:12:mod iso21434;`, identical to master.
- [x] AC4 — `cargo test --test store` passes; `git diff --exit-code master -- <golden fixtures and snapshot/op_log seeds>` exits 0.
- [x] AC5 — `cargo test --test builtin_pack builtin_pack_is_canonical` passes (also checks `write_pack(expected())` gives the same bytes).
- [x] AC6 — `cargo test --test pack second_pack_` → 4 passed (loads equal to `new(second_def())`, canonical bytes, every `Op` variant via `common::second_every_op`, a project saved under the built def loads under the pack). `grep -rn 'med-device' src` prints nothing (see Notes).
- [x] AC7 — `cargo test --test pack_properties round_trip` → `round_trip` (64 random valid defs, 1 entry to every max, non-ASCII/control/quote/backslash labels, optional `digits`, writes twice equal) and `round_trip_fixed_defs` (second def and `max_def()`) pass.
- [x] AC8 — `cargo test --test pack pack_rejects_` → 14 passed: wrong format name → `UnsupportedFormat{None}`; version 2 → `Some(2)` with the same text as `StoreError`; 0, -1, 1.5, 1.0 → `None`; unknown/missing/repeated/mistyped fields, `min: 70000`, `required: 256`, trailing data → `Decode` at the asserted line; duplicate property → `PackError::Methodology(DuplicateProperty("i"))`; `digits` missing and `null` both `None`.
- [x] AC9 — `cargo test --test pack_limits` → 14 passed: every caps-table row at cap and cap + 1 with that row's `Limit`; key cap in all 7 positions (methodology key, impact, level, property, threat, `violates` entry, scheme); label cap in all 6 positions; version and label also show the model-stricter case; maximal pack loads, measured 1,481,198 bytes < 4 MiB.
- [x] AC10 — `cargo test --test pack_limits pack_file_cap_stops_reading` passes: `repeat(b' ')` behind a counting reader gives `PackFileBytes` with at most cap + 1 bytes pulled; also an endless string inside a valid prefix.
- [x] AC11 — `cargo test --lib methodology_error_key_text_` → 4 passed: 1 MiB keys (ASCII, control chars, 4-byte chars) in all six key positions plus an unknown `violates` entry keep at most 64 bytes ending in `...`, `Display` <= 512 bytes; cuts never split a char (emoji offsets 0..3 and a 2-byte lead); keys <= 64 bytes are shared (`Arc::ptr_eq`) and kept exactly; the `InvalidKey("Not-A-Key")` doctest passes.
- [x] AC12 — `cargo test --test pack_properties hostile` → 4 passed (arbitrary bytes, truncated packs, flipped bytes, generated packs cut and flipped; when a mutated pack is valid it must round-trip).
- [x] AC13 — `crates/aegis-model/fuzz/run.sh 30 pack` exits 0 (1,885,315 runs in 31 s); `run.sh 5 snapshot op_log` exits 0.
- [x] AC14 — `cargo test --doc` passes (part of `cargo test`; 217 doctests); clippy with `missing_docs` and `clippy::pedantic` clean. `read_pack`, `write_pack`, `Methodology::read_pack`, `iso21434_r155` state time and space; `Limit` docs say it is carried by both `StoreError` and `PackError`.
- [x] AC15 — `grep -nE '\.unwrap\(|\.expect\(|panic!' src/methodology/pack.rs src/methodology/iso21434.rs` prints nothing.
- [x] AC16 — `cargo bench --bench pack` runs (medians: `iso21434_r155` 14.5 µs, `read_pack_max` 3.50 ms, `decode_pack_max` 3.37 ms, `write_pack_max` 465 µs on an M2 Max); numbers are in ADR 0011; `cargo test --benches` passes.
- [x] AC17 — `test -f docs/adr/0011-methodology-pack-files.md && grep -qi 'MAX_PACK_FILE_BYTES' ...` succeeds; ADR covers format, JSON vs TOML, embedding vs runtime file, caps table with largest-pack arithmetic and the measured size, error-text bounding, open question 1 with its risk (owner answer recorded), bench numbers, dependency notes (none added).
- [x] AC18 — `git diff master -- crates/aegis-model/Cargo.toml` shows only the new `[[bench]] name = "pack"`; `scripts/deny.sh licenses bans sources` → ok for `aegis-model` and `aegis-model/fuzz`.
- [x] AC19 — `cargo test` and `cargo test --benches` pass.

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass
- `cargo test --benches` → pass
- `scripts/ci.sh crates/aegis-model` → pass (fmt, clippy, test ok)
- `scripts/deny.sh licenses bans sources` → pass
- `crates/aegis-model/fuzz/run.sh 30 pack` → pass (exit 0)

Notes:
- Strictness beyond the spec: serde's derived struct visitor also accepts a JSON array of field values in place of an object. Pack wire structs go through a private `Object<T>` wrapper (a `MapOnly` deserializer adapter) so that form is a `Decode` error. The store's snapshot/op-log wire types were left as they are (out of scope); a reviewer may want the same treatment there in a later job.
- `CappedReader` moved from `store/snapshot.rs` into `store/wire.rs` and takes its cap as a parameter; snapshot behaviour is unchanged (its unit test was adapted).
- `FORMAT_VERSION` (1) is shared by snapshot, op log, and pack via the one `FormatVersion` type. ADR 0011 notes that it must become per-format when any format moves to v2.
- AC6 grep: on master, two unit tests inside `src/store/` (`empty_project_round_trips`, `methodology_is_checked_while_decoding`) contained the literal `med-device-demo`. They now take the key from the test kit, so `grep -rn 'med-device' src` is empty as the AC requires; the assertions are otherwise the same.
- An existing unit test (`methodology_rejects_bad_key_syntax`) expected a 65-byte invalid key to be kept whole; it now expects the cut text (`a` + 60 `b` + `...`), which is the new specified behaviour.
- `fuzz/run.sh` with no target now runs all three targets (README updated).
- The property-test generators draw labels and keys as a short char pattern plus a length, which keeps generating maximal definitions cheap. The `pack_properties` suite takes about 3 s in debug, where the first per-char version took 195 s.
- `MethodologyDef::read_pack` decodes from a `BufReader` (8 KiB) around the capped reader, also for the 3.8 KB built-in. Measured cost of `iso21434_r155()` is about 15 µs, so it keeps no cache, as the spec says.
