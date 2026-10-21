# ADR 0010: Sparse per-asset reference counts

- Status: proposed (job 011-sparse-refcounts)
- Date: 2026-10-21
- Crate: `crates/aegis-model` (`RefCounts` in `src/project.rs`; the gates
  in `benches/store_memory.rs`)
- Rules: section 2 (server memory per open project <= 3x its serialized
  size), D1, D4, D7, D10, A8, Q3, L1 in `docs/engineering-standards.md`
- Changes: the "Reference counts, not scanning" section of
  `0008-methodology-neutral-domain-model.md`; corrects the numbers in
  `0009-save-load-format.md`

## Context

Job 010 measured how much heap `Project::load` needs (ADR 0009, Numbers).
One shape broke the server budget "memory per open project <= 3x the
project's serialized size": a file made only of minimal assets, over a
methodology with 64 security properties. Loading it peaked at 6.16x the
file and kept 5.53x after `load` returned. The cause was the in-memory
model, not the file format. Every asset slot carried a dense row of
reference counts, `u32` x every property in the methodology (256 bytes at
64 properties), even though no threat targeted it. A minimal asset is
about 77 bytes of JSON.

Job 010 escalated this (`factory/jobs/010-save-load/decision.md`). The
owner accepted the breach only until the next factory job made the rows
sparse. This is that job. The owner also settled what the budget covers:
the **open project**, which is the heap still held after `load` returns
("kept"). The transient peak during `load` stays gated, but it is not
held to 3x.

Job 011 added three shapes to the bench, in which every asset is
targeted. The narrow one shows that the dense row also broke the budget
for a realistic-looking file: one minimal threat per minimal asset over
the 64-property methodology kept **3.60x** before this change.

## Options

Where the counts are used: `check_set_asset_properties` asks "how many
live threats on asset `a` violate property `p`?" for each property being
dropped. Every op that adds, removes, or recategorizes a threat counts
`violates(category)` in or out. p = |violates| <= 64, and r (the row
length) <= the asset's property count <= 64.

Bytes are for a 64-bit target. "Measured" means `benches/store_memory.rs`
(dhat, requested bytes, so the allocator's own per-block overhead is not
included).

| Option | Per asset slot | Per targeted (asset, property) | Lookup | Notes |
|---|---|---|---|---|
| dense row (job 009) | 4 x methodology properties: 24 B (6 properties) to 256 B (64), plus `Vec` doubling slack (measured: about 308 B per asset at 64) | 0 | O(1) | the 5.53x breach |
| **sorted boxed slice per slot (chosen)** | 16 B (a `Box<[PropertyCount]>` fat pointer; empty means no heap) | 8 B (`Property` u8 + `u32`, padded) | O(log p), p <= 64 | one allocation per targeted asset; no new type of map, no dependency |
| `Option<Box<{PropertySet, Box<[u32]>}>>` per slot | 8 B | 4 B, plus a 24 B header per targeted asset | O(1) by rank in the bitset | two allocations per targeted asset; rank arithmetic on the bitset |
| `BTreeMap<(AssetIdx, Property), u32>` | 0 | about 13 B (full leaf: 11 entries in a 144 B node) to 20 B (half-full) | O(log E), E = all pairs | worse than the dense row when one category violates many properties: 64 pairs cost about 1 KiB per asset |
| map `AssetIdx -> row` | 0 | 8 B, plus about 30 B per map entry | O(log a), or a new hasher dependency (D7) | saves 16 B per untargeted asset; not needed for the budget |

## Decision

One sorted, exact-size boxed slice per asset slot:

```rust
struct PropertyCount { property: Property, threats: u32 }   // 8 bytes

struct RefCounts {
    asset_properties: Vec<Box<[PropertyCount]>>,  // 16 bytes per asset slot
    assets: Vec<u32>,                             // unchanged
    damages: Vec<u32>,                            // unchanged
}
```

- **Canonical form.** Each row is sorted by property, has no duplicates,
  and has no zero counts. A row no threat touches (tombstones included,
  since an asset can only be removed when no threat targets it) is
  `Box::default()`, which does not allocate. The derived `==` against a
  fresh recount (`check_invariants`) is therefore an exact check. The
  recount is built independently of the update code, from a
  `BTreeMap<(slot, property), count>`.
