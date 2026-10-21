# 012-methodology-packs: Load a methodology from a pack file, ship ISO/SAE 21434 + R155 as a pack, cap pack sizes and the key text kept in errors

Builds on job 009 (`MethodologyDef` and `Methodology::new`, ADR 0008: "pack loading can deserialize straight into `MethodologyDef` and reuse `Methodology::new`") and job 010 (capped, strict JSON decoding in `src/store/`, ADR 0009). The job 009 security review asked for size caps once the pack loader arrives. That includes the key text that `MethodologyError::InvalidKey` and `UnknownViolatedProperty` keep from unvalidated input.

## Open questions

One big-picture question. It has a default. If the owner says nothing, the builder implements the default.

1. **Pack identity and trust.** In this job a pack is named by its `key` and `version`, as project files already record it (job 010). Two different pack files can carry the same key and version, for example when a customer edits a pack and does not bump the version. A project file would then load under a pack whose keys mean something else. Pack files are also not signed. **Default: identity is key + version only, and there is no signature or content hash in this job.** The ADR records the risk. Signing, fingerprints and who may install a pack belong to the server's pack registry job (direction-v1 section 2: "Admin work … methodology packs"). The other option is a content hash now, written into project files next to the version. That needs a hash dependency and a project format version 2.

Small choices settled here, which the owner does not need to decide:
- **Format: JSON**, `aegis-methodology-pack` version 1, read with the `serde` and `serde_json` the crate already uses. No new dependency. TOML would be nicer to hand-edit, but it adds a crate, and packs will mostly be written by tools and the admin UI. JSON also keeps one strict, capped decoding approach for every file the crate reads.
- **The built-in pack is embedded in the binary** with `include_str!` from `crates/aegis-model/packs/iso21434-r155.json`, and decoded through the same reader as any other pack. So the default methodology always exists, needs no install path, and cannot be changed on disk. A customer who wants other vocabulary loads their own pack file.
- **The built-in keeps key `iso21434-r155` and version `0.1.0`**, and its content does not change. The golden project and op-log files (`tests/fixtures/`, `fuzz/seeds/`) load unchanged.
- **`Methodology::iso21434_r155()` now returns `Result<Methodology, PackError>`.** It decodes a file, so it reports pack errors honestly. `PackError` gets `From<MethodologyError>`, so the existing doctests change mechanically: `Result<(), aegis_model::MethodologyError>` becomes `Result<(), aegis_model::PackError>`. It decodes each time it is called, which takes microseconds for a pack of about 3 KB. It keeps no global cache.
- **A pack holds exactly a `MethodologyDef`.** It has no metadata, signature, catalog contents, risk matrix or feasibility tables. Those come later with a format version bump.
- **There is also a writer**, `MethodologyDef::write_pack`, so tools can export a pack and tests can round-trip packs. Its output is canonical, and the shipped pack file must be byte-identical to it.
- **Errors keep at most 64 bytes of key text.** A longer key is cut at a char boundary to at most 61 bytes, and `...` is appended. `.` is not a key char, so a cut text can never look like a valid key.

## Goal

A developer can load a methodology from a pack file (`Methodology::read_pack(reader)`) and get the same validated `Methodology` as from `Methodology::new`. ISO/SAE 21434 + UN R155 is no longer Rust code. It is a pack file shipped with the crate, and `Methodology::iso21434_r155()` loads it through the same reader. Adding a second methodology means writing one more pack file, with no code change (D1). The reader is safe on hostile input. The file size, every string and every list are capped while decoding, before `Methodology::new` runs. A broken cap is a typed error, never a panic, and memory stays bounded. Errors from `Methodology::new` never hold more than 64 bytes of key text, however long the input key is.

## Scope

