# ADR 0011: Methodology pack files

- Status: proposed (job 012-methodology-packs)
- Date: 2026-10-22
- Crate: `crates/aegis-model` (`src/methodology/pack.rs`,
  `src/methodology/iso21434.rs`, `packs/iso21434-r155.json`, the shared
  decode leaves in `src/store/wire.rs`, `PackError` in `src/error.rs`);
  fuzz target `crates/aegis-model/fuzz/fuzz_targets/pack.rs`
- Rules: P4, D5, R3, R6, A8, Q3, Q4, Q5, Q7, L1, L2, L6 in
  `docs/engineering-standards.md`
- Builds on: `0008-methodology-neutral-domain-model.md` ("pack loading can
  deserialize straight into `MethodologyDef` and reuse `Methodology::new`")
  and `0009-save-load-format.md` (capped, strict JSON decoding)

## Context

Job 009 made the domain model methodology-neutral: a `MethodologyDef` is
plain data, and `Methodology::new` validates it. The one methodology that
shipped, ISO/SAE 21434 + UN R155, was still Rust code that built a
`MethodologyDef` by hand. Adding a second methodology therefore meant a
code change and a release, which is what the neutral model was meant to
avoid (direction-v1: methodologies are packs).

This job adds a **pack file**: a methodology as data, read at run time.
A pack is untrusted input, like a project file. A customer edits it, an
admin installs it, an attacker can craft it. The job 009 security review
asked for size caps once a pack loader exists, including the key text
that `MethodologyError::InvalidKey` and `UnknownViolatedProperty` keep
from unvalidated input: before this job, a 1 GB key ended up inside the
error, whole.

## Options

### Encoding: JSON or TOML

| Option | Good | Bad |
|---|---|---|
| **JSON (`serde_json`)** | Already a dependency; the same strict, capped decoding as every other file the crate reads (ADR 0009); errors carry a line and column; tools and the admin UI write it easily | Less pleasant to write by hand (quotes, no comments) |
| TOML (`toml`) | Nicer to hand-edit; comments | A new dependency (`toml` plus `toml_edit`/`winnow`, L6, Q7); a second decoding stack whose caps would need their own visitors and tests; packs will mostly be written by tools |

**Chosen: JSON.** No new dependency, and one decoding approach for every
file the crate reads. A hand author gets the strictness of the reader as
documentation: an unknown or misspelled field is an error with its line.

### The built-in methodology: embedded or a runtime file

| Option | Good | Bad |
|---|---|---|
| **Embedded with `include_str!`, decoded by the same reader** | Always present, no install path, cannot change on disk; one code path for every methodology; the pack file in the repository is the reviewable source | About 3.8 KB in the binary; decoded on every call (measured: about 15 µs) |
| A file next to the binary | Customers could patch it | Needs an install path and an error for "missing"; an edited built-in with the same key and version would silently change project meaning (see open question 1) |
| Keep it as Rust code | No decode cost | Two paths (code and pack) that can drift; vocabulary changes stay code changes |

**Chosen: embedded.** `src/methodology/iso21434.rs` is now one
`include_str!` and one call to `Methodology::read_pack`. It holds no
vocabulary, and it stays the only `src` file that names the standard (the
job 009 neutrality check, AC3). `iso21434_r155()` keeps no cache: 15 µs
does not justify a global and its lifetime questions; callers share the
result with an `Arc`. A customer who wants other vocabulary loads their own
pack.

The built-in keeps key `iso21434-r155` and version `0.1.0`, and its
content is exactly the old hand-written definition (`expected()` in
`tests/builtin_pack.rs`). The golden project and op-log files load
unchanged.

### Public API change

`Methodology::iso21434_r155()` now returns `Result<Methodology,
PackError>` (it was `MethodologyError`). It decodes a file, so it reports
pack errors honestly. `PackError` has `From<MethodologyError>`, so the
doctests changed mechanically.

## Decision

### Format v1

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

- A pack holds exactly a `MethodologyDef`: no metadata, signature, catalog
  contents, risk matrix, or feasibility tables. Those need a format
  version bump.
- Every field is required except `digits` on a segment; missing or `null`
  both mean `None`. Every list is present; `catalog_schemes` may be `[]`.
  Declaration order is meaningful, as in `MethodologyDef`.
- Decoding is strict: unknown, missing, repeated, and mistyped fields,
  numbers outside the Rust type's range (`min`/`max` are `u16`,
  `digits`/`required` are `u8`), and trailing non-whitespace are
  `PackError::Decode` with a 1-based line and column (never the parser's
  message, which can quote input). An object written as a JSON array of
  its field values, which serde's derived struct visitor accepts by
  default, is also a `Decode` error: a private `Object<T>` wrapper asks
  the parser for a map.
- `format` and `version` are checked as soon as they are read, with the
  same meaning and text as `StoreError::UnsupportedFormat`: a wrong name,
  or version 0, negative, or fractional, is `{ version: None }`; an
  integer version above 1 is `Some(v)`.
