# ADR 0009: Save/load format: JSON snapshot and op log with stable keys and decode-time caps

- Status: proposed (job 010-save-load)
- Date: 2026-10-21
- Crate: `crates/aegis-model` (`src/store/`, `MAX_DAMAGE_LINKS` in
  `src/threat.rs`); fuzz crate `crates/aegis-model/fuzz/`
- Rules: P4, D3, D7, R2, R3, R6, R8, A8, Q3, Q4, Q5, Q7, L1, L2, L6 in
  `docs/engineering-standards.md`; decision D5 in `docs/decisions.md`
- Builds on: `0008-methodology-neutral-domain-model.md` (open question 2
  there: "typed indices in memory, stable keys on disk")

## Context

A `Project` lives in memory with small typed indices into its methodology's
vocabularies (`ImpactCategory`, `Level`, `Property`, `ThreatCategory`,
`CatalogScheme`). Those indices are positions in declaration order. A file
that stored them would change meaning as soon as a pack author reordered or
extended a vocabulary. Job 010 adds the first persistent formats:

- a **snapshot** of a project, to save and open it;
- an append-only **op log** of every accepted `Op`, to rebuild a project by
  replay, alone or on top of a snapshot.

Both are untrusted input when read back (a file from a customer, a
colleague, or an attacker). The security review of job 009 asked for the
decoded op size, including `damages`, to be capped before `validate` runs
(R3).

## Options

### Encoding: JSON, or a binary format (postcard, CBOR)

| Option | Good | Bad |
|---|---|---|
| **JSON (`serde_json`)** | Readable and diffable; every language reads it; stable and self-describing, so a field can be named in an error position and a version field is natural; MIT/Apache-2.0 | Larger (255 bytes per entity here) and slower than binary; numbers and strings need escaping |
| postcard | Compact and fast; MIT/Apache-2.0 | Not self-describing: the schema lives in Rust struct layout, so a field reorder silently changes meaning; no other tooling reads it |
| CBOR (`ciborium`) | Self-describing, compact, a standard | Not human-readable or diffable; a second encoding to learn for little gain at this size |

**Chosen: JSON.** R8 asks for "a stable versioned format for archives and
exchange", and that is what these files are. The measured cost (below) is a
few milliseconds for a typical project. The local cache that R8 suggests
`rkyv` for is a separate, later concern; nothing here prevents it.

### Op log framing: JSON Lines, or one JSON array

| Option | Good | Bad |
|---|---|---|
| **JSON Lines: a header line, then one record per line** | Appending is one `write_all` of one line; a reader streams one line at a time through a bounded buffer; a cut-off write shows up as a last line with no `\n` (`TruncatedRecord`), and every earlier record is still readable | Each record repeats its field names |
| One JSON array | One value, one parse | Appending means rewriting the closing `]`; reading needs the whole array, or a streaming visitor; a cut-off file is just invalid |

**Chosen: JSON Lines.** The writer has no API that rewrites or truncates.

### Vocabulary on disk: keys, or indices

| Option | Good | Bad |
|---|---|---|
| **Stable string keys** (`"tampering"`, `"r155-annex5"`) | Meaning survives a reordered or extended vocabulary; a removed key fails with a typed `UnknownKey` instead of being reinterpreted; readable | Larger, and a key lookup (O(log m)) per item on load |
| Indices | Small and fast | Meaning depends on declaration order: inserting one property shifts every later index, silently |

**Chosen: keys.** The only JSON numbers in either file are `version` and
`seq` (`tests/store.rs only_keys_on_disk`). AC4 proves the point: a file
written under the second test methodology reads under a copy whose
vocabularies are reversed and extended, every entity keeps its keys, and
saving again gives identical bytes.

### Caps: at decode time, or checks after decoding