In:
- Pack format `aegis-methodology-pack` v1, `MethodologyDef::read_pack`, `MethodologyDef::write_pack`, `Methodology::read_pack`, and `PackError`.
- Moving the built-in vocabulary from `src/methodology/iso21434.rs` into `packs/iso21434-r155.json`. Its vocabulary tests move to `tests/builtin_pack.rs`.
- Decode caps (table below): new `Limit` variants, plus the constants `MAX_PACK_FILE_BYTES` and `MAX_LABEL_BYTES`.
- Bounded key text in `MethodologyError::InvalidKey` and `UnknownViolatedProperty`. `check_key` checks the length first.
- A second methodology as a pack fixture (`second_def()` from the test kit), a `pack` fuzz target, a `divan` bench, and ADR 0011.

Out:
- Pack signing, fingerprints, a pack registry, install paths and admin UI (open question 1).
- Migrating a project to another pack or pack version.
- Pack content beyond `MethodologyDef`: catalog entries, risk matrices and feasibility tables.
- Any change to the project snapshot or op-log formats.

## Design notes

### Files (crate `crates/aegis-model/` unless noted)

| File | Change |
|---|---|
| `packs/iso21434-r155.json` | new: the built-in methodology, canonical `write_pack` output |
| `packs/.gitattributes` | new: `* -text`, so the bytes stay the same on Windows checkouts (the canonical-bytes test compares them) |
| `src/methodology/pack.rs` | new: private wire types, `read_pack`, `write_pack`, the decode entry |
| `src/methodology/iso21434.rs` | shrinks to `const BUILTIN: &str = include_str!("../../packs/iso21434-r155.json")` and `iso21434_r155()`, with no vocabulary data. It stays the only `src` file that names the standard (the job 009 neutrality grep) |
| `src/methodology/mod.rs` | `mod pack;`; `check_key` checks `len() <= MAX_KEY_LEN` before scanning; a helper that bounds the key text kept in errors |
| `src/store/mod.rs`, `src/store/wire.rs` | new `Limit` variants. `BoundedStr`, `BoundedVec`, `Cap`, `Format`/`FormatVersion` and the fault channel become `pub(crate)`. Add a decode entry that needs no methodology context, and map `Fault` to `PackError` |
| `src/error.rs` | `PackError`; doc updates on `InvalidKey` and `UnknownViolatedProperty` ("at most 64 bytes") |
| `src/lib.rs` | re-exports; the "Methodologies" section gains a pack doctest |
| all doctests | `aegis_model::MethodologyError>` becomes `aegis_model::PackError>` where `iso21434_r155()?` is used |
| `tests/builtin_pack.rs` | new: the moved vocabulary tests, plus the old hand-written definition as the expected value |
| `tests/pack.rs`, `tests/pack_limits.rs`, `tests/pack_properties.rs` | new |
| `tests/fixtures/packs/med-device-demo.json` | new: `second_def()` as a pack |
| `tests/common/mod.rs` | may grow helpers (for example `max_def()`, `pack_bytes(&def)`) |
| `benches/pack.rs` + `[[bench]] name = "pack"` | new |
| `fuzz/fuzz_targets/pack.rs`, `fuzz/Cargo.toml` `[[bin]]`, `fuzz/run.sh` (accept `pack`), `fuzz/seeds/pack/` (the two packs) | new target |
| `docs/adr/0011-methodology-pack-files.md` (repo root) | new ADR (L1) |
| `README.md` (repo root) | Fuzzing section lists the `pack` target |

### Public API

```rust
impl MethodologyDef {
    /// Decodes a pack: format check, caps, strict shape. No model validation.
    pub fn read_pack<R: Read>(r: R) -> Result<MethodologyDef, PackError>;
    /// Writes canonical pack JSON. Errors only on I/O.
    pub fn write_pack<W: Write>(&self, w: W) -> Result<(), PackError>;
}
impl Methodology {
    /// `read_pack`, then `Methodology::new`.
    pub fn read_pack<R: Read>(r: R) -> Result<Methodology, PackError>;
    pub fn iso21434_r155() -> Result<Methodology, PackError>;   // was MethodologyError
}
#[non_exhaustive]
pub enum PackError {
    Io(std::io::Error),
    Decode { line: u64, column: u64 },            // bad JSON or wrong shape, 1-based
    LimitExceeded(Limit),
    UnsupportedFormat { version: Option<u64> },   // same meaning and text as StoreError's
    Methodology(#[from] MethodologyError),
}
pub const MAX_PACK_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_LABEL_BYTES: usize = 4 * MAX_LABEL_CHARS;   // 1,024
```

