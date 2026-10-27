# ADR 0014: Store persistence for attack trees and provenance

- Status: proposed (job 016-store-trees-and-provenance); the node cap of decision 3 was replaced by job 065 (see the update at the end)
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

## Update: job 065-tree-node-ops (2026-10-27, FND-02 and FND-03)

Decision 3 above capped a tree at 32 nodes so that `AddAttackTree` and the
inverse of `RemoveAttackTree`, which carry a whole tree, fit one op-log record
(64 KiB). Bigger trees need ops that carry one node.

### Two caps

| Cap | Value | Applies to |
|---|---|---|
| `MAX_ATTACK_TREE_NODES` | 4,096 | nodes in one tree of a project; the snapshot's `nodes` list |
| `MAX_TREE_OP_NODES` | 32 | nodes an op carries whole: `AddAttackTree`, and so `RemoveAttackTree` (its inverse) |

A tree larger than 32 nodes is built by `AddTreeNode` ops and taken apart by
`RemoveTreeNode`; `RemoveAttackTree` on one is refused with
`TreeTooLargeForOneOp`. A snapshot may hold a tree of any size up to the cap
(`Project::load` has its own path that skips only the whole-op cap).
The op-log decode cap is still 4,096 nodes, but a record that long is refused
first by the 64 KiB record cap, so the writer refuses to write more than
`MAX_TREE_OP_NODES` in one `add_attack_tree` record.

### Six node ops

`AddTreeNode`, `RemoveTreeNode`, `LinkTreeNode`, `UnlinkTreeNode`,
`RenameTreeNode`, `SetTreeNodeGate`. Each carries the tree's provenance like
every other change to a tree, and each is undone by the next one:

| Op | Inverse |
|---|---|
| `AddTreeNode` (at arena `index`, child `position`) | `RemoveTreeNode` |
| `RemoveTreeNode` | `AddTreeNode` at the node's old index and position |
| `LinkTreeNode` (`position` among the gate's children, `slot` among the node's extra parents) | `UnlinkTreeNode` |
| `UnlinkTreeNode` | `LinkTreeNode` at the old position and slot |
| `RenameTreeNode`, `SetTreeNodeGate` | the same with the old label or gate |

The `index`, `position` and `slot` are what make undo exact: a tree is equal to
another only if its arena order, child order and extra-parent order are, and
the snapshot writes all three. Inserting or removing a node shifts the arena
(O(V + E)): child lists, owners, extra parents, the ID map and the per-node
rating vectors of the project move together.

### What blocks a removal

`RemoveTreeNode` refuses the root (`CannotRemoveRoot`), a node with children
(`NodeHasChildren`), a node shared with another gate (`NodeIsShared`: unlink it
first) and a step with a rating, factors or controls (`NodeIsRated`). So the
inverse of a removal never has to carry a rating, the same rule as for
descriptions (ADR 0051). An edit clears and unlinks first, in the same batch.

### Evaluation cache

A structural op rebuilds the evaluation of that one tree (O(V + E)) and
refreshes the threats that use it; other trees and threats are untouched. A
rename changes no value and updates nothing. A test applies 120 random node ops
and compares the incremental cache with a fresh one after each.

### Cost (A1, 5 ms budget)

`benches/tree_nodes.rs`, release build, one threat on a full tree of 4,096
nodes, the op and its inverse together (the median of 20 runs):

| Case | Op and inverse |
|---|---|
| Insert at the front (shifts every later node) | 170 microseconds |
| Remove the node at the front | 174 microseconds |
| Change a gate | 111 microseconds |
| Rate a step (existing op, for scale) | 119 microseconds |
| Rename a node | 0.12 microseconds |

The worst case is about 0.2 ms for one op: 25 times under the budget.

### The outline editor

`Edit::ReplaceTree` no longer removes and re-adds the tree. `tree_diff.rs`
compares the tree with the outline and sends the ops for the difference:
nodes with the same ID and owner stay (with their ratings), others are cleared,
unlinked and removed (children first), new ones are added in outline order at
the end of the arena, new shared edges are linked, and a step that moved gets
its rating back. A node whose kept siblings would end up in another order is
replaced too, since there is no op that moves a child. The first line is the
tree's root: a new text renames it and a new gate kind sets its gate. Threats
stay attached throughout, and it is one batch and one undo step. A randomized
test checks 100+ edits between random outlines, including shared steps: the
result is the outline's tree, and one undo restores the project exactly.

One edit is at most 1,024 ops (a submission): a bigger change is refused with
`TooLarge` and made in steps. `RemoveTree` takes a big tree apart the same
way, so a tree of more than about 1,000 nodes is deleted in several steps. The
outline text cap is raised to 2 MiB to hold the largest tree.

### Not done

- No op moves a child to another owner or changes the order of children; the
  outline editor replaces the node instead.
- A node's description (ADR 0051) is not there yet.
- The canvas draws a 4,096-node tree with the existing layout (linear time);
  no new screen measurement was made beyond the tests of job 053.
