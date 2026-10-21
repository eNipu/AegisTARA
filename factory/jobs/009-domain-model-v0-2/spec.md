# 009-domain-model-v0-2: Domain model v0, part 2: assets, damage and threat scenarios, typed operations, over a methodology-neutral core

The owner's answers are in `factory/jobs/009-domain-model-v0-2/rework.md`, and they shape this spec. The core model is methodology-neutral. A `Methodology` value declares the vocabularies. ISO/SAE 21434 + UN R155 is the first methodology, written as data over the neutral types. A second methodology must need a data change only, never a code change.

## Open questions

Each question has a recommended default. If the owner says nothing, the builder implements the default.

1. **A project's methodology is fixed for its lifetime.** `Project::new` takes an `Arc<Methodology>`, and no op changes it. Moving a project to a newer pack version, or to another methodology, would be a later migration job: it would map old vocabulary to new and show the effect first (brainstorm "Crosswalks, pinning, and migration"). **Default: fixed.** The other option is an op that swaps the methodology now, which needs a remapping design first.
2. **How ops name vocabulary items.** In memory, ops carry small typed indices (`ImpactCategory(u8)`, `Level(u8)`, and so on) into the project's methodology. The text at the boundary is resolved once, through `Methodology::impact_category("safety")` and similar lookups (Q4, parse, don't validate). The later serde and op-log job writes the stable **keys** (`"safety"`, `"major"`) on the wire and in files, never the indices, so a reordered pack cannot change the meaning of stored ops (D3 pattern: stable key outside, index inside). **Default: indices in memory, keys on the wire.** The other option is keys in memory too: simpler to reason about, but each op validation then does a string lookup.
3. **A threat holds a set of catalog references, not one.** The owner said "optional". This spec generalises that to a sorted set of 0 to 16 refs, each from any scheme the methodology declares, because the AI backlog cites several catalogs per threat (R155, CAPEC, ATT&CK, EMB3D). For 21434 + R155, an empty set is "unmapped". **Default: a set of at most 16.** The other option is `Option<CatalogRef>`, exactly as answered.

Small choices settled in this spec, which the owner does not need to decide:
- A threat category may violate one *or more* properties (`PropertySet`). In the built-in methodology every STRIDE category violates exactly one.
- The catalog ID syntax is a small declarative grammar, not a regex, so the crate needs no new dependency.
- Each vocabulary has a size limit (see Design notes).

## Goal

A developer can build the core of a TARA in memory, in `aegis-model`. The core has no methodology vocabulary built in. A `Methodology` value declares it:

- impact categories, each with an ordered rating scale;
- security properties;
- threat categories, each mapped to the properties it violates;
- catalog schemes, each with its ID syntax.

ISO/SAE 21434 + UN R155 ships as the first methodology, built in code through the same `Methodology::new(def)` path that a pack file will use later. On top of that, a project holds three kinds of entity:

- **assets**, each with security properties;
- **damage scenarios**, each rated per impact category, where "unrated" is kept separate from the lowest level;
- **threat scenarios**, each with a target asset, a threat category (the targeted properties are derived from it), catalog references, and links to damage scenarios.

Every change is a typed `Op`. `Project::apply` checks the op against the project's methodology and its referential rules, and only then changes the model. On error nothing changes. On success `apply` returns the inverse op, so undo costs O(size of the op). A test declares a second, non-automotive methodology purely as data and runs the same op suite against it.

## Scope

In:
- Stable public IDs `AssetId`, `DamageId`, `ThreatId` (UUIDv7) and arena indices `AssetIdx`, `DamageIdx`, `ThreatIdx` (u32) (D2, D3).
- `MethodologyDef`: plain, pack-shaped data. `Methodology`: the validated form, holding the typed vocabulary indices, lookup by key, and catalog syntax parsing and formatting. `MethodologyError` for invalid definitions.
- The built-in `Methodology::iso21434_r155()`.
- `PropertySet`, `CatalogRef`, and the `Asset`, `DamageScenario`, `ThreatScenario` views.
- `Project`: `new(Arc<Methodology>)`, the read API, `validate(&Op)`, `apply(Op) -> Result<Op, ModelError>`. New `ModelError` variants.
- Unit tests, integration tests, a test for a second methodology, proptests over both methodologies, and doctests.
- ADR `docs/adr/0008-methodology-neutral-domain-model.md` (L1).

