# ADR 0002: DAG links and cycle detection

- Status: accepted (job 002-cycle-detection, rework 1: owner answered the
  three open questions)
- Date: 2026-10-19
- Crate: `crates/aegis-model`
- Rules: A1, A2, A3, A8, A9, A10, D12, P1, P4, D8 in `docs/engineering-standards.md`
- Extends: `0001-arena-and-typed-ids.md`

## Context

Attack trees share subtrees: one step such as "obtain key fob" can sit under
two OR gates. Job 001 only had `add_child`, so the model was always a tree.
Sharing turns it into a DAG, and once edges can point to existing nodes, an
edit (or, later, an import, merge, or co-editing replay) could close a cycle.
The engine evaluates in topological order (A2) and must never recurse forever
(A3).

Three questions were open in the spec and were answered by the owner:

1. Does `link` accept an edge that closes a cycle, or reject it?
2. Do errors carry internal `NodeIdx` values or stable `NodeId`s?
3. With shared nodes, what does `parent()` mean, and how are all parents
   found?

## Q1: where cycles are caught

| Option | Cost | Notes |
|---|---|---|
| Permissive `link`, cycles found later by `topological_order()` | `link` O(k), k = children of the parent | The model can hold a state the engine cannot evaluate; the error appears far from the edit that caused it. |
| **Eager rejection in `link`** (reachability check from `child` to `parent`) | O(descendants of `child` + their edges), at most O(V + E) per call | The model is always a DAG. The error appears at the edit that causes it. |

**Decision: eager rejection.** `link(parent, child)` runs an iterative DFS over
`child`'s descendants. If `parent` is reached (including `parent == child`),
the link is refused with `WouldCreateCycle` and the tree is unchanged.

Why:

- A cycle has no meaning in an attack tree: feasibility (AND/OR aggregation)
  has no defined value on a cycle, and ISO/SAE 21434 attack paths are acyclic.
  The model should not hold a state the engine cannot evaluate (D12).
- The error appears at the edit that causes it, not later at evaluation.
- Future merges and co-editing replay operations through `link`: two edits that
  are fine alone but form a cycle together give a clean conflict on the second
  one, instead of a broken saved project.

Cost: the DFS visits only nodes reachable from `child`, each once (visited
flags), and their child edges, plus one zeroed O(V) allocation for the flags
(`vec![false; V]`, a single `calloc`, not a scan). The worst case, linking the
root's whole graph, is O(V + E); for the `large` project (100,000 nodes) that is
far below the 50 ms interaction budget. The DFS keeps the current path on its
own stack (node, next child position), so the path for the error comes for free
and a 100,000-deep chain needs no recursion (test `deep_chain_rejects_cycle`).
Visited flags are a `Vec<bool>`, not `fixedbitset` (D8 is a SHOULD): no new
dependency, and the flag array is short-lived.

Edge cases, each with a test in `src/tree.rs` (`mod links`): self-link
(rejected), linking a node under its own descendant (rejected), linking the
root under any gate (rejected; the root reaches every node), a cycle that runs
through an earlier link (rejected), a diamond/shared subtree (allowed), a
shortcut edge to an existing descendant (allowed), and a rejected link leaves
children and parents unchanged.

**Upgrade path (out of scope):** a bulk import that adds many links would pay
one DFS per link, O(L (V + E)) for L links. It can instead add all edges and
validate once with Kahn's algorithm, O(V + E) total, rejecting the whole batch
with the `Cycle` path.

### `topological_order` stays, as evaluation order and defense in depth

`topological_order()` keeps its behavior: it is the engine's evaluation order
(A2) and the cycle check before evaluation (A3). Because `link` keeps the DAG
invariant, its `Cycle` error should never fire for a tree built through the
public API; it guards against future bugs and future code paths that build
arenas directly (deserialization).

