# ADR 0008: Methodology-neutral domain model

- Status: proposed (job 009-domain-model-v0-2)
- Date: 2026-10-21
- Crate: `crates/aegis-model` (`src/methodology/`, `src/asset.rs`,
  `src/damage.rs`, `src/threat.rs`, `src/op.rs`, `src/project.rs`)
- Rules: P4, P5, D2, D3, D4, D5, D7, D9, D10, D12, A8, A9, Q3, Q4, Q5, Q6,
  L1 in `docs/engineering-standards.md`
- Builds on: `0001-arena-and-typed-ids.md` (arenas, typed indices, UUIDv7
  public IDs)

## Context

Part 2 of the domain model adds the TARA core next to the attack trees:
assets with security properties, damage scenarios rated per impact
category, and threat scenarios that target an asset, cite catalog
entries, and link to damage scenarios.

The owner's principle (job 009 `rework.md`, 2026-10-21):

> Make the data structures in a way that is easy to adopt different
> methodologies even if it's hard to implement initially. We are not in
> rush to release. Maybe the v0 can accommodate iso21434 but the base shall
> set up for other methodologies.

So the core must not hard-code ISO/SAE 21434 vocabulary as Rust enums: no
fixed impact categories, rating scales, security properties, threat
taxonomy (STRIDE), or catalog scheme (UN R155 Annex 5). ISO/SAE 21434 +
UN R155 ships as the first methodology, but as data over neutral types.
The test of the design: a second methodology must be a data change, never
a code change.

## Options

### Vocabulary as enums or as data

| Option | Good | Bad |
|---|---|---|
| A. Rust enums (`enum ImpactCategory { Safety, Financial, .. }`) | The compiler checks every match; no index validation; a damage scenario is a fixed `[Option<Level>; 4]` | Every new methodology is a code change and a release; packs cannot add vocabulary; the core speaks one standard |
| **B. A `MethodologyDef` (plain data) validated into a `Methodology` that hands out typed `u8` indices** | A second methodology is data; the same path serves the built-in and, later, pack files; indices stay 1 byte | Every op must check indices against the project's methodology; a damage scenario needs a `Box<[Option<Level>]>` sized at run time; an index from one methodology can be passed to a project of another (caught at run time, not compile time) |
| C. String keys everywhere | Simple to read; no index tables | A string compare and allocation per check; larger ops and entities |

**Chosen: B.** The cost is real but bounded. Validation is O(1) per
index (a range check) and O(p) per property set with p <= 64, so no op
becomes O(project size). The boxed impacts slice costs one allocation per
damage scenario (16 bytes of header plus 2 bytes per category). The
"wrong methodology" risk is the reason every op re-checks indices and
every `CatalogRef` against the project's methodology (defense in depth;
`tests/second_methodology.rs` exercises it with indices from the built-in
methodology).

### The built-in methodology goes through the same path

`Methodology::iso21434_r155()` builds a `MethodologyDef` and calls
`Methodology::new`. It never sets fields directly, so it obeys every rule
a pack will, and `src/methodology/iso21434.rs` is the only non-test file
that names its vocabulary. Labels are short names only, with no text or
tables from the standards (direction-v1 section 11).

## Decision

### Methodology

`MethodologyDef` has public fields and no `#[non_exhaustive]`: impact
categories (each with an ordered scale, lowest first), properties, threat
categories (each with the property keys it violates), and catalog schemes
(each with an ID syntax). Strings are `Arc<str>` (D5).

`Methodology::new` checks, each rule with its own `MethodologyError`
variant naming the offending key: key syntax `[a-z][a-z0-9_-]{0,63}` and
uniqueness per vocabulary (and per scale for levels); labels non-blank and
at most 256 chars; version 1 to 64 chars; vocabulary sizes; `violates`
non-empty, declared, no repeats; and the catalog syntax rules below.
Lookups by key use `BTreeMap<Arc<str>, u8>`, O(log m).