Out:
- Pack file loading and serde. The types must allow them later (plain data, no closures, no trait objects).
- Provenance (D6), classification labels, facets, and variant conditions.
- The op log and persistence.
- Item definition and system elements. Links from threats to attack trees, risk values, and feasibility.
- Migrating a project between methodology versions, and crosswalks.
- Batched ops, persistent snapshots (`imbl`), and lint rules.
- Shipping any catalog contents (only the R155 ID syntax ships).
- Changes to the `AttackTree`, evaluation, or explain-why code.

## Design notes

Crate: only `crates/aegis-model/`. No new dependencies. Arrange modules by domain concept (Q6):

| File | Contents |
|---|---|
| `src/id.rs` | Add the three ID pairs through one private `macro_rules!` or shared helper, so the UUIDv7 check lives in one place. Keep `NodeId` and `NodeIdx` behaving the same as now. |
| `src/methodology/mod.rs` | `MethodologyDef` and its parts, `Methodology`, the vocabulary index types, `Methodology::new`, and lookups. |
| `src/methodology/catalog.rs` | `CatalogSyntax`, `SegmentDef`, `CatalogRef`, parsing, and `Display` through the methodology. |
| `src/methodology/iso21434.rs` | `Methodology::iso21434_r155()`. **The only non-test file that may name 21434 or R155 vocabulary.** |
| `src/asset.rs` | `PropertySet` and `Asset`. |
| `src/damage.rs` | `DamageScenario`. |
| `src/threat.rs` | `ThreatScenario`. |
| `src/op.rs` | `Op`. |
| `src/project.rs` | `Project`, `validate`, `apply`, the read API, and `check_invariants` (pub(crate)). |
| `src/error.rs` | New `ModelError` variants and the new `MethodologyError`. Existing variants and their Display texts stay unchanged. |
| `src/lib.rs` | Re-exports, plus a crate-doc section "Methodologies" with an example. |

### Methodology definition (data a pack will provide)

The definition types are plain structs with public fields and no `#[non_exhaustive]`, so tests and a later serde layer can build them. Strings are `Arc<str>` (D5).

```rust
pub struct MethodologyDef {
    pub key: Arc<str>, pub version: Arc<str>, pub label: Arc<str>,
    pub impact_categories: Vec<ImpactCategoryDef>,   // { key, label, levels: Vec<LevelDef> } lowest first
    pub properties: Vec<PropertyDef>,                // { key, label }
    pub threat_categories: Vec<ThreatCategoryDef>,   // { key, label, violates: Vec<Arc<str>> }  property keys
    pub catalog_schemes: Vec<CatalogSchemeDef>,      // { key, label, syntax: CatalogSyntax }
}
pub struct LevelDef { pub key: Arc<str>, pub label: Arc<str> }
pub struct CatalogSyntax { pub prefix: Arc<str>, pub segments: Vec<SegmentDef>, pub required: u8 }
pub struct SegmentDef { pub min: u16, pub max: u16, pub digits: Option<u8> }
```

`Methodology::new(def) -> Result<Methodology, MethodologyError>` checks the rules below. Each check has its own error variant, named by key. Keys are vocabulary, not project data.

- **Keys:** they match `[a-z][a-z0-9_-]{0,63}` and are unique within their vocabulary, and level keys are unique within their category. A threat category's `violates` is non-empty, names declared properties only, and has no repeats.
- **Text fields:** labels are non-empty after trimming and at most 256 chars. `version` is 1 to 64 chars.
- **Limits** (chosen so the index types stay `u8` and `PropertySet` stays a `u64`):
  - impact categories: 1 to 32;
  - levels per scale: 2 to 16;
  - properties: 1 to 64;
  - threat categories: 1 to 64;
  - catalog schemes: 0 to 16.