| Algorithm option | Cost | Notes |
|---|---|---|
| **Kahn (1962)** | O(V + E) time, O(V) space | Iterative (queue), so no stack overflow on deep chains. The order is a by-product, which the engine needs anyway. Leftover nodes prove a cycle exists. |
| Tarjan SCC (1972) | O(V + E) | Finds all strongly connected components, so all cycles at once. Usually written recursively. Reporting all cycles is out of scope. |
| Recursive DFS with colors | O(V + E) | Simple, but recursion depth = graph depth: a 100,000-node chain overflows the default 2 MB test thread stack. |

Cycle path extraction after Kahn stops:

1. For each remaining node, record its first remaining predecessor (parents in
   ascending index, children in stored order). Every remaining node has one,
   because its in-degree only counts edges from remaining parents.
2. From the smallest remaining index, walk predecessors, recording each node's
   position in the walk, until a node repeats. The part of the walk from the
   first visit of the repeated node is a cycle.
3. Reverse it into edge direction and rotate so the smallest index comes first.

O(V + E) time and O(V) space. The scan order and rotation make the result
deterministic (P4). Since cycles cannot be built through the public API, this
path is tested at the arena level in `src/topo.rs`, including a property test
against an independent three-color DFS and a 100,000-node cyclic chain.

## Q2: errors carry stable `NodeId`s

**Decision:** `Cycle(Vec<NodeId>)`, `WouldCreateCycle { parent, child, path:
Vec<NodeId> }` (path = existing chain from `child` down to `parent`), and
`DuplicateEdge { parent: NodeId, child: NodeId }`. Display shows hyphenated
UUIDs. Errors never contain labels. `UnknownNode` stays a `NodeIdx` (an
out-of-range index has no ID); job 001's `NotAGate(NodeIdx)` is unchanged (out
of scope).

Why: a `NodeIdx` is only valid inside one in-memory tree. A `NodeId` is the
same in project files, logs, the server, and after a reload, so a bug report
can be traced to the exact nodes. The UI gets the index back with `idx_of` in
O(1). Labels are customer data and errors reach logs, so they are excluded.

Cost: mapping a path to IDs is O(path length), only on the error path.

## Q3: `owner()` and `parents()`

**Decision:** `parent()` is renamed `owner()`: the gate a node was created under
with `add_child` (for tree layout and nested serialization later). New
`parents(idx)` iterates every gate that holds the node: the owner first, then
linked parents in link order (deterministic, P4), O(number of parents), no
allocation. Linked parents are stored per node in a `Vec<NodeIdx>` that `link`
appends to in O(1) amortized; an empty `Vec` does not allocate, so a plain tree
node costs only its 24-byte header.

Why: in a DAG a node can have several parents. Incremental recompute (A1) must
re-evaluate *all* ancestors of a changed node, and future delete and
change-impact features need all parents too. A `parent()` that silently
returned one of them would be a trap that gives stale scores (P1), and finding
all parents without an index costs O(V + E) per query.

The test-only invariant check verifies, for every edge `p -> c`, that `p` is
in `parents(c)` exactly once, and the reverse, and that the graph is acyclic.

## Complexity

| Operation | Time | Extra space |
|---|---|---|
| `link` | O(k + d + e): k = children of `parent`, d, e = descendants of `child` and their edges; at most O(V + E) | O(V) (visited flags, DFS stack) |
| `owner` | O(1) | none |
| `parents` | O(number of parents) | none |
| `topological_order` (Ok) | O(V + E) | O(V) plus the output |
| `topological_order` (Cycle) | O(V + E) | O(V) plus the path |

Nothing is O(V^2) in a single interactive call (A9). All graph walks are
iterative.

## Consequences

- The model is always a DAG; consumers can rely on it, and
  `topological_order()` still re-checks before evaluation.
- Building a graph with L links through `link` costs O(L (V + E)) worst case;
  bulk import should use the batch upgrade path above.
- Only one cycle is reported by `Cycle`. Tarjan SCC can be added if needed.
- The order is recomputed from scratch each call. Incremental or cached order
  (A1) is future work.
- Each node carries one more `Vec` header (24 bytes) for linked parents.
  ADR 0004 (`0004-linked-parents-storage.md`) measures this layout against
  boxed slices and a sparse map, and records whether to keep it.