The public `*Def` types get no serde derives. Private wire types in `pack.rs` mirror them with bounded leaves, then move into the `Def`. The `Arc<str>` values are moved, not copied.

### Format (v1)

```json
{
  "format": "aegis-methodology-pack",
  "version": 1,
  "methodology": {
    "key": "iso21434-r155", "version": "0.1.0", "label": "ISO/SAE 21434 + UN R155",
    "impact_categories": [{ "key": "safety", "label": "Safety",
                            "levels": [{ "key": "negligible", "label": "Negligible" }] }],
    "properties": [{ "key": "integrity", "label": "Integrity" }],
    "threat_categories": [{ "key": "tampering", "label": "Tampering", "violates": ["integrity"] }],
    "catalog_schemes": [{ "key": "r155-annex5", "label": "UN R155 Annex 5",
                          "syntax": { "prefix": "", "required": 1,
                                      "segments": [{ "min": 1, "max": 32 }, { "min": 1, "max": 99 }] } }]
  }
}
```

- Every field is required except `digits` on a segment. If `digits` is missing or `null`, it means `None`. All lists are present, and `catalog_schemes` may be `[]`. Declaration order is meaningful, as in `MethodologyDef`.
- Decoding is strict. Unknown, missing, repeated and mistyped fields, numbers out of the Rust type's range (`min`/`max` u16, `digits`/`required` u8), and trailing non-whitespace all give `Decode`. `format` and `version` are checked as soon as they are read, as in job 010. A wrong name, or version 0, a negative or a fractional version, gives `UnsupportedFormat { version: None }`. An integer version above 1 gives `Some(v)`.
- The writer uses `serde_json`'s pretty printer (2-space indent), in the field order above. It leaves `digits` out when it is `None`, writes non-ASCII as raw UTF-8, and adds a final `\n`. The same def always gives the same bytes (P4).

### Decode caps (R3, R6)

Each cap is checked while decoding, inside the serde visitor, before `Methodology::new`. Each cap is at least the model's own limit, so every def that `Methodology::new` accepts round-trips through `write_pack` and `read_pack`.

| What | Cap | Error |
|---|---|---|
| whole file | `MAX_PACK_FILE_BYTES` = 4 MiB (capped reader, reads at most cap + 1 bytes) | new `Limit::PackFileBytes { max }` |
| methodology key, every vocabulary key, every `violates` entry | 64 bytes | `Limit::KeyBytes` (existing) |
| methodology version | 256 bytes | `Limit::MethodologyVersionBytes` (existing) |
| every label | `MAX_LABEL_BYTES` = 1,024 bytes | new `Limit::LabelBytes { max }` |
| catalog prefix | 16 bytes (`MAX_PREFIX_LEN`) | new `Limit::CatalogPrefixBytes { max }` |
| `impact_categories` | 32 | new `Limit::ImpactCategories { max }` |
| `levels` (each category) | 16 | new `Limit::Levels { max }` |
| `properties`, each `violates` | 64 | `Limit::Properties` (existing) |
| `threat_categories` | 64 | new `Limit::ThreatCategories { max }` |
| `catalog_schemes` | 16 | new `Limit::CatalogSchemes { max }` |
| `segments` | 4 | new `Limit::CatalogSegments { max }` |

The largest valid pack is about 1.5 MB: 689 labels of 256 control chars, each escaped to 6 bytes, plus 64 threat categories that each violate 64 properties with 64-byte keys. The 4 MiB cap leaves room for that. The ADR shows this arithmetic and the measured size of the test's maximal pack. Every new `Limit` variant's `Display` names the cap and its value, never the input.

