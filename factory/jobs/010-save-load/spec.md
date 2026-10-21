# 010-save-load: Save and load a `Project`, and an append-only op log, written with stable vocabulary keys and capped at decode time

Builds on job 009 (`docs/adr/0008-methodology-neutral-domain-model.md`, open question 2 there: "typed indices in memory, stable keys on disk"). Security review notes from job 009 (`factory/jobs/009-domain-model-v0-2/round-1/review-security.md`) asked for exactly this: cap the decoded op size, including `damages`, before `validate` runs (R3).

## Open questions

Only big-picture questions. Each has a default. If the owner says nothing, the builder implements the default.

1. **Loading under a different version of the same methodology.** A file records the methodology `key` and `version` it was written with. Because files hold keys and not indices, a project saved under pack version 1.0 still means the same thing under 1.1 if 1.1 only reorders or adds vocabulary. A key that 1.1 dropped fails with a typed error, so nothing is silently reinterpreted. **Default: the `key` must match, and a different `version` is accepted. The log reader exposes the file's version (`OpLogReader::methodology_version`). For snapshots, a small `peek` helper can follow when a caller needs it.** The other option is to reject any version mismatch until the migration job exists. That is safer if a pack author ever changes what a key *means*, but it makes every pack update break every existing file.
2. **Tamper evidence for the op log.** Direction-v1 hash-chains the *audit* log. It says nothing about the op log. **Default: no hash chain in this job.** Records carry a contiguous sequence number, and the format has a version field, so a chain (each record holding the hash of the previous one) can come in format version 2 when the server's store job decides on it. The other option is to chain now, which adds a hash dependency (for example `sha2` or `blake3`) and has to settle canonical bytes now.

Small choices settled here, which the owner does not need to decide:
- **Format: JSON**, through `serde` + `serde_json`. A snapshot is one compact JSON object. The op log is JSON Lines: one header line, then one record per line. Why: R8 asks for a stable, versioned format for archives and exchange. JSON is readable and diffable, every language can read it, and both crates are MIT/Apache-2.0. A binary format (postcard, CBOR) stays an option for the local cache (R8 also suggests `rkyv` there) and is out of scope.
- **Where the bytes go is out of scope.** The API writes to any `std::io::Write` and reads from any `Read`/`BufRead`. Files, PostgreSQL rows and the network are later jobs.
- **A new model limit: at most 256 damage links per threat** (`MAX_DAMAGE_LINKS`). `validate` enforces it, so every project the model accepts can be saved and loaded again. Today `damages` has no limit of its own.
- **No `cargo-fuzz` in this job.** `libfuzzer-sys` is licensed `(MIT OR Apache-2.0) AND NCSA`, and NCSA is not in the D5 allow list. Hostile-input proptests cover the decoder here (AC13). Adding a fuzz target needs its own license decision first.

## Goal