| Option | Good | Bad |
|---|---|---|
| **Caps inside the decoder** (bounded serde visitors and a bounded line reader) | Memory stays bounded whatever the input: a list stops at item N+1, a string is checked on the parser's borrowed text before it is copied, a line stops at 64 KiB + 1 | Custom visitors; a cap found inside serde must be carried out as a typed error |
| Plain `Vec<T>`/`String`, then check lengths | Simple, derive-only | A 60 MB `damages` array is fully allocated before anyone looks at it (R3); the model's `validate` would be the first check |

**Chosen: decode-time caps.** Mechanism: serde errors carry only text, so a
bounded visitor records a typed `Fault` in a thread-local side channel,
then returns a fixed-text serde error; `wire::decode` clears the channel
before each decode and reads it once after. The channel never holds input
text and cannot leak between decodes (same thread, cleared first). Faults:
a cap (`LimitExceeded(Limit)`), a repeated key in a set or map
(`DuplicateEntry`), a malformed or non-v7 ID (`InvalidId`), a wrong format
name or version (`UnsupportedFormat`). Covered by `tests/store_limits.rs`
and `src/store/wire.rs` unit tests.

## Decision

### Format v1

Snapshot (`aegis-project` v1), one line ending in `\n`:

```json
{"format":"aegis-project","version":1,"methodology":{"key":"iso21434-r155","version":"0.1.0"},
 "assets":[{"id":"<uuid>","name":"Brake ECU","properties":["availability","integrity"]}],
 "damage_scenarios":[{"id":"<uuid>","name":"Vehicle cannot brake","impacts":{"safety":"severe"}}],
 "threat_scenarios":[{"id":"<uuid>","name":"Modify firmware","asset":"<uuid>","category":"tampering",
   "catalog_refs":[{"scheme":"r155-annex5","id":"4.1"}],"damages":["<uuid>"]}]}
```

Op log (`aegis-oplog` v1), a header line, then one record per line:

```
{"format":"aegis-oplog","version":1,"methodology":{"key":"iso21434-r155","version":"0.1.0"}}
{"seq":1,"op":{"op":"add_asset","id":"<uuid>","name":"Brake ECU","properties":["integrity"]}}
{"seq":2,"op":{"op":"set_impact","id":"<uuid>","category":"safety","level":null}}
```

Rules:

- **Keys, never indices.** IDs are hyphenated lowercase `UUIDv7` text,
  checked for the canonical form and then through the ID types' `FromStr`.
  A catalog ref is `{scheme: <key>, id: <text>}`, written with
  `display_catalog_ref` and parsed again with `parse_catalog_ref` on load,
  so it is checked against the scheme's syntax again.
- **Op tags** are the `Op` variant names in snake_case, internally tagged as
  `"op"`; field names are the `Op` field names; `SetImpact.level` is a key
  or `null` (unrated) and is required.
- **Canonical output, independent of vocabulary order (P4).** Snapshot
  entities in public-ID order; property sets sorted by key; impacts an
  object sorted by category key with unrated categories left out; snapshot
  catalog refs sorted by scheme key, then numeric segments; damage links
  sorted by ID. In an op, lists keep the op's own order, so
  `decode(encode(op)) == op` exactly (AC7); property sets in ops are sorted
  by key. Equal projects save to identical bytes whatever their history
  (AC3).
- **Strict decoding.** `deny_unknown_fields` everywhere; duplicate fields,
  repeated keys in a set or map, a blank line, any `\r`, trailing data, and
  a last record without `\n` are errors.
- **Header checks.** Wrong `format` or a `version` other than 1:
  `UnsupportedFormat`, as soon as the field is read. Another methodology
  key: `MethodologyMismatch`. Another methodology version: accepted (open
  question 1).
- **Sequence.** `seq` starts at 1 and goes up by exactly 1; a gap, repeat,
  or step back is `Sequence { expected, found }`. `resume(w, m, n)`
  continues at `n + 1`.
