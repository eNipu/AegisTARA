# 001-aegis-model: aegis-model crate, arena attack tree with typed IDs

## Open questions

1. **Crate location.** There is no workspace yet (backlog item "Workspace skeleton" is separate). This spec assumes `crates/aegis-model/` with its own `Cargo.toml`. The approver should confirm the path.
2. **Edges between existing nodes.** This job builds trees only: a node is created already attached to its parent, so cycles cannot happen. Job 002 (cycle detection, A3) needs a `link(parent, child)` operation that can make a DAG. This spec leaves `link` to job 002. The approver should confirm that split.
3. **Step payload.** Feasibility factors on steps belong to the engine/pack work. This spec gives steps only a label. Confirm.

## Goal

Production code gets its first crate, `aegis-model`. It holds an attack tree (AND, OR, and sequential AND gates, plus leaf steps) in an arena (`Vec<Node>`) and refers to nodes by a typed `NodeIdx(u32)`. Each node also has a stable public `NodeId` (UUIDv7) that files, APIs, and history will use. One map resolves `NodeId` to `NodeIdx` in O(1). Users do not see anything new yet. Later work (engine, store, desktop client) builds on these types, and they replace the spike's recursive `find` (O(n)) and its `String` IDs (standards section 3, "Lesson from our own spike").

## Scope