- **Catalog syntax:** 1 to 4 segments, `1 <= required <= segments.len()`, and `min <= max` in every segment. When `digits` is `Some(w)`, `w` is 1 to 5 and `max < 10^w`. The prefix is 0 to 16 ASCII letters, `-`, or `_` (it never ends in a digit, so parsing is unambiguous). Segments are separated by `.`.

`Methodology` keeps the validated data in private fields and derives `Clone`, `Debug`, `PartialEq`, and `Eq`. It provides:

- `key()`, `version()`, `label()`;
- iterators over each vocabulary in declaration order;
- lookup by key (`impact_category(&str) -> Option<ImpactCategory>`, `level(ImpactCategory, &str) -> Option<Level>`, `property`, `threat_category`, `catalog_scheme`);
- labels and keys by index;
- `levels(cat)`;
- `violates(ThreatCategory) -> PropertySet`;
- `parse_catalog_ref(CatalogScheme, &str) -> Result<CatalogRef, ModelError>`;
- `display_catalog_ref(&CatalogRef) -> impl Display`.

Key lookups use `BTreeMap<Arc<str>, u8>`, O(log m), where m is the vocabulary size.

**Vocabulary indices** `ImpactCategory(u8)`, `Level(u8)`, `Property(u8)`, `ThreatCategory(u8)`, and `CatalogScheme(u8)` have no public constructor; only a `Methodology` hands them out. They derive `Copy`, `Eq`, `Ord`, `Hash`, and `Debug`. A `Level` is a position in one category's scale, so its `Ord` is the scale order (lowest = 0). Comparing levels across categories has no meaning, and the docs say so.

**`PropertySet(u64)`**: `empty`, `FromIterator<Property>`, `with`, `contains`, `is_subset`, `iter` (ascending), `len`, `is_empty`. It takes 8 bytes. It has no raw-bits constructor.

**`CatalogRef { scheme: CatalogScheme, len: u8, segments: [u16; 4] }`**: it is `Copy` and at most 12 bytes. It can be built only by `parse_catalog_ref`.

- **Parsing:** read the exact prefix, then `len` dot-separated segments, where `required <= len <= segments.len()`. A segment with `digits: Some(w)` must have exactly `w` digits. A segment with `digits: None` is 1 to 5 digits with no leading zero. Each value must lie in `min..=max`. Parsing rejects whitespace, signs, and empty parts with `ModelError::InvalidCatalogId { scheme }`.
- **Ordering:** a manual `Ord` compares `scheme`, then `segments[..len]` lexicographically, so `4 < 4.1 < 4.2 < 5`. Unused segments are always 0, so a derived `Eq` is correct.

### Built-in methodology: `Methodology::iso21434_r155() -> Result<Methodology, MethodologyError>`

It builds a `MethodologyDef` and passes it through `Methodology::new` (it never sets fields directly). It uses short labels only, with no ISO text or tables (direction-v1 section 11).

- **Key and version:** key `iso21434-r155`, version `0.1.0`.
- **Impact categories:** `safety`, `financial`, `operational`, `privacy`. Each has the levels `negligible`, `moderate`, `major`, `severe` (lowest first).
- **Properties:** `confidentiality`, `integrity`, `availability`, `authenticity`, `authorization`, `non-repudiation`.
- **Threat categories (STRIDE), each violating exactly one property:**
  - `spoofing` violates authenticity;
  - `tampering` violates integrity;
  - `repudiation` violates non-repudiation;
  - `information-disclosure` violates confidentiality;
  - `denial-of-service` violates availability;
  - `elevation-of-privilege` violates authorization.
- **Catalog scheme:** `r155-annex5` (label "UN R155 Annex 5"), with an empty prefix and the segments `[{1..=32, None}, {1..=99, None}]`, `required: 1`. So `N` or `N.M`.

### Entities and project storage