- **Atomic append.** `append` encodes into a reused buffer first and writes
  the whole line with one `write_all` only if encoding succeeded. It refuses
  (`UnencodableOp`, nothing written) an op it could not read back: an index
  not in the writer's methodology, a repeated impact category, a name or
  list over its cap, or a record over 64 KiB.
- **Load builds through `apply`.** `Project::load` decodes, checks the
  methodology, then applies `AddAsset`, `AddDamageScenario`, and
  `AddThreatScenario` ops, so model validation has one path (Q4) and the
  snapshot needs no dependency order of its own. Only live entities are
  saved; tombstones and slot layout are not (equality is logical).
- **Errors carry no decoded text** except vocabulary keys (at most 64 bytes,
  printed with `{:?}`). `Decode` keeps only line and column and drops the
  `serde_json` message, which can quote input such as a name (AC14).

### Decode caps

| Cap | Value | Model limit it covers |
|---|---|---|
| `MAX_PROJECT_FILE_BYTES` (snapshot) | 64 MiB | none; the typical project is 1.1 MB |
| `MAX_OP_RECORD_BYTES` (one log line, header included) | 64 KiB | the largest valid op is 13,261 bytes (measured, `limits_admit_every_valid_project`) |
| `MAX_NAME_BYTES` | 1,024 bytes | `MAX_NAME_CHARS` = 256 chars of up to 4 bytes; the model then checks chars |
| `MAX_KEY_BYTES` (vocabulary keys and the header's methodology key) | 64 bytes | `MAX_KEY_LEN` = 64 |
| `MAX_METHODOLOGY_VERSION_BYTES` | 256 bytes | `MAX_VERSION_CHARS` = 64 chars |
| `MAX_CATALOG_REF_BYTES` | 64 bytes | prefix 16 + 4 segments x 5 digits + 3 dots = 39 |
| `catalog_refs` per op or threat | 16 | `MAX_CATALOG_REFS` = 16 |
| `damages` per op or threat | 256 | `MAX_DAMAGE_LINKS` = 256 (new) |
| impacts per op or damage scenario | 32 | `MAX_IMPACT_CATEGORIES` = 32 |
| properties per set | 64 | `MAX_PROPERTIES` = 64 |

Each cap is at least the model's own limit, so every project the model
accepts round-trips (AC12: a methodology at every vocabulary limit with
64-byte keys, a threat with 16 refs and 256 links, names of 256 four-byte
chars, every category rated). Entity lists in a snapshot are bounded by the
file cap. `serde_json` also limits nesting to 128 levels.

**New model limit `MAX_DAMAGE_LINKS = 256`.** Before this job `damages` had
no limit of its own (only "each link unique and live", so up to n). A
decoder must cap every list, and a cap below what the model accepts would
make some valid projects unsavable. So the model now rejects a 257th link
in `AddThreatScenario` and `LinkDamage` with
`ModelError::TooManyDamageLinks { threat, max }` (`tests/project.rs
too_many_damage_links`). 256 links per threat is far above any real TARA.

### Complexity

n = live entities, L = damage links plus catalog refs, B = input bytes,
m = methodology size, k = size of one op.

| Operation | Time | Extra space |
|---|---|---|
| `save` | O(n + L log L_max + m log m) | O(m) for the key-sorted vocabulary order; the project is not copied |
| `load` | O(B + n log n + L log n) | O(B) |
| `append` | O(k log k) | O(k), reused buffer |
| reader, per record | O(record bytes + k log k) | O(64 KiB), reused line buffer |

Nothing is O(n) per op. Property sets are emitted in key order without
sorting per asset: the set's bits are permuted by each property's rank in
key order (precomputed once) and walked in order, O(p).

## Open questions and outcome

1. **Loading under a different version of the same methodology.** The key
   must match; any version is accepted; the log reader exposes the file's
   version (`OpLogReader::methodology_version`). Files hold keys, so a key
   the loaded version no longer has fails with `UnknownKey` and is never
   silently reinterpreted. Confirmed by the owner on 2026-10-21. A `peek`
   helper for the snapshot's version can follow when a caller needs it.
2. **Tamper evidence for the op log.** No hash chain in this job.
   Records carry a contiguous `seq`, and the format has a version field, so
   a chain (each record holding the hash of the previous one) can come in
   format v2 with no new dependency now. Confirmed by the owner on
   2026-10-21.
3. **Format.** JSON through `serde` and `serde_json`, frozen by the golden
   files `tests/fixtures/project-v1.json` and `tests/fixtures/oplog-v1.jsonl`
   (all 16 op variants, fixed IDs, built-in methodology; `* -text` in
   `.gitattributes`). Confirmed by the owner on 2026-10-21.

## Fuzzing (owner change during the build, 2026-10-21)

The spec ruled out `cargo-fuzz` because `libfuzzer-sys` is licensed
`(MIT OR Apache-2.0) AND NCSA` and NCSA was not in the D5 allow list. The
owner then allowed it for non-shipped tooling:

- **D5** now says crates used only to build, test, fuzz, or benchmark
  (dev-dependencies and fuzz crates, never linked into a shipped binary)
  may also use NCSA, each through a per-crate exception in `deny.toml`
  naming the crate and version and pointing to D5.
- **`deny.toml`**: `{ crate = "libfuzzer-sys@0.4.13", allow = ["NCSA"] }`
  in `[licenses] exceptions` (NCSA stays off the global allow list), and a
  `[bans] deny` entry for `libfuzzer-sys` with `wrappers =
  ["aegis-model-fuzz"]`, so any other crate that depends on it fails the
  `bans` check. Checked by hand: a scratch crate depending on
  `libfuzzer-sys` fails `bans`; the fuzz crate without the exception fails
  `licenses`. `tools/deny-fixtures/bad-ncsa` proves NCSA is still rejected
  for everything else. ADR 0003 records the first exception.
- **`scripts/deny.sh`** also checks every `crates/*/fuzz/Cargo.toml`, so CI
  covers the fuzz crate's licenses without making it a production crate
  (`scripts/ci.sh` and the MSRV job keep the `crates/*/` glob).
- **Targets** (`crates/aegis-model/fuzz/fuzz_targets/`): `snapshot` loads
  any input and, if it loads, saves and loads again and asserts equality;
  `op_log` reads every record, replays it, re-encodes every decoded op and
  asserts it reads back equal. Seeds: the golden v1 files in
  `fuzz/seeds/<target>/`.
- **Running.** Nightly is not used in CI. `crates/aegis-model/fuzz/run.sh
  [SECONDS] [TARGET...]` builds the targets on stable with the coverage
  flags `cargo-fuzz` passes (no sanitizer) and runs each for a bounded
  time; with nightly and `cargo-fuzz`, `cargo +nightly fuzz run <target>
  corpus/<target> seeds/<target>` adds AddressSanitizer (the crate has no
  `unsafe` code, so the sanitizer adds little). Result on this machine,
  60 s per target, run twice: `snapshot` 7.7 million runs (cov 2,763,
  corpus 1,992), `op_log` 3.1 million runs (cov 3,306, corpus 1,272), no
  crash. The proptest hostile-input tests (AC13, 512 cases) stay as the
  in-CI guard.

## Dependency notes (L6)

- **`serde` 1.0.229** (MIT OR Apache-2.0; now a facade over `serde_core`
  plus `serde_derive`). A format implements `Deserializer`, which drives
  parsing and calls one `visit_*` method of a type's `Visitor` per value;
  derive generates the `Deserialize` impl and its visitor, and an
  internally tagged enum first buffers the whole object into a private
  `Content` tree to find the tag, then deserializes the variant from that
  buffer.
- **`serde_json` 1.0.154** (MIT OR Apache-2.0; pulls `itoa` (MIT OR
  Apache-2.0), `memchr` (Unlicense OR MIT), and `zmij` (MIT), all within
  D5). A recursive-descent parser
  over a `Read` source (`SliceRead` for `from_slice`): strings without
  escapes are handed to the visitor borrowed from the input, others are
  unescaped into one reused `scratch` buffer; nesting is capped at 128
  levels (`remaining_depth`), so deep input cannot overflow the stack.
- **`libfuzzer-sys` 0.4.13** (fuzz crate only; `(MIT OR Apache-2.0) AND
  NCSA`, pulls `arbitrary`, `cc`). Its build script compiles LLVM's
  libFuzzer C++ runtime, which owns `main`; `fuzz_target!` exports
  `rust_fuzzer_test_input`, which libFuzzer calls with each mutated input,
  catching a panic as a crash.

## Numbers

Release build, `cargo bench --bench store` and `--bench store_memory`, on
the build machine (Apple Silicon laptop; the reference machine is a 4-core
2022 laptop, so expect somewhat slower there). Workload
(`benches/store_kit`): the `typical` TARA shape, 300 assets, 2,000 damage
scenarios, and 2,000 threats with 2 refs and 2 damage links each (4,300
entities); a 10,000-op log (the 4,300 ops that build it, then renames and
rating changes).

| Bench | Median | Throughput |
|---|---|---|
| `save` (snapshot, 1,097,749 bytes) | 1.07 ms | 1.03 GB/s |
| `load` | 4.31 ms | 255 MB/s |
| `append_10k` (2,010,433 bytes) | 1.95 ms | 5.1 M ops/s |
| `read_10k` | 8.21 ms | 1.2 M ops/s |
| `replay_log` (read + apply, 10,000 ops) | 9.87 ms | 1.0 M ops/s |

- **Size:** 255 bytes per entity in the snapshot; 201 bytes per op in the
  log. Uncompressed JSON; the "150 bytes per node after compression"
  budget is for the stored format and is not measured here.
- **Load memory:** peak heap during `load` 4,001,584 bytes = **3.65x the
  file size** (`store_memory` fails above 5x); the loaded project keeps
  912,320 bytes = 0.83x; 45,549 allocations. The peak is the input buffer
  (1x, dropped after decoding) plus the owned wire entities, whose keys
  and names are separate `Arc<str>` allocations (each with a 16-byte
  header), plus the project being built. Decoding straight into indices
  with a `DeserializeSeed` would cut the wire overhead; not needed at
  these numbers.
- **Budget:** "open project from local cache <= 1.0 s (large)". Loading the
  TARA part of the typical project takes 4.3 ms, 0.4% of that budget. Load
  is linear in bytes plus O(n log n), so a TARA part ten times larger
  (43,000 entities, about 11 MB) would take roughly 45 ms, 4.5%. Attack
  trees are not in `Project` yet and will add to this.
- **Lesson (L2):** `load` (4.3 ms) beats `replay_log` (9.9 ms for 10,000
  ops, which rebuild the same 4,300 entities plus 5,700 edits). Both go
  through `apply`; the difference is mostly decoding: an op record costs
  about 2x a snapshot entity per byte, because serde buffers each
  internally tagged op into a `Content` tree before choosing its variant.
  A snapshot plus a short log tail is the fast path to open a project; a
  hand-written op visitor that reads `"op"` first could remove the buffer
  if replay ever matters.

## Consequences

- Files are stable across vocabulary reorderings and additions, and a
  removed key is a typed error. Changing what a key *means* stays a
  pack-author responsibility until a migration job exists.
- A format change needs a version bump and new golden files; the v1 files
  stay as regression fixtures (`regenerate_golden_fixtures` is an ignored
  test, for deliberate format changes only).
- `MAX_DAMAGE_LINKS` is a new model rule; a project with more than 256
  links on one threat could never have been saved, and now cannot be built.
- Keeping a project and its log in sync is the caller's job: apply, then
  append; if the append fails, apply the inverse (documented on
  `OpLogWriter`). The client-core and server-actor jobs own this.