- The writer (`MethodologyDef::write_pack`) uses `serde_json`'s pretty
  printer (2-space indent) in the field order above, leaves out `digits`
  when it is `None`, writes non-ASCII as raw UTF-8, and ends with `\n`.
  The same definition always gives the same bytes (P4). The shipped pack
  and the second test methodology's pack
  (`tests/fixtures/packs/med-device-demo.json`) are byte-identical to the
  writer's output (tests `builtin_pack_is_canonical`,
  `second_pack_is_canonical`). `packs/.gitattributes` sets `* -text`, so a
  Windows checkout keeps the bytes.
- The public `*Def` types get no serde derives. Private `*Wire` types in
  `pack.rs` mirror them with bounded leaves, then move their `Arc<str>`
  values into the `Def` (no string is copied). Writing serializes borrowed
  views straight from the `Def`.

### Shared decoding with the store

`BoundedStr`, `BoundedVec`, `Cap`, `Format`/`FormatVersion`,
`CappedReader` (now parameterized by its cap), and the fault side channel
in `src/store/wire.rs` became `pub(crate)`. A second entry point,
`decode_pack`, runs a decode without a methodology context (no pack wire
type resolves vocabulary keys) and maps a recorded fault to `PackError`.
The thread-local fault cell is shared and is cleared before and taken
after each decode, so a fault never leaks between a store decode and a
pack decode.

### Decode caps (R3, R6)

Each cap is checked while decoding, inside the serde visitor, before
`Methodology::new` runs, and each is at least the model's own limit, so
every definition that `new` accepts round-trips (AC7, and `round_trip`
over random definitions up to every maximum count).

| What | Cap | Error |
|---|---|---|
| whole file | `MAX_PACK_FILE_BYTES` = 4 MiB (capped reader, pulls at most cap + 1 bytes) | `Limit::PackFileBytes { max }` |
| methodology key, every vocabulary key, every `violates` entry | 64 bytes | `Limit::KeyBytes` |
| methodology version | 256 bytes | `Limit::MethodologyVersionBytes` |
| every label | `MAX_LABEL_BYTES` = 1,024 bytes (256 chars of up to 4 bytes) | `Limit::LabelBytes { max }` |
| catalog prefix | 16 bytes | `Limit::CatalogPrefixBytes { max }` |
| `impact_categories` | 32 | `Limit::ImpactCategories { max }` |
| `levels` (each category) | 16 | `Limit::Levels { max }` |
| `properties`, each `violates` | 64 | `Limit::Properties` |
| `threat_categories` | 64 | `Limit::ThreatCategories { max }` |
| `catalog_schemes` | 16 | `Limit::CatalogSchemes { max }` |
| `segments` | 4 | `Limit::CatalogSegments { max }` |

`Limit` is carried by both `StoreError::LimitExceeded` and
`PackError::LimitExceeded`. Every new variant's `Display` names the cap and
its value, never the input.

**The largest valid pack.** A pack is largest when every count is at its
maximum, every key is 64 bytes, and every label and the version are made of
control chars, which JSON escapes to 6 bytes each (`\u0001`):

| Part | Arithmetic | Bytes |
|---|---|---|
| labels | (1 methodology + 32 categories + 32 x 16 levels + 64 properties + 64 threat categories + 16 schemes) = 689 labels x 256 chars x 6 bytes | 1,058,304 |
| `violates` | 64 threat categories x 64 entries x (10 indent + 66 quoted key + `,` + `\n`) | about 319,500 |
| version | 64 chars x 6 bytes | 384 |
| keys and structure | 689 keys of 64 bytes plus field names, indentation, and 16 syntaxes | about 103,000 |
| **total, measured** | `maximal_pack_loads_below_the_file_cap` in `tests/pack_limits.rs` (`max_def()`) | **1,481,198 (1.41 MiB)** |

The 4 MiB cap leaves 2.8x headroom over the largest valid pack, for
whitespace a hand author adds, and stays small enough that a hostile file
costs at most a few MiB. A def that passes the caps but not the model can
be larger when written (1,024-byte labels of control chars would be about
4.2 MB); such a def is invalid, and the round-trip guarantee covers only
valid ones.

### Bounded key text in errors

`MethodologyError::InvalidKey` and `UnknownViolatedProperty.property` are
the only variants that keep key text that was not validated. Every other
key field has passed `check_key` first, so it is at most 64 bytes. Both go
through one helper, `error_key_text`:

- a key of at most 64 bytes is kept as is (`Arc::clone`, no allocation);
- a longer key becomes its longest char-boundary prefix of at most 61
  bytes plus `...` (one bounded allocation, on the error path only). `.` is
  not a key char, so a cut text never looks like a valid key.

`check_key` now tests `len() <= MAX_KEY_LEN` before it scans the bytes, so
an oversized key is rejected in O(1). With keys escaped by `{:?}` (at most
6 bytes per byte, for control chars), every such error's `Display` stays
under 512 bytes; the tests feed 1 MiB keys of ASCII, control chars, and
4-byte chars in every key position (`methodology_error_key_text_*`).