- **`Asset { name: Arc<str>, properties: PropertySet }`**. The property set is non-empty, and every member is declared.
- **`DamageScenario { name: Arc<str>, impacts: Box<[Option<Level>]> }`**. It has one slot per impact category of the methodology, and `None` means unrated. Getters: `impact(cat) -> Option<Level>` and `impacts() -> impl Iterator<Item = (ImpactCategory, Option<Level>)>`.
- **`ThreatScenario { name, asset: AssetIdx, category: ThreatCategory, catalog_refs: Vec<CatalogRef>, damages: Vec<DamageIdx> }`**. `catalog_refs` is sorted with no duplicates and holds at most 16 refs. `damages` is sorted by the target's `DamageId` with no duplicates, so neither order depends on edit history (P4). The targeted properties are derived: `Project::targets(ThreatIdx) -> Option<PropertySet>` returns `methodology.violates(category)` (D12: they can never disagree).
- **Project storage:**
  - `Arc<Methodology>`.
  - Per kind, an arena `Vec<Option<T>>`. A removed slot becomes `None` (a tombstone) and is not reused in v0.
  - A parallel `Vec` of public IDs.
  - A `BTreeMap<XId, XIdx>`. Iteration order reaches output, so D7 calls for a `BTreeMap`. Lookups are O(log n).
  - Derived reference counts, marked as a cache (D10). A flat `Vec<u32>` holds asset x property counts, with stride = property count (D4). Another `Vec<u32>` holds the number of linking threats per damage scenario.
- **Read API:**
  - `methodology()`;
  - per kind, `x(idx)`, `x_idx(&XId)`, `x_id(idx)`, and a count;
  - `assets()`, `damage_scenarios()`, and `threat_scenarios()`, which iterate in public-ID order, skip tombstones, and yield `(XIdx, XId, &X)`.
- **`impl PartialEq + Eq for Project`** is logical equality. The methodologies are equal (`Arc::ptr_eq` or content), and the projects hold the same IDs per kind with equal content: threat references are compared by public ID, and slot layout and tombstones are ignored. Time O(n + L + m), where n is the number of entities, L the number of damage links and catalog refs, and m the methodology size. Derive `Clone` and `Debug`.

### Operations

`Op` derives `Clone`, `Debug`, `PartialEq`, and `Eq`, and is `#[non_exhaustive]`. Entities are referenced by public ID, and vocabulary by typed index (Open question 2). k is the number of damage links or catalog refs on the threat. No op is O(n) in project size (A9).

| Op | Inverse | Cost |
|---|---|---|
| `AddAsset { id, name, properties }` | `RemoveAsset { id }` | O(log n) |
| `RenameAsset { id, name }` | the same op with the old name | O(log n) |
| `SetAssetProperties { id, properties }` | the same op with the old set | O(log n + p) |
| `RemoveAsset { id }` | `AddAsset` with the old fields | O(log n + p) |
| `AddDamageScenario { id, name, impacts: Vec<(ImpactCategory, Level)> }` (unlisted categories are unrated) | `RemoveDamageScenario { id }` | O(log n + c) |
| `RenameDamageScenario { id, name }` | the same op with the old name | O(log n) |
| `SetImpact { id, category, level: Option<Level> }` | the same op with the old level | O(log n) |
| `RemoveDamageScenario { id }` | `AddDamageScenario` with the rated pairs in category order | O(log n + c) |
| `AddThreatScenario { id, name, asset, category, catalog_refs: Vec<CatalogRef>, damages: Vec<DamageId> }` | `RemoveThreatScenario { id }` | O(k log n + k log k + p) |
| `RenameThreatScenario { id, name }` | the same op with the old name | O(log n) |
| `SetThreatCategory { id, category }` | the same op with the old category | O(log n + p) |
| `AddCatalogRef { threat, catalog_ref }` | `RemoveCatalogRef` | O(log n + k) |
| `RemoveCatalogRef { threat, catalog_ref }` | `AddCatalogRef` | O(log n + k) |
| `LinkDamage { threat, damage }` | `UnlinkDamage` | O(log n + k) |
| `UnlinkDamage { threat, damage }` | `LinkDamage` | O(log n + k) |
| `RemoveThreatScenario { id }` | `AddThreatScenario` with the old fields, in stored order | O(log n + k + p) |