- **Lookup** is a binary search in the asset's row; a missing entry
  means 0.
- **Update** (`count_row`): if the row's set of properties does not
  change (count in properties it already has, or count out none that
  reach 0), the counts change in place with no allocation. Otherwise the
  row and `violates` merge into one new exact-size slice: one allocation,
  or none if the row becomes empty.
- **`SetThreatCategory` counts the new category in before the old one
  out**, so a property both categories violate never drops to 0 and
  causes no reallocation.
- **`AddAsset` pushes an empty row**, not `property_count` zeroed
  counters.
- The flat-index overflow check in `check_add_asset` is gone: nothing is
  flat-indexed any more. On 64-bit targets it could never fire. The
  slot-limit check stays.
- The per-asset and per-damage-scenario threat counts stay dense: 4
  bytes per slot, and they do not grow with the methodology.

No public API, error, error count, or file format changes. Every existing
test, including the golden v1 fixtures and the 256-case op proptests,
passes unchanged.

Why not the alternatives: the `Option<Box<..>>` layout halves the bytes
per entry, but needs two allocations per targeted asset and rank
arithmetic, and the chosen layout already keeps every shape well under
3x. The `BTreeMap` of pairs loses to the dense row in the wide case. A
map from asset to row saves only the 16 bytes per untargeted asset, which
the budget does not need.

## Complexity (A8)

- **Space:** O(asset slots + damage slots + targeted (asset, property)
  pairs). Before: O(asset slots x methodology properties).
- **Time:** a lookup is O(log p). Counting one threat in or out is
  O(r + p), r and p at most 64, so every op stays in the cost class ADR
  0008 lists (O(p + k) for a threat op). It allocates at most once per
  op that changes a row's set of properties.
- **`clone` and `==`** stay O(n + L + m). `Clone` now makes one
  allocation per targeted asset instead of one large copy of the dense
  row. `Project`'s `==` is logical and does not compare the counts.
- **Trade-off:** a fully targeted row costs 8 bytes per entry, twice the
  dense row's 4. A project in which every asset is targeted on all 64
  properties keeps more than before (`targeted_wide_max`, below: 0.98x to
  1.36x). It stays far under 3x, because such an asset already costs
  about 300 bytes of JSON for its 64 property keys.

## Numbers

### Memory

`cargo bench --bench store_memory`, release build, on the build machine
(Apple Silicon laptop). Heap as a factor of the file size; each hostile
file is about 4 MiB, and the factors do not depend on the size. "Before"
is `master` (job 010) with the new bench; "after" is this job. Gates are
the measured factor plus about 35%, rounded up to 0.1. Kept gates are
capped at 3.0, and no gate is higher than its job 010 value.

