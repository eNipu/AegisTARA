# ADR 0014: Store persistence for attack trees and provenance

- Status: proposed (job 016-store-trees-and-provenance)
- Date: 2026-10-25
- Crate: `crates/aegis-model` (`src/tree.rs`, `src/project.rs`,
  `src/error.rs`, `src/lib.rs`, `src/store/`)
- Rules: P3, P4, P5, D4, D6, D7, D8, D9, D10, R3, R6, A8, A9, Q3, Q4, Q5,
  Q6, L1 in `docs/engineering-standards.md`
- Builds on: `0009-save-load-format.md`, `0012-threat-attack-trees.md`,
  `0013-element-provenance.md`

## Context

Jobs 013 ([ADR 0012](0012-threat-attack-trees.md)) and 014
([ADR 0013](0013-element-provenance.md)) added attack trees, step
feasibility ratings, threat-to-tree links, the 4 attack-tree `Op` variants
(`AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`,
`RemoveAttackTree`), and element/operation `Provenance` to `aegis-model`.
Both jobs deferred wire encoding in `aegis-project` snapshots and
`aegis-oplog` logs to this job, returning `StoreError::UnencodableOp`
whenever a project held attack trees or non-default provenance.

This ADR records the wire format, decode-time caps, and DAG reconstruction
invariants for persisting attack trees, step feasibility ratings, threat
tree links, and provenance.

## Options

### 1. Backward compatibility and canonical omission of defaults

| Option | Good | Bad |
|---|---|---|
| **A. Keep `version: 1` and omit new fields when empty/default (`attack_trees` omitted when empty, `attack_tree` omitted when `None`, `provenance` omitted when `is_default()`); reject explicit `null` and explicit default provenance on decode** | Existing v1 golden fixtures (`tests/fixtures/project-v1.json`) and op logs remain byte-for-byte identical on `save`; canonical representation is unique (`save(load(bytes)) == bytes`); zero wire overhead for default provenance | Decoder needs a small `some` helper so `"provenance": null` and `"attack_tree": null` are rejected rather than silently treated as missing |
| B. Bump wire version to 2 and always emit `"attack_trees": []`, `"attack_tree": null`, and `"provenance": {...}` | No conditional field skipping | Breaks all v1 fixtures; bloats every entity and op with 60+ bytes of default provenance |

**Chosen: A.**

### 2. Attack-tree DAG wire encoding

An `AttackTree` is a rooted DAG stored in an arena (`NodeIdx(0)` is the
root; every non-root node has an `owner` with `owner.index() < node.index()`
plus zero or more `linked_parents` from `AttackTree::link`).

| Option | Good | Bad |
|---|---|---|
| A. Encode a sequence of `add_child` and `link` construction steps | Replays public constructors | Verbose; couples the snapshot format to builder history rather than current state |
| **B. Encode `nodes` in arena order (`0..len`); omit `parents` when a node has `<= 1` incoming edge; require `parents: [owner, ...linked_parents]` when a shared node has `>= 2` incoming edges** | Compact for pure trees (no `parents` field on any node); preserves exact `owner` and `linked_parents` order for shared DAG nodes so `load(save(p)) == p` and `undo(RemoveAttackTree)` are exact; validates in O(V + E) time | Reconstruction must verify that `parents` on shared nodes is an exact permutation of incoming edges with `parents[0].index() < node.index()` |

**Chosen: B.**

### 3. Attack-tree node cap (`MAX_ATTACK_TREE_NODES`)

| Option | Good | Bad |
|---|---|---|
| **A. Cap each attack tree in a `Project` at `MAX_ATTACK_TREE_NODES = 32` nodes (`ModelError::TooManyAttackTreeNodes`, `Limit::AttackTreeNodes { max: 32 }`, `Limit::StepFeasibility { max: 32 }`)** | Matches the existing per-entity collection caps (`32` impact ratings, `32` catalog refs, `64` damage links); keeps `Op::AddAttackTree` and `Op::RemoveAttackTree` within the 64 KiB single-line op-log cap (`Limit::OP_LINE_BYTES`) even with worst-case 1,024-byte labels and full DAG links | Standalone `AttackTree` outside `Project` still supports up to `u32::MAX` nodes (tested up to 1,000,000 nodes in `benches/eval.rs`), while project-managed trees are bounded at 32 nodes |
| B. Unbounded nodes per tree in `Project` | No node cap on `AddAttackTree` | A single `AddAttackTree` or `RemoveAttackTree` op could exceed the 64 KiB JSON Lines op-log frame cap, breaking `append` -> `read` round-tripping |

**Chosen: A.**

## Decision

### Snapshot format (`aegis-project` v1)

