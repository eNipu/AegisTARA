# ADR 0001: Arena attack tree with typed IDs

- Status: accepted (job 001-aegis-model)
- Date: 2026-10-19
- Crate: `crates/aegis-model`
- Rules: D1, D2, D3, D5, D7, D12, Q2, Q3, Q4 in `docs/engineering-standards.md`

## Context

The spike (`spikes/spike-model`) stored the attack tree as nested owned
`Node` values with `String` IDs and found nodes with a recursive `find`, which
costs O(n) per lookup. The production model must handle the `large` reference
project (100,000 nodes) with lookups by public ID in O(1), must be safe to share
with the engine, store, and desktop client, and must serialize easily later.

Nodes need two kinds of identity:

- a **stable public ID** that survives edits, merges, and file round-trips, and
  that comes from untrusted input (files, network);
- a **cheap internal handle** for in-memory links (parent, children) and for
  the engine's inner loops.

## Options considered

### Graph representation

| Option | Pros | Cons |
|---|---|---|
| `Rc<RefCell<Node>>` with parent `Weak` links | Familiar object-graph style | One heap allocation per node, pointer chasing, runtime borrow panics, reference cycles possible, hard to serialize, not `Send` |
| Nested owned nodes (spike style) | Simple for pure trees | Lookup by ID is O(n); no parent links; cannot become a DAG later |
| **Arena (`Vec<Node>`) + typed integer index** | Contiguous memory, no `unsafe`, no runtime borrow checks, `Copy` handles, trivial to serialize, ready for DAG links in job 002 | Indices are only valid for the tree that issued them; removal needs a policy later (tombstones or a free list) |

### Map from public ID to index

| Option | Pros | Cons |
|---|---|---|
| `BTreeMap<NodeId, NodeIdx>` | Deterministic iteration order; O(log n) | Slower lookups; we never iterate the map, so its ordering buys nothing |
| `FxHashMap` / `ahash` | Fastest hashing | Fx is not resistant to hash flooding; keys come from untrusted files and network input (D7). Also an extra dependency |
| **`std::collections::HashMap` (SipHash)** | O(1) expected, flood resistant, no extra dependency | Somewhat slower hashing than Fx |

### Index width

| Option | Pros | Cons |
|---|---|---|
| `usize` | No conversion when indexing | 8 bytes on 64-bit targets; doubles the size of every child list and parent link |
| **`u32`** | 4 bytes; 4 billion nodes is far above the `stress` size (1,000,000) | One checked conversion when a node is added |

## Decision

- `AttackTree` stores nodes in an arena `Vec<Node>`. Public IDs (`Vec<NodeId>`)
  and parent links (`Vec<Option<NodeIdx>>`) are parallel arrays with the same
  index (struct-of-arrays, D4), so code that walks structure does not touch IDs.
- `NodeIdx(u32)` is the internal handle. It has no public constructor; the only
  `usize` to `NodeIdx` conversion is the private `NodeIdx::try_from_usize`, and
  overflow becomes `ModelError::CapacityExceeded` instead of a silent wrap.
- `NodeId(Uuid)` is the public ID and can only hold a version 7 UUID ("parse,
  don't validate", Q4). UUIDv7 is time-ordered, which helps sorted storage later.
- One `std::collections::HashMap<NodeId, NodeIdx>` with the default SipHash
  hasher, because keys are untrusted. The map is never iterated for output;
  deterministic order comes from the arena (`AttackTree::iter`).
- `Node` is an enum (`Gate { kind, label, children }` or `Step { label }`), so
  a step with children cannot be represented (D12). Labels are `Arc<str>` so
  repeated labels can be shared without copies (D5).
- Nodes are only added, each already attached to its parent, so the structure
  is always a tree. Linking existing nodes (DAGs) and cycle detection are job
  002; see `0002-dag-links-and-cycle-detection.md`, which replaces the "always a
  tree" property: after `link`, the structure is a directed graph that may
  contain cycles.

## Consequences

- Lookup by public ID is O(1) expected; parent, children, and node access by
  index are O(1); `add_child` is O(1) amortized. Iteration order is insertion
  order and is deterministic.
- Memory per node: one `Node` enum (an `Arc<str>` fat pointer plus, for gates,
  a `Vec` header), 16 bytes of UUID, 8 bytes of `Option<NodeIdx>`, and one map
  entry (key, value, and control byte). No allocation on lookup.
- A `NodeIdx` from one tree is meaningless in another. Out-of-range indices are
  rejected (`UnknownNode`) or return `None`/empty, but an in-range index from a
  different tree is not detected. A per-tree generation tag can be added later
  if this becomes a real problem.
- Removal is not supported yet. When it is, it must keep indices stable
  (tombstones or a free list) or remap them in one pass.
- No benchmark numbers yet: the crate has no benchmarks (out of scope for this
  job). The `large_tree` test builds 100,000 nodes and resolves every ID in
  about 0.1 s in release mode on the development laptop, including UUID
  generation; a `criterion` benchmark comparing `HashMap` and `BTreeMap`
  (L2) is follow-up work.