The vocabulary index types `ImpactCategory`, `Level`, `Property`,
`ThreatCategory`, and `CatalogScheme` wrap a `u8` and have no public
constructor. `Level`'s `Ord` is the scale order within one category;
comparing levels of different categories has no meaning.

### Vocabulary limits and why

| Vocabulary | Limit | Why |
|---|---|---|
| impact categories | 1 to 32 | `u8` index; a `u32` bitmask detects repeated categories in an op in O(c) |
| levels per scale | 2 to 16 | a 1-level scale cannot rate anything; 16 covers every scale we know of |
| properties | 1 to 64 | `PropertySet` is one `u64` |
| threat categories | 1 to 64 | `u8` index; STRIDE has 6, LINDDUN 7, CAPEC mechanisms 9 |
| catalog schemes | 0 to 16 | a methodology may cite no catalog |
| catalog refs per threat | 0 to 16 | Open question 3 |

### Catalog IDs: a declarative grammar, not a regex

A `CatalogSyntax` is an exact prefix (0 to 16 ASCII letters, `-`, `_`;
never a digit, so the boundary to the first segment is unambiguous) and 1
to 4 numeric segments joined by `.`, of which the first `required` must be
present. A segment has a range `min..=max` and either a fixed digit count
`Some(w)` (zero-padded, `max < 10^w`) or `None` (1 to 5 digits, no leading
zero). A `CatalogRef` is `{ scheme: u8, len: u8, segments: [u16; 4] }`,
`Copy`, 10 bytes. Parsing rejects whitespace, signs, and empty parts.
`Ord` compares the scheme, then the used segments lexicographically, so
`4 < 4.1 < 4.2 < 5`.

| Catalog | Example | Syntax |
|---|---|---|
| UN R155 Annex 5 | `4`, `4.1` | prefix `""`, `[{1..=32, None}, {1..=99, None}]`, required 1 |
| CAPEC | `CAPEC-123` | prefix `"CAPEC-"`, `[{1..=9999, None}]`, required 1 |
| MITRE ATT&CK | `T1059`, `T1059.001` | prefix `"T"`, `[{1..=9999, Some(4)}, {1..=999, Some(3)}]`, required 1 |

| Option | Good | Bad |
|---|---|---|
| **Declarative grammar** | No new dependency; IDs parse into numbers, so they sort numerically (`4.2 < 4.10`) and fit in 10 bytes; a pack declares data, not code; the grammar is small enough to validate fully at load time | Cannot express arbitrary forms (letters inside segments, for example EMB3D `TID-101` works but `CVE-2024-1234` needs a 2-segment `-` separator we do not have) |
| `regex` crate | Any form | A new dependency (L6, Q7); IDs stay strings, so they sort as text (`4.10 < 4.2`) and need an `Arc<str>` each; a regex in a pack is code-like input that needs size and complexity limits |

**Chosen: the grammar.** If a catalog needs another separator later, the
grammar can gain a per-segment separator field without changing stored
refs.

### Entities and storage

- `Asset { name, properties: PropertySet }`; the set is non-empty and
  declared.
- `DamageScenario { name, impacts: Box<[Option<Level>]> }`, one slot per
  impact category; `None` is *unrated*, kept apart from the lowest level.
- `ThreatScenario { name, asset: AssetIdx, category, catalog_refs,
  damages }`. The targeted properties are not stored: `Project::targets`
  returns `methodology.violates(category)`, so the two cannot disagree
  (D12). `catalog_refs` is sorted and unique; `damages` is sorted by the
  target's `DamageId` and unique. Neither order depends on edit history
  (P4).

Per entity kind the project keeps an arena `Vec<Option<T>>`, a parallel
`Vec` of public IDs, and a `BTreeMap<XId, XIdx>`.

### Tombstones, not `swap_remove`