Here p is at most 64 properties and c at most 32 categories (constants of the methodology).

**`validate(&self, &Op) -> Result<(), ModelError>`** changes nothing. **`apply(&mut self, Op) -> Result<Op, ModelError>`** runs the same checks and then commits. The commit cannot fail, because capacity is checked during validation. The checks:

- **IDs:** add ops need an unused ID in that kind, and all other ops need an existing target.
- **Names:** non-empty after trimming, at most 256 chars.
- **Vocabulary from the project's methodology:** every vocabulary index is in range for it. Each `Level` is in range for its own category. Every `PropertySet` member is declared. Every `CatalogRef` names a declared scheme and its segments satisfy that scheme's syntax. That syntax check is defense in depth against refs parsed under another methodology.
- **Assets:** the property set is non-empty.
- **Damage scenarios:** the impacts list names each category at most once.
- **Threats:**
  - the asset exists and carries every property in `violates(category)`;
  - the damages exist and appear once each;
  - the catalog refs are unique, at most 16;
  - `SetThreatCategory` needs the asset to carry every property the new category violates.
- **Referential rules:** `SetAssetProperties` must not drop a property that a threat targets. Removing an asset or damage scenario is rejected while it is referenced (no cascade). Linking, unlinking, and adding or removing a catalog ref are rejected if the link or ref already exists, or does not exist.
- **Capacity:** arenas are limited to u32 indices.

An op that sets a value to what it already is succeeds and returns an equal inverse. The methodology itself is immutable (Open question 1). So the owner's rule "reject removing a property or category from use while a threat relies on it" applies through `SetAssetProperties` and `SetThreatCategory`.

**New `ModelError` variants.** They carry public IDs, vocabulary indices, and counts, never entity names (customer data, the same rule as today):

- Duplicates and missing targets: `DuplicateAsset`, `DuplicateDamageScenario`, `DuplicateThreatScenario`, `UnknownAsset`, `UnknownDamageScenario`, `UnknownThreatScenario`.
- Names: `EmptyName`, `NameTooLong { max }`.
- Vocabulary not in the methodology: `UnknownImpactCategory(ImpactCategory)`, `UnknownLevel { category, level }`, `UnknownProperty(Property)`, `UnknownThreatCategory(ThreatCategory)`, `UnknownCatalogScheme(CatalogScheme)`, `InvalidCatalogId { scheme }`.
- Assets and properties: `NoSecurityProperties(AssetId)`, `PropertyNotOnAsset { asset, property }`, `PropertyInUse { asset, property, threats: u32 }`.
- Removal while referenced: `AssetInUse { asset, threats }`, `DamageScenarioInUse { damage, threats }`.
- Duplicates inside an op: `DuplicateImpactCategory(ImpactCategory)`, `DuplicateDamageLink { threat, damage }`.
- Links and refs that do not exist: `DamageNotLinked { threat, damage }`, `CatalogRefNotFound { threat }`.
- Limits: `DuplicateCatalogRef { threat }`, `TooManyCatalogRefs { threat, max }`, `ProjectCapacityExceeded`.

`MethodologyError` is a separate `thiserror` enum with one variant per rule in `Methodology::new`.

**Project-sized costs:** `PartialEq`, `Clone`, `check_invariants`, and the iterators are O(n + L). Every public item states its cost (A8).

### ADR 0008

`docs/adr/0008-methodology-neutral-domain-model.md` covers:

- the owner's principle;
- why the vocabulary is data (`MethodologyDef`) and not enums, and what that costs (index validation in every op, `Box<[Option<Level>]>` per damage scenario);
- **a worked sketch of a second methodology** (for example an ETSI TVRA-style or a medical-device-style one), declared only as a `MethodologyDef`, with no code change. Name what would still need code (crosswalks, migration, scoring formulas);
- Open questions 1 to 3 and their outcomes;
- the declarative catalog grammar versus a regex dependency, shown on the R155, CAPEC, and ATT&CK ID forms;
- the vocabulary limits and why (`u8` indices, a `u64` property set);
- inverse ops versus snapshots for undo (D9);
- tombstones versus `swap_remove` (`swap_remove` forces every reference to a moved entity to be rewritten in O(threats));
- `BTreeMap` ID maps (D7);
- reference counts versus scanning;
- the measured sizes of `PropertySet`, `CatalogRef`, and the index types.