- Top-level `attack_trees` is an optional array sorted by `TreeId` and
  omitted when the project has no attack trees:
  ```json
  {
    "id": "0199c82c-c001-7000-8000-000000000030",
    "nodes": [
      {
        "id": "0199c82c-c001-7000-8000-000000000031",
        "label": "Compromise brake command",
        "gate": "or",
        "children": [
          "0199c82c-c001-7000-8000-000000000032",
          "0199c82c-c001-7000-8000-000000000033"
        ]
      },
      {
        "id": "0199c82c-c001-7000-8000-000000000032",
        "label": "Inject frames on OBD port"
      },
      {
        "id": "0199c82c-c001-7000-8000-000000000033",
        "label": "Reflash gateway over diagnostic session"
      }
    ],
    "step_feasibility": {
      "0199c82c-c001-7000-8000-000000000032": "high",
      "0199c82c-c001-7000-8000-000000000033": "low"
    }
  }
  ```
- In `nodes`:
  - `gate` (`"or"`, `"and"`, `"seq_and"`) and `children` (`1..=32` `NodeId`s)
    must both be present on gate nodes and both be omitted on step nodes.
  - `parents` is omitted when a node has at most one incoming edge (root has
    0; unshared child has 1, which is its `owner` at a smaller arena index).
    When a node has 2 or more incoming edges, `parents` lists all incoming
    parent `NodeId`s (`parents[0]` is the `owner` with `owner.index() < i`;
    `parents[1..]` are `linked_parents` in link order).
- `step_feasibility` in a snapshot is a JSON object whose entries are sorted
  by `NodeId` and whose values are methodology feasibility-level keys
  (resolved while decoding via `FeasibilityLevelKey`, reporting
  `StoreError::UnknownKey { kind: VocabKind::FeasibilityLevel, .. }` on an
  unknown key).
- On each threat in `threat_scenarios`, `"attack_tree": "<TreeId>"` is
  emitted when the threat links to a tree and omitted when `None`.
- On each asset, damage scenario, threat scenario, and attack tree, as well
  as each creating or mutating `Op` in `aegis-oplog`, `"provenance"` is
  omitted when `Provenance::is_default()` and otherwise encoded as:
  ```json
  {
    "actor": "alice",
    "timestamp_ms": 1760000000000,
    "source": "human"
  }
  ```
  (`source` is `"human"`, `"import"`, `"catalog"`, or `"ai"`).

### Op-log format (`aegis-oplog` v1)

All 20 `Op` variants are supported in `OpLogWriter::append` and
`OpLogReader::read`:
- `add_attack_tree` (`id`, `nodes`, `step_feasibility` preserving the op's
  entry order, optional `provenance`)
- `set_step_feasibility` (`tree`, `node`, `level: Option<key>`, optional
  `provenance`)
- `set_threat_attack_tree` (`threat`, `tree: Option<TreeId>`, optional
  `provenance`)
- `remove_attack_tree` (`id`)
- All 16 prior variants now also encode and decode optional `provenance`.

`StoreError::UnencodableOp` is retained in `StoreError` (`#[non_exhaustive]`)
for source compatibility and as a defensive guard if a live slot is missing
during `save`.

### Decode-time caps (`Limit` and `VocabKind`)

- `MAX_ACTOR_BYTES: usize = 1024` (`Limit::ActorBytes { max: 1024 }`)
- `MAX_ATTACK_TREE_NODES: usize = 32`
  (`Limit::AttackTreeNodes { max: 32 }`,
  `ModelError::TooManyAttackTreeNodes { tree, max: 32 }`)
- `Limit::StepFeasibility { max: 32 }`
- `VocabKind::FeasibilityLevel` (`"feasibility level"`)

## Complexity

n = trees in the project, V = nodes per tree (`<= 32`), E = edges per tree,
r = rated steps (`<= V`).

| Operation | Time | Space |
|---|---|---|
| `AttackTree::from_wire_parts` | O(V + E) | O(V + E) |
| `Project::save` (tree portion) | O(n log n + sum(V + E + r log r)) | O(n + max(V + E + r)) |
| `Project::load` (tree portion) | O(n log n + sum(V + E + r log r)) | O(sum(V + E)) |

## Numbers

Measured on aarch64-apple-darwin (`benches/store_memory.rs`, 4 MiB snapshot
of 9,533 3-node attack trees with 2 rated steps each):

| Case | File bytes | Peak heap | Kept heap | Budget |
|---|---:|---:|---:|---:|
| `attack_trees` | 4,194,676 | 11,611,238 (2.77x, gate 3.4x) | 8,179,618 (1.95x, gate 2.4x) | <= 3.0x |

## Consequences

- Every `Project` state and every `Op` variant (all 20) now round-trips
  through `Project::save` / `Project::load` and `OpLogWriter::append` /
  `OpLogReader::read` with canonical byte-for-byte determinism.
- Existing v1 snapshots and op logs without attack trees or non-default
  provenance continue to load and save with zero format changes.