| Case | Before peak / kept | After peak / kept | Gates peak / kept (job 010 peak gate) |
|---|---|---|---|
| `typical` | 1.03 / 0.83 | 1.03 / 0.83 | 1.4 / 1.2 (1.5) |
| `short_property_keys` | 1.47 / 1.32 | **0.54 / 0.38** | 0.8 / 0.6 (2.0) |
| `short_impact_keys` | 0.76 / 0.54 | 0.76 / 0.54 | 1.1 / 0.8 (1.5) |
| `unknown_property_keys` | 0.02 / 0.00 | 0.02 / 0.00 | 0.1 / 0.1 (0.1) |
| `unknown_impact_keys` | 0.02 / 0.00 | 0.02 / 0.00 | 0.1 / 0.1 (0.1) |
| `unknown_one_byte_keys` | 0.02 / 0.00 | 0.02 / 0.00 | 0.1 / 0.1 (0.1) |
| `long_names` | 1.31 / 1.28 | **1.08 / 1.04** | 1.5 / 1.5 (2.0) |
| `one_huge_string` | 1.02 / 0.00 | 1.02 / 0.00 | 1.4 / 0.1 (2.5) |
| `minimal_entities` | 2.99 / 2.59 | **2.08 / 1.68** | 2.9 / 2.3 (4.0) |
| `assets_only_builtin` | 2.43 / 1.81 | **2.31 / 1.68** | 3.2 / 2.3 (3.3) |
| `assets_only_max` | 6.16 / 5.53 | **2.41 / 1.78** | 3.3 / 2.2 (8.3) |
| `damages_only_builtin` | 2.73 / 1.85 | 2.73 / 1.85 | 3.7 / 2.5 (3.7) |
| `damages_only_max` | 3.53 / 2.65 | 3.53 / 2.65 | 4.8 / 3.0 (4.8) |
| `threats_only_builtin` | 1.91 / 1.10 | 1.91 / 1.10 | 2.6 / 1.5 (2.6) |
| `threats_only_max` | 1.93 / 1.12 | 1.93 / 1.12 | 2.6 / 1.6 (2.6) |
| `targeted_narrow_builtin` (new) | 2.56 / 1.75 | 2.53 / 1.72 | 3.5 / 2.4 (none) |
| `targeted_narrow_max` (new) | 4.41 / **3.60** | **2.57 / 1.76** | 3.5 / 2.4 (none) |
| `targeted_wide_max` (new) | 1.18 / 0.98 | 1.57 / 1.36 | 2.2 / 1.9 (none) |

Notes:

- Assets only, over the max vocabulary: the dense row plus its doubling
  slack was about 289 bytes per asset (54,472 assets in a 4 MiB file,
  (5.53 - 1.78) x 4,194,482 / 54,472). That matches 16 MiB of `u32`
  capacity for 3.5 M counters, less the new 16-byte slot (plus slack).
  The spec's estimate was about 1.8x kept and 2.4x peak; measured 1.78x
  and 2.41x.
- `targeted_wide_max` is the one shape that grows (about 227 more bytes
  per asset: a 512-byte row instead of a 256-byte dense row with slack).
  See the trade-off under Complexity.
- Allocation counts rise where assets are targeted (one block per
  targeted asset: `targeted_narrow_builtin` 40,231 to 57,419 blocks over
  the whole load) and fall slightly where none are.
- **Budget:** kept is at most 2.65x for every shape and every
  methodology size measured (damage scenarios only over 32 impact
  categories), and the bench fails if any kept gate exceeds 3x. One
  transient peak is above 3x: the same shape, 3.53x while loading, from
  the dense impact slots in `DamageScenario`. Under the owner's answer
  the budget limits the open project, so that peak is gated (4.8x) but
  not held to 3x. Making the impact slots sparse would lower it; that is
  out of scope here.

### Time (AC5)

`cargo bench --bench store` (divan medians, 100 samples), run on the same
machine. `master` ran three times, the job branch three times, with one
pair run back to back by swapping only `src/project.rs`:

| Bench | `master` medians | Job branch medians | Change (worst branch vs best master) |
|---|---|---|---|
| `load` (1.1 MB snapshot, 4,300 entities) | 5.396, 5.346, 5.221 ms | 5.135, 5.157, 5.207 ms | -0.3% |
| `replay_log` (10,000 ops) | 9.807, 9.290, 8.960 ms | 8.936, 8.858, 9.050 ms | +1.0% |

Both are within the +10% limit. The typical project has 300 assets and
2,000 threats, so most assets are targeted, and the merge path runs on
almost every `AddThreatScenario` during load and replay. The allocations
it adds are offset by not zero-filling a dense row per asset.

## Consequences

- Every valid shape the bench knows stays within the 3x server budget
  for the open project, at every methodology size, and CI checks it.
- The per-asset property-count cost now follows what threats target, not
  the methodology's vocabulary size. Adding properties to a methodology
  no longer costs memory per asset.
- A fully targeted asset costs 8 bytes per property instead of 4. If that
  ever matters, the `Option<Box<{PropertySet, Box<[u32]>}>>` layout
  halves it at the price of a second allocation.
- Persistent collections for snapshots (D9) are still open; the rows are
  plain owned slices, so a later `imbl` job can wrap them.
