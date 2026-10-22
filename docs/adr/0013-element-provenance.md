# ADR 0013: Element provenance on entities and operations

- Status: proposed (job 014-provenance)
- Date: 2026-10-24
- Crate: `crates/aegis-model` (`src/provenance.rs`, `src/asset.rs`,
  `src/damage.rs`, `src/threat.rs`, `src/op.rs`, `src/project.rs`,
  `src/error.rs`, `src/store/`)
- Rules: P3, P4, D3, D4, D5, D6, D7, D8, D9, D10, R3, R6, A8, A9, Q3, Q4,
  Q5, Q6, L1 in `docs/engineering-standards.md`
- Builds on: `0008-methodology-neutral-domain-model.md`,
  `0009-save-load-format.md`, `0012-threat-attack-trees.md`

## Context

Rule D6 in `docs/engineering-standards.md` requires provenance on every
element in the project:

> **D6.** Every element carries provenance: who created or changed it,
> when, and from what source (human, import, catalog, AI).

And D4 requires every state mutation to be an explicit, serializable
operation (`Op`) with an inverse for O(op size) undo (D9).

Three design questions had to be resolved with the owner (recorded in
`factory/jobs/014-provenance/rework.md`):

1. Whether an element stores a single current `Provenance` record (updated
   on each mutation and restored on undo) or a dual `created` + `modified`
   pair.
2. How `Op` carries provenance so undo restores the exact previous
   provenance in O(1) while keeping in-memory entity sizes within the 3x
   server memory budget (`benches/store_memory.rs`).
3. How the v1 snapshot and op-log store handle non-default provenance
   before format v2 adds wire fields for attack trees and provenance.

The owner's guidance across all three questions:

> Select based on the project's guiding principle: it should be
> maintainable, easily scalable, and accurate.

## Options

### 1. Element provenance shape: single current `Provenance` vs. `created` + `modified`

| Option | Good | Bad |
|---|---|---|
| **A. Single `Provenance` on each element (`Asset`, `DamageScenario`, `ThreatScenario`, and `TreeEntry`)** | One source of truth on the entity; full creation and edit history lives in the append-only op log (D4, ADR 0009); every mutating `Op` swaps `element.provenance` with `op.provenance` and puts the old `Provenance` into the returned inverse `Op`, so `apply` + `undo` is a symmetric `std::mem::replace` | Answering "who originally created this after N edits?" without the op log requires reading the first op for that ID |
| B. Both `created: Provenance` and `modified: Provenance` on every element | Creation metadata stays on the snapshot entity without the op log | Doubles per-entity provenance storage; `Add*` inverse ops and `Remove*` must carry both `created` and `modified` to make remove-then-undo exact |

**Chosen: A.** Keeping the latest provenance on the element and the full
history in the op log avoids duplicating audit history inside every live
entity and keeps every mutation's inverse symmetric.

### 2. In-memory representation of `Provenance` and how `Op` carries it

A naive `Provenance { actor: Arc<str>, timestamp_ms: u64, source: ProvenanceSource }`
is 32 bytes inline on 64-bit targets (`Arc<str>` 16 + `u64` 8 + `u8` + 7
padding). Adding 32 bytes inline to `DamageScenario` (was 32 bytes) would
double its arena slot from 32 to 64 bytes and push the `damages_only_max`
benchmark in `benches/store_memory.rs` from 2.65x kept heap to ~3.45x,
exceeding the 3.0x `SERVER_BUDGET`.

| Option | Good | Bad |
|---|---|---|
| A. Inline 32-byte struct | No indirection when reading non-default provenance | Adds 24–32 bytes to every entity slot even when provenance is default (`Project::load` v1); breaks the 3.0x kept-heap budget on dense damage-scenario files |
| **B. `Provenance(Option<Arc<ProvenanceInner>>)` with canonical `None` for default provenance (`actor: ""`, `timestamp_ms: 0`, `source: Human`)** | `size_of::<Provenance>() == 8` bytes; `Provenance::default()` and `Provenance::new("", 0, Human)` allocate 0 heap bytes; `Clone` is an `Option<Arc>` clone (O(1)); `damages_only_max` kept heap is 2.78x (within the 3.0x budget) | One `Arc` allocation (32 bytes) when constructing a non-default `Provenance`, shared across the `Op`, the entity, and the inverse `Op` |

**Chosen: B.** And for `Op`:
- All 16 creating and mutating `Op` variants (`Add*`, `Rename*`, `Set*`,
  `AddCatalogRef`, `RemoveCatalogRef`, `LinkDamage`, `UnlinkDamage`) carry
  `provenance: Provenance` directly.