A developer can save a `Project` to bytes and load it back as an equal project, and can record every accepted `Op` in an append-only log that can be replayed onto an empty project, or onto a loaded snapshot, to rebuild it. Files never hold in-memory vocabulary indices. Every impact category, level, property, threat category and catalog scheme is written as its stable string key. So loading a file under a methodology whose vocabularies are reordered, or have new items inserted, gives the same meaning, and saving again gives the same bytes. Decoding is safe on hostile input: the total size, every record, every string, and every list (including an op's `damages` and `catalog_refs`) are capped while decoding, before any model validation runs. Breaking a cap gives a typed error, never a panic, and memory stays bounded.

## Scope

In:
- `Project::save` and `Project::load` (snapshot format `aegis-project` v1).
- `OpLogWriter` and `OpLogReader` (log format `aegis-oplog` v1), and `LoggedOp`.
- Private serde wire types. Save serializes from borrowed views, and load decodes into owned wire types, then converts keys to indices.
- Decode caps (table below), the `Limit` enum, and a new `StoreError`.
- The model limit `MAX_DAMAGE_LINKS = 256` and `ModelError::TooManyDamageLinks`.
- Tests: round trips, determinism, reordered methodology, a golden v1 fixture, hostile inputs, caps, and errors that never leak names. Plus a `divan` bench and ADR 0009.
- Dependencies: `serde` (feature `derive`) and `serde_json`, nothing else.

Out:
- `AttackTree` serialization. Trees are not part of `Project` yet. They come with the job that links threats to trees.
- Methodology pack files (the next backlog job). The methodology is supplied by the caller, and only its key and version are written.
- Storage backends, file locking, fsync policy, encryption, compression, and the server's PostgreSQL schema.
- Hash chaining (open question 2), provenance fields (a later job, through a format version bump), and migration between methodology versions.
- Keeping a project and its log in sync transactionally. The caller applies an op, then appends it. If the append fails, the caller undoes the op with the inverse (documented on `append`). The client-core and server-actor jobs own this.
- Saving tombstones or slot layout. Equality is logical (ADR 0008), so only live entities are saved.

## Design notes

### Files

Crate `crates/aegis-model/` only.

| File | Contents |
|---|---|
| `Cargo.toml`, `Cargo.lock` | add `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"`; refresh the lock file (CI runs `--locked`) |
| `src/store/mod.rs` | public API re-exports, the cap constants, `Limit`, module docs with the format description |
| `src/store/wire.rs` | private serde types: borrowed `*View<'a>` for writing, owned `*Wire` for reading; `BoundedStr<const N>`, `BoundedVec<T, const N>`, and a bounded key-set and key-map visitor that rejects duplicates |
| `src/store/snapshot.rs` | `Project::save`, `Project::load` |
| `src/store/op_log.rs` | `OpLogWriter`, `OpLogReader`, `LoggedOp`, and op encode/decode (index <-> key) |
| `src/error.rs` | `StoreError`; `ModelError::TooManyDamageLinks { threat, max }` |
| `src/threat.rs`, `src/project.rs` | `MAX_DAMAGE_LINKS`, checked in `validate` for `AddThreatScenario` and `LinkDamage` |
| `src/lib.rs` | `mod store;`, re-exports, a "Saving and loading" section with a doctest |
| `tests/store.rs`, `tests/store_limits.rs`, `tests/store_properties.rs` | integration tests and proptests |
| `tests/fixtures/project-v1.json`, `tests/fixtures/oplog-v1.jsonl`, `tests/fixtures/.gitattributes` (`* -text`) | golden v1 files, generated once with the test kit's fixed `v7(n)` IDs and checked in |
| `tests/common/mod.rs` | may grow helpers, for example a `replay` callback that also receives the accepted op, and `reordered(def)` |
| `benches/store.rs` | `divan` bench, registered as `[[bench]] name = "store" harness = false` |
| `docs/adr/0009-save-load-format.md` | ADR (L1), including the dependency notes (L6) |

### Public API

```rust
impl Project {
    pub fn save<W: Write>(&self, w: W) -> Result<(), StoreError>;
    pub fn load<R: Read>(r: R, methodology: Arc<Methodology>) -> Result<Project, StoreError>;
}
pub struct LoggedOp { pub seq: u64, pub op: Op }
impl<W: Write> OpLogWriter<W> {
    pub fn create(w: W, m: Arc<Methodology>) -> Result<Self, StoreError>;   // writes the header line
    pub fn resume(w: W, m: Arc<Methodology>, last_seq: u64) -> Self;      // appends after an existing log
    pub fn append(&mut self, op: &Op) -> Result<u64, StoreError>;          // returns the new seq
    pub fn last_seq(&self) -> u64;
    pub fn into_inner(self) -> Result<W, StoreError>;                      // flushes
}
impl<R: BufRead> OpLogReader<R> {
    pub fn open(r: R, m: Arc<Methodology>) -> Result<Self, StoreError>;   // reads and checks the header
    pub fn methodology_version(&self) -> &str;                            // the version in the header
    pub fn last_seq(&self) -> u64;
    pub fn line(&self) -> u64;                                            // 1-based line of the last record read
}
impl<R: BufRead> Iterator for OpLogReader<R> { type Item = Result<LoggedOp, StoreError>; } // fused after the first error
```

`Project::load` builds the project only through `Project::apply` with `AddAsset`, then `AddDamageScenario`, then `AddThreatScenario` ops. Model validation therefore has a single path (Q4), and the snapshot needs no dependency order of its own. `load` does not return the snapshot's methodology version (see open question 1).

### Format v1

The snapshot is one line, ending in `\n`:

```json
{"format":"aegis-project","version":1,"methodology":{"key":"iso21434-r155","version":"0.1.0"},
 "assets":[{"id":"<uuid>","name":"Brake ECU","properties":["availability","integrity"]}],
 "damage_scenarios":[{"id":"<uuid>","name":"Vehicle cannot brake","impacts":{"safety":"severe"}}],
 "threat_scenarios":[{"id":"<uuid>","name":"Modify firmware","asset":"<uuid>","category":"tampering",
   "catalog_refs":[{"scheme":"r155-annex5","id":"4.1"}],"damages":["<uuid>"]}]}
```

The op log has one header line, then one record per line, each ending in `\n`:

```
{"format":"aegis-oplog","version":1,"methodology":{"key":"iso21434-r155","version":"0.1.0"}}
{"seq":1,"op":{"op":"add_asset","id":"<uuid>","name":"Brake ECU","properties":["integrity"]}}
{"seq":2,"op":{"op":"set_impact","id":"<uuid>","category":"safety","level":null}}
```

Rules:
- **Keys, never indices.** The only JSON numbers in any file are `version` and `seq`. IDs are the hyphenated lowercase UUID text. They must be UUIDv7, checked through the existing `FromStr`. A catalog ref is `{scheme: <key>, id: <text>}`. The text is formatted with `display_catalog_ref`, and on load it is parsed again with `parse_catalog_ref`, so it is checked against the scheme's syntax again.
- **Op tags** are the `Op` variant names in snake_case (`add_asset` ... `remove_threat_scenario`), internally tagged as `"op"`. Field names are the `Op` field names. `SetImpact.level` is a key or `null` (unrated).
- **Canonical, methodology-order-free output (P4).** Snapshot entities are in public-ID order (the existing iterators). Property sets are sorted by key, as bytes. Impacts are an object sorted by category key, and unrated categories are left out. Snapshot catalog refs are sorted by scheme key, then by numeric segments. Damage links are sorted by ID. In an op, lists keep the op's own order (`catalog_refs`, `damages`, and `impacts` as an object in op order), so `decode(encode(op)) == op` exactly, and the order still never depends on vocabulary indices. Property sets in ops are sorted by key.
- **Strict decoding.** Every wire struct has `deny_unknown_fields`. A duplicate field, a duplicate key in a set or map, a blank line, `\r`, trailing data after the value, or a final record without `\n` is an error. Keys are looked up with the `Methodology::*(key)` lookups, and a miss gives `UnknownKey`.
- **Header checks.** `format` must be the expected string and `version` must be `1`, otherwise `UnsupportedFormat`. The methodology `key` must equal the supplied one, otherwise `MethodologyMismatch`. A different methodology `version` is accepted (open question 1).
- **Sequence.** `seq` starts at 1 and goes up by exactly 1. The reader rejects a gap, a repeat or a step back with `Sequence { expected, found }`. `resume(w, m, n)` continues at `n + 1`.
- **Atomic append.** `append` encodes the op into a reused internal buffer (R2) first. Only when encoding succeeded does it write the whole line with one `write_all`. An op whose indices are not in the writer's methodology gives `UnencodableOp`, and nothing is written.

### Decode caps (R3, R6)

Each cap is checked while decoding: inside the serde visitor, or by a bounded reader before serde sees the bytes. A list visitor counts elements and pre-allocates at most `min(size_hint, N)`. On element N+1 it stops with `Limit::X`, so the list never holds more than N items. Each cap is at least the model's own limit, so any valid project round-trips (AC12).

| Cap | Value | Model limit it covers |
|---|---|---|
| `MAX_PROJECT_FILE_BYTES` (snapshot) | 64 MiB | none; a typical project (300 assets, 2,000 threats) is about 1 MB |
| `MAX_OP_RECORD_BYTES` (one log line, header included) | 64 KiB | the largest valid op is about 12 KiB |
| name | 1,024 bytes | `MAX_NAME_CHARS` = 256 chars, up to 4 bytes each; the model then checks chars |
| vocabulary key (including the header's methodology key) | `MAX_KEY_LEN` = 64 bytes | same |
| methodology version in a header | 256 bytes | `MAX_VERSION_CHARS` = 64 chars |
| catalog ref text | 64 bytes | prefix 16 + 4 segments x 5 digits + 3 dots = 39 |
| `catalog_refs` per op or threat | `MAX_CATALOG_REFS` = 16 | same |
| `damages` per op or threat | `MAX_DAMAGE_LINKS` = 256 | new, same |
| impacts per op or damage scenario | `MAX_IMPACT_CATEGORIES` = 32 | same |
| properties per set | `MAX_PROPERTIES` = 64 | same |

The entity lists of a snapshot are bounded by the file byte cap. Each wire entity takes no more memory than its JSON text, so peak load memory is O(file bytes) plus the project. The builder must state the measured factor in the ADR.

`Limit` is a public `#[non_exhaustive]` enum with one variant per cap, each carrying `max`. Caps found inside a serde visitor must come out as the typed `StoreError::LimitExceeded(Limit)`, not as a generic decode error. The mechanism is up to the builder (for example a thread-local side channel set by the bounded visitors, or a fixed tag in the custom error message mapped back). It must have no `unwrap`, and AC11 must cover it.

### `StoreError`

A new `thiserror` enum, `#[non_exhaustive]`, with these variants:
- `Io(#[from] std::io::Error)`
- `Decode { line, column }`: bad JSON or wrong shape
- `LimitExceeded(Limit)`
- `UnsupportedFormat`
- `MethodologyMismatch { expected, found }`
- `UnknownKey { kind: VocabKind, key }`
- `InvalidId`
- `InvalidCatalogRef { scheme }`
- `DuplicateEntry`
- `Sequence { expected, found }`
- `TruncatedRecord { line }`
- `MissingHeader`
- `UnencodableOp`
- `Model(#[from] ModelError)`

**No error ever carries decoded text except vocabulary keys.** Keys are capped at 64 bytes and printed with `{:?}`, as `MethodologyError` already does. `Decode` keeps only the position and drops the `serde_json` message, because that message can quote input such as an entity name. `StoreError` is not `PartialEq` (`io::Error`), so tests use `matches!`.

### Complexity (A8; put it in each doc comment)

n = live entities, L = damage links plus catalog refs, B = input bytes, m = methodology size, k = size of one op.

| Operation | Time | Extra space |
|---|---|---|
| `save` | O(n + L log L_max + m log m) | O(m) for the key-sorted vocabulary order, no copy of the project |
| `load` | O(B + n log n + L log n) | O(B) |
| `append` | O(k log k) | O(k), reused buffer |
| reader, per record | O(record bytes + k log k) | O(64 KiB), reused line buffer |

Nothing is O(n) per op.

### Bench (L2)

`benches/store.rs` measures:
- `save` and `load` on a generated project shaped like the `typical` reference: 300 assets, 2,000 damage scenarios, and 2,000 threats with 2 refs and 2 damage links each;
- `append` and read for 10,000 ops.

The ADR records the times, the snapshot size in bytes per entity, and the peak load memory factor. Budget context: "open project from local cache <= 1.0 s (large)". This job covers only the TARA part of a project, so the ADR says how much of that budget it uses.

## Acceptance criteria

All commands run from `crates/aegis-model/` unless marked "repo root".

- [ ] AC1 The only new dependencies are `serde` (derive) and `serde_json`, and the lock file is current. Check: `cargo tree --depth 1 -e normal` lists exactly `serde`, `serde_json`, `thiserror`, `uuid`; `cargo test --locked` succeeds; (repo root) `scripts/deny.sh licenses bans sources` passes.
- [ ] AC2 Snapshot round trip, proptest, 256 cases per methodology (built-in and `second()`), over the existing random op sequences. After every accepted op: `Project::load(save(p), m) == p`, and saving the loaded project gives the same bytes. Test: `cargo test --test store_properties snapshot_round_trip`.
- [ ] AC3 Determinism (P4). Two projects with equal content, reached by different op orders and including removed (tombstoned) entities, save to identical bytes. Saving one project twice gives identical bytes. Test: `cargo test --test store save_is_deterministic`.
- [ ] AC4 Reordered methodology keeps meaning. `reordered(second_def())` has the same key and version. It reverses the impact categories, properties, threat categories, each `violates` list and the catalog schemes, and prepends one unused item to each of those lists. It never reorders a level scale. A project saved under `second()` loads under the reordered methodology, every entity reads back the same keys (property keys, impact category and level keys, threat category key, catalog ref text), and saving it again gives identical bytes. An op log written under `second()` reads under the reordered methodology, and re-encoding every op gives identical record bytes. Test: `cargo test --test store reordered_methodology_keeps_meaning`.
- [ ] AC5 No indices on disk. For a snapshot and a log that use every op variant and every vocabulary kind, parsing each line as `serde_json::Value` and walking it finds JSON numbers only under the keys `version` and `seq`. Test: `cargo test --test store only_keys_on_disk`.
- [ ] AC6 The v1 format is frozen by golden files. `tests/fixtures/project-v1.json` loads, and re-saves byte for byte. `tests/fixtures/oplog-v1.jsonl` reads, replays, and re-encodes byte for byte, and it covers all 16 op variants. Both files are the built-in methodology with fixed IDs. Test: `cargo test --test store golden_v1`.
- [ ] AC7 Op round trip, proptest, 256 cases per methodology. For every op the project accepts and for its inverse, `decode(encode(op)) == op`. Test: `cargo test --test store_properties op_round_trip`.
- [ ] AC8 Replay. Proptest, 256 cases per methodology:
  - (a) logging every accepted op, then reading the log and applying each op to an empty project, gives a project `==` the final one;
  - (b) a snapshot saved at a random step, plus replaying the log records after that step, gives the same project.

  Test: `cargo test --test store_properties replay`.
- [ ] AC9 Append-only and sequencing. Tests cover each of these:
  - `seq` runs 1, 2, 3, ...;
  - `resume` continues the numbering;
  - the reader rejects a gap, a repeated `seq` and a step back with `Sequence`;
  - an `append` of an op with an index outside the writer's methodology returns `UnencodableOp` and leaves the writer's `Vec<u8>` byte-identical;
  - the writer has no API that rewrites or truncates.

  Test: `cargo test --test store op_log_sequencing`.
- [ ] AC10 Framing and header errors. Each gives its typed error:
  - a final record without `\n` gives `TruncatedRecord`, after the earlier records were yielded as `Ok`;
  - a blank line, `\r\n`, or trailing data gives `Decode`;
  - empty input gives `MissingHeader`;
  - a wrong `format` string or `version: 2` gives `UnsupportedFormat`;
  - another methodology key gives `MethodologyMismatch`;
  - an unknown key of each `VocabKind` gives `UnknownKey`;
  - a non-v7 or malformed ID gives `InvalidId`;
  - a bad catalog ref text gives `InvalidCatalogRef`;
  - a duplicate key in a property set or impact map gives `DuplicateEntry`;
  - an unknown field gives `Decode`;
  - a snapshot threat that names a missing asset gives `Model(UnknownAsset(_))`.

  A file whose methodology `version` differs but whose key matches loads successfully (open question 1 default). Test: `cargo test --test store framing_and_header_errors`.
- [ ] AC11 Caps fire before `validate`. Each hostile input below gives `LimitExceeded` with the matching `Limit` and `max`, not a `ModelError`. Each is tested both as an op-log record and, where the field exists, in a snapshot:
  - 17 `catalog_refs`;
  - 257 `damages`;
  - 33 impacts;
  - 65 properties;
  - a name of 1,025 bytes;
  - a key of 65 bytes;
  - a catalog ref text of 65 bytes;
  - a methodology version of 257 bytes;
  - a log line of 64 KiB + 1 byte;
  - a snapshot of 64 MiB + 1 byte, fed from `std::io::repeat(b' ').take(..)` without building it in memory.

  Test: `cargo test --test store_limits`.
- [ ] AC12 Cap parity: every valid project round-trips. A threat with exactly 16 refs and 256 damage links, names of 256 four-byte chars, 64-byte keys, and every impact category rated saves, loads and logs successfully. A 257th link through `LinkDamage` or `AddThreatScenario` fails `validate` with `ModelError::TooManyDamageLinks`. The existing op proptests and invariants still pass. Tests: `cargo test --test store_limits limits_admit_every_valid_project` and `cargo test --test project too_many_damage_links`.
- [ ] AC13 Hostile input never panics. A proptest, at least 512 cases, feeds `Project::load` and `OpLogReader` random bytes, and also valid files with random byte flips, insertions and truncations. Each run returns `Ok` or `Err`. A project that loads `Ok` saves and loads again as an equal project. Test: `cargo test --test store_properties hostile_input_never_panics`.
- [ ] AC14 Errors never leak names. The sentinel `SECRET-LABEL-9f3` goes into a name, into an ID position, into a catalog ref text, and as a string where `seq` (a number) is expected, so each input fails at a different stage. No `Display` or `Debug` output of the resulting `StoreError` contains it. Test: `cargo test --test store errors_have_no_labels`.
- [ ] AC15 Docs. Every new public item has a doc comment with a compiling example. Every public function that takes project-sized input states its time and space cost (A8, Q5). `lib.rs` has a save/load doctest. Check: `cargo test --doc` and `cargo clippy --all-targets -- -D warnings` (`missing_docs` and `clippy::pedantic` are on).
- [ ] AC16 No panics in library code (Q3). Check (repo root): `for f in crates/aegis-model/src/store/*.rs; do awk '/^#\[cfg\(test\)\]/{exit} {print FILENAME":"FNR": "$0}' "$f"; done | grep -E '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!|unimplemented!'` prints nothing. `#![forbid(unsafe_code)]` stays.
- [ ] AC17 Neutrality still holds: no automotive words in core code. Check (repo root), which must print nothing:
  `grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' crates/aegis-model/src --include='*.rs' | grep -v '^crates/aegis-model/src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//' | grep -vE ':[0-9]+:\s*(pub(\([a-z]+\))? )?mod iso21434;$'`
- [ ] AC18 Bench and ADR. `cargo test --benches` passes, and `cargo bench --bench store` runs. `docs/adr/0009-save-load-format.md` covers:
  - the options compared: JSON vs postcard/CBOR, JSON Lines vs one array, keys vs indices, decode-time caps vs post-decode checks;
  - the format rules and the caps table;
  - both open questions and their outcome;
  - two-line dependency notes for `serde` and `serde_json` (L6);
  - the bench numbers and the load memory factor.

  Check (repo root): `test -f docs/adr/0009-save-load-format.md && grep -qi 'serde_json' docs/adr/0009-save-load-format.md && grep -qi 'MAX_DAMAGE_LINKS' docs/adr/0009-save-load-format.md`.
- [ ] AC19 Nothing else regresses. The existing tests, benches and doctests pass unchanged. Check: `cargo test` and `cargo test --benches`.

## Checks

From `crates/aegis-model/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --locked
cargo test --benches
cargo tree --depth 1 -e normal
```

From the repo root:

```sh
scripts/ci.sh crates/aegis-model
scripts/deny.sh licenses bans sources
# AC16: must print nothing
for f in crates/aegis-model/src/store/*.rs; do awk '/^#\[cfg\(test\)\]/{exit} {print FILENAME":"FNR": "$0}' "$f"; done | grep -E '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!|unimplemented!'
# AC17: must print nothing
grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' crates/aegis-model/src --include='*.rs' | grep -v '^crates/aegis-model/src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//' | grep -vE ':[0-9]+:\s*(pub(\([a-z]+\))? )?mod iso21434;$'
test -f docs/adr/0009-save-load-format.md
```

If the 1.85 toolchain is installed, also run `cargo +1.85 check --locked` from `crates/aegis-model/`. CI runs it in any case (ADR 0007).