A removed entity leaves `None` in its slot, and slots are not reused in
v0. `swap_remove` would keep the arena dense, but it moves the last entity
into the hole, so every reference to the moved entity must be rewritten:
a moved asset is referenced by its threats, a moved damage scenario by
every threat that links it. Finding them is O(threats) without a reverse
index, which breaks "edits cost O(what changed)" (A9). Tombstones cost one
`Option` niche (0 bytes for these types, measured below) and the memory of
dead slots until a later compaction job (which would also be the place to
reuse slots).

### `BTreeMap` ID maps (D7)

Iteration over entities reaches output (reports, files, hashes), so it
must not depend on hash order. The `BTreeMap` gives public-ID order for
free and O(log n) lookups. `AttackTree` uses a `HashMap` because its
iteration order is the arena order; here the arena order depends on edit
history (tombstones), so it must not reach output.

### Reference counts, not scanning

The "in use" checks (remove an asset or damage scenario, drop a property)
would scan every threat: O(threats) per op. Instead the project keeps
derived counts, marked as a cache (D10): a flat `Vec<u32>` of asset x
property counts with row length = property count (D4), a `Vec<u32>` of
threats per asset, and a `Vec<u32>` of threats per damage scenario. Every
op updates them in O(p + k). A lib-level proptest recounts them from the
threats after every op (`check_invariants`).

### Typed ops and inverse ops for undo (D9)

Every change is an `Op` (16 variants). `validate(&Op)` runs every check
without changing anything; `apply(Op)` runs the same checks, then commits
and returns the inverse op. Every fallible lookup in a commit happens
before its first mutation, so even an unexpected error leaves the project
unchanged. No op is O(project size).

| Option | Good | Bad |
|---|---|---|
| **Inverse ops** | Undo costs O(size of the op); the inverse is also the shape the later op log stores | Every op needs a correct inverse; proven by AC10 and the proptests |
| Persistent snapshots (`imbl`) | Undo is "restore the snapshot", O(1); good for what-if branches | A new dependency; every mutation pays the structural-sharing cost; out of scope for v0 |
| Clone the project | Trivial | O(n) per edit: the spike's mistake (standards section 3) |

Snapshots stay a later option for what-if branches (D9); they do not
conflict with inverse ops.

A project also has a slot limit (`Project::with_slot_limit`, default
`u32::MAX`): an add beyond it fails with `ProjectCapacityExceeded`. It
keeps every index in `u32`, bounds projects built from untrusted input
(R6), and makes the capacity error testable.

## Open questions and outcomes

1. **Methodology lifetime.** Confirmed: fixed at creation.
   `Project::new(Arc<Methodology>)`; no op changes it. Migration to a new
   pack version or another methodology is a later job (map old vocabulary
   to new and show the effect first). The owner's rule "reject removing a
   property or category from use while a threat relies on it" therefore
   applies through `SetAssetProperties` and `SetThreatCategory`.
2. **How ops name vocabulary.** Confirmed: typed `u8` indices in memory,
   resolved once from keys (Q4). The later serde and op-log job writes the
   stable keys on the wire and in files, never indices, so reordering a
   pack never changes the meaning of stored ops (the D3 pattern: stable key
   outside, index inside).
3. **Catalog refs per threat.** Confirmed: a sorted set of 0 to 16 refs
   from any declared scheme, so one threat can cite R155, CAPEC, and
   ATT&CK. Zero refs is "unmapped".

## A second methodology, as data only

Here is a sketch of a second methodology, in the style of ETSI TS 102 165-1
(TVRA). It is declared as a `MethodologyDef` and needs no code change:

```rust
MethodologyDef {
    key: "etsi-tvra".into(),
    version: "0.1.0".into(),
    label: "ETSI TVRA (sketch)".into(),
    impact_categories: vec![ImpactCategoryDef {
        key: "impact".into(),
        label: "Impact".into(),
        levels: ["low", "medium", "high"].map(level).into(),
    }],
    properties: ["confidentiality", "integrity", "availability",
                 "authenticity", "accountability"].map(property).into(),
    threat_categories: vec![
        threat("interception", &["confidentiality"]),
        threat("manipulation", &["integrity", "authenticity"]),
        threat("denial-of-service", &["availability"]),
        threat("repudiation", &["accountability"]),
    ],
    catalog_schemes: vec![scheme("capec", "CAPEC-", &[(1, 9999, None)], 1)],
}
```