## Acceptance criteria

Run all commands from `crates/aegis-model/` unless noted.

- [ ] AC1 `AssetId`, `DamageId`, and `ThreatId` accept only UUIDv7 and round-trip through `Display` and `FromStr`. A `compile_fail` doctest shows that an `AssetId` cannot be passed where a `ThreatId` is expected. Test: `cargo test --lib ids_` and `cargo test --doc`.
- [ ] AC2 `Methodology::new` rejects, each with its own `MethodologyError` variant:
  - a duplicate key in every vocabulary, and a duplicate level key;
  - a bad key syntax, a blank or too-long label, a blank version;
  - a 1-level scale and a 17-level scale;
  - 0 or 33 impact categories, 0 or 65 properties, 0 or 65 threat categories, 17 catalog schemes;
  - a threat category with empty, unknown, or repeated `violates`;
  - catalog syntax with 0 or 5 segments, `required` of 0 or above the segment count, `min > max`, `digits` too small for `max`, or a prefix with a digit or a space.

  Test: `cargo test --lib methodology_rejects_`.
- [ ] AC3 `Methodology::iso21434_r155()` returns `Ok`, and its vocabulary matches Design notes exactly: keys, order, four levels lowest first, and each STRIDE category's `violates` is the single mapped property. It is built through `Methodology::new`. Test: `cargo test --lib iso21434_vocabulary`.
- [ ] AC4 Catalog syntax:
  - R155 accepts `1`, `32`, `4.1`, and `32.99`. It rejects `0`, `33`, `04`, `4.0`, `4.`, `.1`, ` 4`, `+4`, `4.1.2`, and the empty string. The order is `4 < 4.1 < 4.2 < 5`.
  - A test-declared CAPEC-style scheme (`CAPEC-`, `[{1..=9999, None}]`) accepts `CAPEC-123` and rejects `CAPEC-0123` and `capec-123`.
  - A test-declared ATT&CK-style scheme (`T`, `[{1..=9999, Some(4)}, {1..=999, Some(3)}]`, required 1) accepts `T1059` and `T1059.001` and rejects `T1059.1` and `T59`.
  - `display_catalog_ref` round-trips through parsing for every accepted ID.

  Test: `cargo test --lib catalog_syntax`.
- [ ] AC5 `PropertySet` set operations work. `size_of::<PropertySet>() == 8`, `size_of::<CatalogRef>() <= 12`, and every vocabulary index is 1 byte. Test: `cargo test --lib value_sizes` and `cargo test --lib property_set`.
- [ ] AC6 A worked 21434 example builds two assets, two damage scenarios (one partly unrated), and two threats (one with an R155 ref) through `apply`. The read API returns every field. `targets()` equals the STRIDE mapping. `damages()` and `catalog_refs()` are in sorted order whatever order they were added in. An unrated impact is `None`, not the lowest level. Test: `cargo test --test project worked_example`.
- [ ] AC7 **Second methodology as data.** `tests/second_methodology.rs` declares a non-automotive `MethodologyDef` with:
  - three impact categories, with scales of 3, 5, and 2 levels;
  - only the properties C, I, and A;
  - a threat category that violates two properties;
  - a CAPEC-style scheme.

  It applies every `Op` variant and checks the results. It also checks that a 21434 index outside this methodology's ranges (for example property 5, or level 3 in a 3-level category) is rejected. The test file is the only change needed to support this methodology. Test: `cargo test --test second_methodology`.