`MethodologyDef` built in Rust is not capped (it is trusted input from the
caller), so this bounding matters for definitions built by tools, not only
for packs.

### Complexity (A8)

| Function | Time | Space |
|---|---|---|
| `MethodologyDef::read_pack` | O(B), B = input bytes | O(B): at most 4 MiB in, at most about 1.4 MB of definition |
| `Methodology::read_pack` | O(B + m log m + text), m <= 689 entries | O(B) |
| `MethodologyDef::write_pack` | O(m + text) | O(1) besides the 8 KiB `BufWriter`; no copy of the def |
| `Methodology::iso21434_r155` | `read_pack` on 3.8 KB, m = 40 | O(m) |

## Open question 1: pack identity and trust (owner answer, 2026-10-22)

A pack is identified by its `key` and `version` only, as project files
already record them (ADR 0009). There is no signature and no content hash
in this job, so no new dependency and the project format stays v1.

**Risk, accepted by the owner.** Two different pack files can carry the
same key and version, for example when a customer edits a pack and does not
bump the version. A project file would then load under a pack whose keys
mean something else: `severe` might now be the second level of four
instead of the fourth, or `tampering` might violate another property. The
load still succeeds whenever every key resolves, so the change is silent.
Packs are also unsigned, so anyone who can write the pack file can change
what every project under it means.

Signing, content fingerprints, and who may install a pack belong to the
server's pack registry job (direction-v1 section 2, "Admin work …
methodology packs"). The rejected alternative for now was a content hash
written into project files next to the methodology version: it needs a hash
dependency and a project format version 2.

The built-in pack is not exposed to this risk on disk: it is compiled in.

## Tests and fuzzing

- `tests/builtin_pack.rs`: the shipped pack decodes to the old
  hand-written definition and equals `Methodology::new(expected)`; it is
  canonical; the vocabulary, STRIDE-mapping, and `r155-annex5` syntax
  tests moved here from the unit tests of `iso21434.rs`.
- `tests/pack.rs`: the second methodology as a pack (it loads equal to
  `second_def()`, every `Op` variant applies over it, and a project saved
  under the hand-built methodology loads under the pack); the strict
  format, each case with its exact error and line.
- `tests/pack_limits.rs`: every cap at the cap and at cap + 1, the key cap
  in all seven key positions, the label cap in all six label positions;
  the file cap stops an endless reader after at most cap + 1 bytes; the
  maximal pack.
- `tests/pack_properties.rs`: `round_trip` over random valid definitions
  (1 entry up to every maximum, non-ASCII and escape-heavy labels,
  optional `digits`); `hostile_*` over arbitrary bytes and over valid packs
  that are cut short or have bytes flipped. Labels and keys are drawn as a
  short char pattern plus a length, which keeps a maximal random
  definition cheap to generate (the suite takes about 3 s in debug).
- Fuzz target `pack` (`fuzz/run.sh 30 pack`): any input fails with a
  `PackError` or loads, and a loaded definition writes and reads back
  equal with deterministic bytes. Seeds are the two packs. A 31 s run did
  1.8 million executions without a finding.

## Numbers

`cargo bench --bench pack` (divan, 100 samples, Apple M2 Max, release
profile), medians:

| Bench | Input | Median | Throughput |
|---|---|---|---|
| `iso21434_r155` | built-in pack, 3,794 bytes | 14.5 µs | 262 MB/s |
| `decode_pack_max` (`MethodologyDef::read_pack`) | maximal pack, 1,481,198 bytes | 3.37 ms | 439 MB/s |
| `read_pack_max` (`Methodology::read_pack`) | maximal pack | 3.50 ms | 423 MB/s |
| `write_pack_max` | maximal def | 465 µs | 3.2 GB/s |

The lesson (L2): validation (`Methodology::new`) adds about 4% to reading
the maximal pack; the decoder's escape handling dominates. The built-in
costs 15 µs per call, so decoding it on every call instead of caching it is
the simpler choice (P3).

## Dependency notes (L6)

None added. The reader and writer use `serde` (derive, `Deserializer`
forwarding with `forward_to_deserialize_any!`) and `serde_json` (the
pretty printer and `Deserializer::from_reader`), both already
dependencies. `Cargo.toml` gains only the `[[bench]] name = "pack"` entry;
the fuzz crate gains one `[[bin]]`, with no lock file change.

## Consequences

- Adding a methodology is writing one pack file; no `src` file names the
  second test methodology (`grep -rn 'med-device' src` is empty).
- Every file the crate reads (snapshot, op log, pack) now shares one
  strict, capped decoding approach and one fault channel.
- `FORMAT_VERSION` (1) is shared by the three formats. When one of them
  moves to version 2, `FormatVersion` must take the version per format.
- Pack identity is key + version only until the pack registry job adds
  signing or fingerprints (open question 1).
- Migration of a project to another pack or pack version stays out of
  scope.