In:
- New library crate `crates/aegis-model` (edition 2024, `publish = false`, `license = "LicenseRef-Proprietary"`, `rust-version` pinned (Q7)).
- Newtypes `NodeIdx(u32)` and `NodeId(Uuid)` (D2, D3). `NodeId` is created from a UUID only if it is version 7 (Q4: parse, don't validate).
- `AttackTree`: node arena, root, parent and child links, and the `NodeId` to `NodeIdx` map.
- `Node` as an enum: `Gate { kind: GateKind, children }` and `Step { .. }` (D12). `GateKind = And | Or | SeqAnd`.
- Labels stored as `Arc<str>` (D5).
- Typed errors with `thiserror` (Q3). No `unwrap`/`expect`/panicking indexing in library code.
- Unit tests, `proptest` property tests, and doctests on every public item (Q5).
- A short decision record `docs/adr/0001-arena-and-typed-ids.md` (L1): arena vs `Rc<RefCell>`, map choice, `u32` vs `usize`.

Out:
- Linking existing nodes / DAGs, cycle detection (job 002).
- Node removal, moving, editing labels, undo, operations log, snapshots (D9).
- Feasibility factors, scoring, explain-why (engine).
- Serialization (serde), file formats, `rkyv`.
- Assets, threats, damage scenarios, other entity IDs (`AssetIdx`, `FactorIdx`).
- Cargo workspace, `cargo-deny`, CI, benchmarks. Copying code from `spikes/spike-model` (L3).

## Design notes

Files:
- `crates/aegis-model/Cargo.toml`
- `crates/aegis-model/src/lib.rs`: `#![forbid(unsafe_code)]` (Q2), `#![warn(missing_docs)]`, `#![warn(clippy::pedantic)]`, re-exports.
- `crates/aegis-model/src/id.rs`: `NodeIdx`, `NodeId`.
- `crates/aegis-model/src/tree.rs`: `Node`, `GateKind`, `AttackTree`.
- `crates/aegis-model/src/error.rs`: `ModelError`.
- `crates/aegis-model/tests/properties.rs`: proptest.
- `docs/adr/0001-arena-and-typed-ids.md`.

Dependencies (all MIT or Apache-2.0, permitted by decision D5): `uuid` (features `v7`), `thiserror`. Dev: `proptest`. No other crates.

Types (sketch, names are binding, field layout is not):

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIdx(u32);          // no public constructor; only AttackTree hands them out
impl NodeIdx { pub fn index(self) -> usize; }

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(Uuid);
impl NodeId {
    pub fn new_v7() -> Self;                                  // fresh UUIDv7
    pub fn from_uuid(u: Uuid) -> Result<Self, ModelError>;    // rejects non-v7
    pub fn as_uuid(&self) -> &Uuid;
}
impl FromStr for NodeId { type Err = ModelError; }           // parse + v7 check
impl Display for NodeId;                                     // hyphenated lowercase

pub enum GateKind { And, Or, SeqAnd }
pub enum Node {
    Gate { kind: GateKind, label: Arc<str>, children: Vec<NodeIdx> },
    Step { label: Arc<str> },
}

pub struct AttackTree {
    nodes: Vec<Node>,              // arena, index = NodeIdx
    ids: Vec<NodeId>,              // index -> public ID (parallel array, D4)
    parents: Vec<Option<NodeIdx>>, // index -> parent (root: None)
    by_id: HashMap<NodeId, NodeIdx>, // public ID -> index
    root: NodeIdx,
}
```

Public API on `AttackTree` (each project-sized function documents time and space big-O in its doc comment, A8):

| Function | Cost |
|---|---|
| `new(root_id: NodeId, root: Node) -> Result<Self, ModelError>` (root must have no children) | O(1) |
| `add_child(parent: NodeIdx, id: NodeId, node: Node) -> Result<NodeIdx, ModelError>` (`node` given with empty children) | O(1) amortized |
| `root() -> NodeIdx`, `len()`, `is_empty()` | O(1) |
| `get(idx) -> Option<&Node>` | O(1) |
| `idx_of(&NodeId) -> Option<NodeIdx>` | O(1) expected |
| `id_of(idx) -> Option<NodeId>` | O(1) |
| `parent(idx) -> Option<NodeIdx>`, `children(idx) -> &[NodeIdx]` (empty for steps and unknown indices) | O(1) |
| `iter() -> impl Iterator<Item = (NodeIdx, NodeId, &Node)>` in insertion (arena) order | O(n) total |

Errors (`ModelError`, `#[non_exhaustive]`): `NotVersion7`, `InvalidUuid`, `DuplicateId(NodeId)`, `UnknownNode(NodeIdx)`, `NotAGate(NodeIdx)` (adding a child to a step), `ChildrenNotEmpty` (new node passed with children), `CapacityExceeded` (more than `u32::MAX` nodes). A failed call leaves the tree unchanged.

Map choice (D7): `NodeId` values come from files and network, so they are untrusted keys. Use `std::collections::HashMap` (SipHash). The map is never iterated for output; deterministic order comes from the arena (`iter`). Record this in the ADR.

Index conversion: one private `fn NodeIdx::try_from_usize(usize) -> Option<NodeIdx>` is the only place that turns a `usize` into a `NodeIdx`. Unit test it at the `u32::MAX` boundary, since building 4 billion nodes in a test is not practical.

Invariants (checked by tests): `nodes.len() == ids.len() == parents.len() == by_id.len()`; for every index `i`, `by_id[ids[i]] == i`; every non-root node has exactly one parent and appears exactly once in that parent's `children`; the root has no parent.

Memory: per node about one enum (label `Arc<str>` + `Vec`), one 16-byte UUID, one `Option<u32>`, one map entry. No per-lookup allocation.

## Acceptance criteria

All commands run from `crates/aegis-model/` unless noted.

- [ ] AC1: The crate builds as a library with edition 2024, `rust-version` set, `#![forbid(unsafe_code)]` in `src/lib.rs`, and only the dependencies listed above. Check: `cargo build` and `grep -n 'forbid(unsafe_code)' src/lib.rs` and `cargo tree --depth 1 -e normal` shows only `uuid` and `thiserror`.
- [ ] AC2: `NodeIdx` and `NodeId` are distinct types; passing one where the other is expected does not compile. Check: a `compile_fail` doctest on `NodeIdx`, run by `cargo test --doc`.
- [ ] AC3: `NodeId::from_uuid` and `NodeId::from_str` accept a v7 UUID and reject a v4 UUID (`NotVersion7`) and malformed text (`InvalidUuid`); `Display` then `from_str` round-trips. Check: `cargo test id::`.
- [ ] AC4: Building the spike's 13-node tree shape (`small` reference) through `new` and `add_child` gives `len() == 13`, correct `parent`/`children` for every node, and `idx_of(id_of(i)) == Some(i)` for all nodes. Check: `cargo test tree::small_tree`.
- [ ] AC5: `add_child` returns `DuplicateId` for a reused `NodeId`, `NotAGate` when the parent is a step, `UnknownNode` for an index from another, larger tree, and `ChildrenNotEmpty` for a node passed with children; after each error the tree is unchanged (`len` and lookups equal the state before). Check: `cargo test tree::errors`.
- [ ] AC6: `NodeIdx::try_from_usize` returns `Some` for `u32::MAX as usize` and `None` for `u32::MAX as usize + 1` (on 64-bit targets). Check: `cargo test id::capacity`.
- [ ] AC7: Property test: for any random sequence of up to 2,000 `add_child` calls (random existing parent, random gate kind or step, some reused IDs), the invariants in Design notes hold after every call, and `iter()` returns nodes in insertion order. Check: `cargo test --test properties`.
- [ ] AC8: A 100,000-node tree (`large` reference size, generated in the test) builds and resolves every `NodeId` to the right index. No timing assertion. Check: `cargo test --release tree::large_tree`.
- [ ] AC9: No `unwrap(`, `expect(`, or `panic!` in non-test library code, except at sites with a `// INVARIANT:` comment. Check: `grep -nE 'unwrap\(|expect\(|panic!' src/*.rs` lists only `#[cfg(test)]` code or commented invariant sites.
- [ ] AC10: Every public item has a doc comment, and every public function on `AttackTree` states its time complexity. Check: `cargo doc --no-deps` with `#![warn(missing_docs)]` produces no warnings (`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`).
- [ ] AC11: `docs/adr/0001-arena-and-typed-ids.md` exists (from repo root) with context, options considered (arena + IDs vs `Rc<RefCell>`; `HashMap` vs `BTreeMap` vs `FxHashMap`; `u32` vs `usize`), decision, and consequences. Check: `test -f docs/adr/0001-arena-and-typed-ids.md`.
- [ ] AC12: Nothing under `spikes/` changes. Check (repo root): `git diff --stat master -- spikes/` is empty.

## Checks

From `/Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model`:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --release tree::large_tree
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```