The test suite holds a real one: `tests/common/mod.rs` declares a
medical-device-style methodology (three impact categories with 3-, 5-,
and 2-level scales; only C, I, and A; a `ransomware` category that
violates two properties; a CAPEC scheme), and `tests/second_methodology.rs`
plus the proptests run every op over it. No library file changed for it.

What would still need code:

- **Crosswalks** between methodologies (for example STRIDE to TVRA threat
  families) and **migration** of a project to another pack version: they
  map vocabulary, which is logic over two definitions.
- **Scoring formulas**: risk = f(impact, feasibility) differs per
  methodology (21434 risk matrices, TVRA's likelihood x impact). A
  declarative risk matrix is a likely next data type; anything beyond a
  table is code.
- **Feasibility models** (attack potential, CVSS-like vectors), which the
  attack-tree side computes.
- **Catalog contents** and ID forms the grammar cannot express (see the
  catalog table).

## Complexity

n = entities of the kind, k = damage links or catalog refs on one threat,
p <= 64 properties, c <= 32 impact categories, m = methodology size.

| Operation | Time |
|---|---|
| `Methodology::new` | O(m log m + text) |
| key lookup | O(log m) |
| `parse_catalog_ref` | O(text) |
| add / rename / remove asset | O(log n + p) |
| `SetAssetProperties` | O(log n + p) |
| add / remove damage scenario | O(log n + c) |
| `SetImpact` | O(log n) |
| `AddThreatScenario` | O(k log n + k log k + p) |
| `RemoveThreatScenario` | O(log n + k + p) |
| `SetThreatCategory` | O(log n + p) |
| add/remove catalog ref, link/unlink damage | O(log n + k) |
| iterators | O(1) per item |
| `PartialEq`, `Clone`, `check_invariants` | O(n + L + m), L = links and refs |

## Numbers

Measured with `std::mem::size_of` on aarch64-apple-darwin, Rust 1.99
(`value_sizes` and `property_set_size` assert the first three rows):

| Type | Bytes |
|---|---|
| `PropertySet` | 8 |
| `CatalogRef` | 10 |
| each vocabulary index | 1 |
| `Option<Level>` | 2 |
| `AssetIdx` / `AssetId` | 4 / 16 |
| `Asset` (and `Option<Asset>`) | 24 |
| `DamageScenario` (and `Option<..>`) | 32 + 2 per category on the heap |
| `ThreatScenario` (and `Option<..>`) | 72 + 10 per ref + 4 per link on the heap |
| `Op` | 104 |

Tombstones are free in slot size: `Option<T>` uses the niche in `Arc<str>`.

The proptests run 256 sequences of up to 64 ops per methodology. Over
those, about 36% of generated ops succeed for the built-in methodology
and 33% for the second one, every one of the 16 op variants succeeds at
least once, and 25 of the 26 new error variants occur (the capacity error
is covered by a unit test with a slot limit).

## Consequences

- A second methodology is a data change. Pack loading (serde) can
  deserialize straight into `MethodologyDef` and reuse `Methodology::new`.
- Indices are only meaningful with their methodology; anything that stores
  ops or entities must write keys, not indices (Open question 2).
- Removing an entity leaves a tombstone; long-lived projects with heavy
  churn will want compaction, which must remap indices in one O(n + L)
  pass.
- The neutrality check (AC15) greps `src` for automotive words outside
  `src/methodology/iso21434.rs`. The module declaration `mod iso21434;` in
  `src/methodology/mod.rs` necessarily names that file and is the one line
  the grep reports; it is a file name, not vocabulary.