- The 4 removal variants (`RemoveAsset`, `RemoveDamageScenario`,
  `RemoveThreatScenario`, `RemoveAttackTree`) delete the entity, so they do
  not store provenance on a deleted slot; their returned inverse `Add*` op
  carries the removed entity's stored `Provenance` so undoing a removal
  restores the exact provenance.

### 3. Store v1 behavior before format v2

| Option | Good | Bad |
|---|---|---|
| **A. Keep `aegis-project` v1 and `aegis-oplog` v1 wire formats unchanged; reject non-default provenance in `Project::save` and `OpLogWriter::append` with `StoreError::UnencodableOp`** | Existing v1 golden fixtures and fuzz seeds stay byte-for-byte valid; never silently drops provenance on save or append; matches ADR 0012's handling of attack trees until format v2 | Projects or ops with non-default provenance require format v2 to persist |
| B. Silently strip provenance when writing v1 | Save/append succeed on any provenance | Violates round-trip accuracy (`load(save(p)) == p` and `read(append(op)) == op` would fail whenever provenance is non-default) |

**Chosen: A.**

## Decision

### Types (`src/provenance.rs`)

- `ProvenanceSource` (`Copy`, `Clone`, `Debug`, `Default`, `PartialEq`,
  `Eq`, `PartialOrd`, `Ord`, `Hash`):
  - `Human` (default)
  - `Import`
  - `Catalog`
  - `Ai`
- `Provenance(Option<Arc<ProvenanceInner>>)`:
  - `Provenance::default()` has `actor() == ""`, `timestamp_ms() == 0`,
    `source() == ProvenanceSource::Human`, `is_default() == true`, and
    allocates nothing.
  - `Provenance::new(actor, timestamp_ms, source)` canonicalizes
    `("", 0, ProvenanceSource::Human)` to `None` so equality and hashing
    are structural (`Ord` and `Hash` compare
    `(actor(), timestamp_ms(), source())`).
  - Convenience constructors: `Provenance::human`, `Provenance::import`,
    `Provenance::catalog`, `Provenance::ai`.
- `MAX_ACTOR_CHARS: usize = 256` (matching `MAX_NAME_CHARS`).

### Validation (`src/project.rs`, `src/error.rs`)

Every creating or mutating `Op` validates its `provenance`:
- Default provenance (`Provenance::default()`, where `is_default()` is
  true) is accepted for programmatic and v1-decoded operations.
- Any non-default provenance must have a non-blank `actor` after trimming
  whitespace (`ModelError::EmptyActor`) and at most `MAX_ACTOR_CHARS`
  (256) Unicode scalar values (`ModelError::ActorTooLong { max: 256 }`).
  In accordance with security rule R3 / ADR 0009, neither error variant
  stores or echoes the rejected actor string.

### Entity accessors and equality

- `Asset::provenance(&self) -> &Provenance`
- `DamageScenario::provenance(&self) -> &Provenance`
- `ThreatScenario::provenance(&self) -> &Provenance`
- `Project::attack_tree_provenance(&self, idx: TreeIdx) -> Option<&Provenance>`
- `Op::provenance(&self) -> Option<&Provenance>`
- `Project::eq` compares provenance on every live asset, damage scenario,
  threat scenario, and attack tree.

## Complexity

Every provenance check, update, and undo is O(1) in project size:
- `check_provenance`: O(actor bytes), bounded by `MAX_ACTOR_CHARS` = 256.
- `commit` / `undo`: O(1) pointer swap (`std::mem::replace` on an 8-byte
  `Provenance`).

## Numbers

Measured on aarch64-apple-darwin:

| Item | Value |
|---|---|
| `ProvenanceSource` | 1 byte |
| `Provenance` | 8 bytes (`Option<Arc<ProvenanceInner>>` niche) |
| `Asset` (and `Option<Asset>`) | 32 bytes (was 24) |
| `DamageScenario` (and `Option<DamageScenario>`) | 40 bytes (was 32) |
| `ThreatScenario` (and `Option<ThreatScenario>`) | 88 bytes (was 80) |
| `Op` | 112 bytes (was 104) |
| `damages_only_max` peak / kept (`benches/store_memory.rs`) | 3.65x / 2.78x (gates: 4.8x / 3.0x) |

## Consequences

- Every element in `Project` now carries `Provenance` (D6), updated on
  every mutation and restored on undo (D9).
- All 20 `Op` variants and all 36 `ModelError` variants are exercised by
  unit, integration, and property tests across both the built-in ISO/SAE
  21434 + UN R155 methodology and the second medical-device methodology.
- Format v2 for snapshots and op logs will encode both attack trees (ADR
  0012) and provenance together.