- [ ] AC8 Every new `ModelError` variant is triggered by at least one test, `validate` returns the same error as `apply`, and the project is `==` to its clone from before the call. Test: `cargo test --test project rejects_`.
- [ ] AC9 Referential integrity:
  - `RemoveAsset` fails with `AssetInUse` and `RemoveDamageScenario` fails with `DamageScenarioInUse` while they are referenced.
  - `SetAssetProperties` that drops a targeted property fails with `PropertyInUse`, and `SetThreatCategory` to a category the asset cannot satisfy fails with `PropertyNotOnAsset`.
  - All of these succeed after the threat is removed or recategorised.

  Test: `cargo test --test project referential_integrity`.
- [ ] AC10 For every `Op` variant, applying the op and then its inverse gives a project `==` to the one before, and the inverse of the inverse equals the original op. Test: `cargo test --test project inverse_round_trip`.
- [ ] AC11 Proptest, 256 cases **per methodology** (the 21434 one and the AC7 one), sequences of 0 to 64 random ops that mix valid and invalid ones (existing and fresh IDs, in-range and out-of-range vocabulary indices drawn from the methodology's sizes):
  - a failed op leaves the project unchanged, and `validate` agrees with `apply`;
  - each inverse restores the previous project;
  - applying all inverses in reverse gives `Project::new(methodology)`.

  Test: `cargo test --test ops_properties`.
- [ ] AC12 A lib-level proptest runs the same sequences and calls `check_invariants` after every op. The invariants are:
  - ID maps and arenas agree;
  - the reference counts equal a recount;
  - every threat's asset carries `violates(category)`;
  - `damages` and `catalog_refs` are sorted and unique, and point to live slots and declared schemes;
  - every `impacts` row has one slot per category, with levels in range.

  Test: `cargo test --lib project_invariants`.
- [ ] AC13 Determinism (P4): two projects built from the same ops in two different valid orders are `==`, and their iterators yield the same IDs in ascending order. Test: `cargo test --test project deterministic_order`.
- [ ] AC14 Errors never contain names. The sentinel name `SECRET-LABEL-9f3` is used on every entity, every new `ModelError` variant is triggered, and no `Display` or `Debug` output contains the sentinel. Test: `cargo test --test project errors_have_no_labels`.
- [ ] AC15 Neutrality: no automotive vocabulary appears in core code outside `src/methodology/iso21434.rs`. Check: `grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' src --include='*.rs' | grep -v '^src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//'` prints nothing. Comments and doc comments are excluded, so docs may say "for example ISO/SAE 21434".
- [ ] AC16 Every new public item has a doc comment with a compiling example. Each function that takes project-sized or methodology-sized input states its time and space cost (Q5, A8). Check: `cargo test --doc` and `cargo clippy --all-targets -- -D warnings` (`missing_docs` and `clippy::pedantic` are on).
- [ ] AC17 No `unwrap`, `expect`, `panic!`, or panicking indexing outside `#[cfg(test)]` in the new files. Check: `grep -rnE '\.unwrap\(|\.expect\(|panic!' src/methodology src/asset.rs src/damage.rs src/threat.rs src/op.rs src/project.rs` shows hits only inside test modules. Arena and vocabulary access goes through `get`.
- [ ] AC18 The existing tests and benches still pass unchanged. Check: `cargo test` and `cargo test --benches`.
- [ ] AC19 ADR 0008 exists, covers the items in Design notes, and includes the second-methodology sketch. Check (from the repo root): `test -f docs/adr/0008-methodology-neutral-domain-model.md && grep -qi 'second methodology' docs/adr/0008-methodology-neutral-domain-model.md`.
- [ ] AC20 The CI gates and the license policy pass, with no new dependency in `Cargo.toml`. Check (from the repo root): `scripts/ci.sh crates/aegis-model`, `scripts/deny.sh licenses bans sources`, and `git diff --exit-code master -- crates/aegis-model/Cargo.toml` (or only `[[test]]`/`[[bench]]` entries changed).

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
grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' crates/aegis-model/src --include='*.rs' | grep -v '^crates/aegis-model/src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//'   # must print nothing
test -f docs/adr/0008-methodology-neutral-domain-model.md
```