### Bounded key text in errors

`MethodologyError::InvalidKey` and `UnknownViolatedProperty.property` are the only variants that keep key text that has not been validated. Every other key field is checked by `check_key` first, so it is at most 64 bytes. Both go through one helper. A key of at most 64 bytes is kept as is, with `Arc::clone` and no allocation. A longer key becomes its longest char-boundary prefix of at most 61 bytes plus `...`, which is one bounded allocation on the error path. `check_key` tests `len() <= MAX_KEY_LEN` before it scans the bytes, so it rejects an oversized key in O(1).

### Cost (A8)

- `MethodologyDef::read_pack`: time O(file bytes). Space O(file bytes), which is at most 4 MiB in and about 1.5 MB of def at the caps.
- `Methodology::read_pack`: that, plus `new`'s O(m log m + text), where m is the vocabulary size (at most 689 entries).
- `write_pack`: time O(m + text). It writes straight from the def, with no copy.
- `iso21434_r155`: the same as `read_pack` on about 3 KB. The bench records the number.

## Acceptance criteria

Run commands from `crates/aegis-model/` unless they say "repo root".

- [ ] AC1 The built-in comes from the pack. `iso21434_r155()` decodes `packs/iso21434-r155.json` and equals `Methodology::new(expected)`, where `expected` is the old hand-written definition, now in `tests/builtin_pack.rs`. Key `iso21434-r155`, version `0.1.0`, and the moved vocabulary, STRIDE-mapping and `r155-annex5` syntax tests all pass. Check: `cargo test --test builtin_pack`.
- [ ] AC2 `src/methodology/iso21434.rs` holds no vocabulary data. Check: `grep -nE 'negligible|confidentiality|spoofing|tampering|r155-annex5|Annex 5' src/methodology/iso21434.rs | grep -vE '^[0-9]+:\s*//'` prints nothing.
- [ ] AC3 Neutrality holds (job 009 AC15). Check: `grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' src --include='*.rs' | grep -v '^src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//'` prints only the `mod iso21434;` line of `src/methodology/mod.rs` (a file name, as ADR 0008 notes), and the same result as on `master`.
- [ ] AC4 Existing files load unchanged. Check: `cargo test --test store`, and from the repo root `git diff --exit-code master -- crates/aegis-model/tests/fixtures/project-v1.json crates/aegis-model/tests/fixtures/oplog-v1.jsonl crates/aegis-model/fuzz/seeds/snapshot crates/aegis-model/fuzz/seeds/op_log`.
- [ ] AC5 The shipped pack is canonical. `write_pack(read_pack(file))` is byte-identical to `packs/iso21434-r155.json`. Check: `cargo test --test builtin_pack builtin_pack_is_canonical`.
- [ ] AC6 A second methodology is a data change (D1). `tests/fixtures/packs/med-device-demo.json` loads with `Methodology::read_pack` and equals `Methodology::new(second_def())`. A test applies every `Op` variant over the loaded methodology. No `src` file names it. Check: `cargo test --test pack second_pack_` and `grep -rn 'med-device' src` prints nothing.
- [ ] AC7 Round trip (proptest). For random valid defs, from 1 entry per vocabulary up to every count at its max, with non-ASCII and escape-heavy labels and optional `digits`, both `MethodologyDef::read_pack(write_pack(def)) == def` and `Methodology::read_pack(..) == Methodology::new(def)` hold. Writing twice gives the same bytes. Check: `cargo test --test pack_properties round_trip`.
- [ ] AC8 Strict format. Each case gets its exact error: wrong `format` → `UnsupportedFormat { version: None }`; `version` 2 → `Some(2)`; `version` 0, -1 or 1.5 → `None`; unknown, missing, repeated or mistyped field, `min: 70000`, trailing data → `Decode` with the right line; a def that breaks a model rule (for example a repeated property key) → `PackError::Methodology(DuplicateProperty(..))`. A `digits` that is missing and one that is `null` both decode as `None`. Check: `cargo test --test pack pack_rejects_`.
- [ ] AC9 Caps. For every row of the caps table, the value at the cap decodes (or reaches a model error where the model is stricter), and cap + 1 gives `LimitExceeded` with that row's `Limit`. The key cap is tested in every key position, including a `violates` entry and the methodology key. A maximal valid pack, at every count's max with 64-byte keys and labels of 256 escaped control chars, loads, and the test asserts its size is below `MAX_PACK_FILE_BYTES`. Check: `cargo test --test pack_limits`.
- [ ] AC10 The reader stops at the file cap. Reading from an endless source (`std::io::repeat(b' ')` behind a byte-counting reader) gives `LimitExceeded(Limit::PackFileBytes { .. })` after at most `MAX_PACK_FILE_BYTES + 1` bytes are pulled. Check: `cargo test --test pack_limits pack_file_cap_stops_reading`.
- [ ] AC11 Bounded error text. `Methodology::new` is given a 1 MiB invalid key as the methodology key, an impact category, a level, a property, a threat category and a scheme key, and a 1 MiB unknown `violates` entry. Each error's key text is at most 64 bytes, ends in `...`, and its `Display` is at most 512 bytes. A cut never splits a char (test with 4-byte chars). Keys of 64 bytes or fewer are kept exactly (the existing `InvalidKey("Not-A-Key")` doctest passes). Check: `cargo test --lib methodology_error_key_text_`.
- [ ] AC12 Hostile input never panics. Proptest with arbitrary bytes, and with valid packs that are truncated or have bytes flipped: `read_pack` returns `Ok` or `Err` and never panics. Check: `cargo test --test pack_properties hostile`.
- [ ] AC13 A fuzz target exists. `pack` fuzzes `Methodology::read_pack`, with the two pack files as seeds. Check (repo root): `crates/aegis-model/fuzz/run.sh 30 pack` exits 0, and `crates/aegis-model/fuzz/run.sh 5 snapshot op_log` still exits 0.
- [ ] AC14 Docs. Every new public item has a doc comment with a compiling example. `read_pack`, `write_pack` and `iso21434_r155` state their time and space cost (Q5, A8). The `Limit` docs say they are carried by both `StoreError` and `PackError`. Check: `cargo test --doc` and clippy (`missing_docs` and `clippy::pedantic` are on).
- [ ] AC15 No panics in new library code. Check: `grep -nE '\.unwrap\(|\.expect\(|panic!' src/methodology/pack.rs src/methodology/iso21434.rs` shows hits only inside `#[cfg(test)]` modules.
- [ ] AC16 Bench. `benches/pack.rs` measures `iso21434_r155()`, `Methodology::read_pack` on the maximal pack, and `write_pack`, and the ADR records the numbers. Check: `cargo bench --bench pack` runs, and `cargo test --benches` passes.
- [ ] AC17 ADR. `docs/adr/0011-methodology-pack-files.md` covers the format, JSON vs TOML, embedding vs a runtime file, the caps table with the largest-valid-pack arithmetic, error-text bounding, open question 1 and its risk, bench numbers, and dependency notes (L6: none added). Check (repo root): `test -f docs/adr/0011-methodology-pack-files.md && grep -qi 'MAX_PACK_FILE_BYTES' docs/adr/0011-methodology-pack-files.md`.
- [ ] AC18 No new dependency, and the license policy passes. Check (repo root): `git diff master -- crates/aegis-model/Cargo.toml` shows only new `[[bench]]`/`[[test]]` entries, and `scripts/deny.sh licenses bans sources` passes.
- [ ] AC19 All existing tests, doctests and benches pass. Check: `cargo test` and `cargo test --benches`.

## Checks

From `crates/aegis-model/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --benches
```

From the repo root:

```sh
scripts/ci.sh crates/aegis-model
scripts/deny.sh licenses bans sources
crates/aegis-model/fuzz/run.sh 30 pack
```
