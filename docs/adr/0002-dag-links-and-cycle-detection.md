# ADR 0002: DAG links and cycle detection

- Status: proposed (job 002-cycle-detection)
- Date: 2026-10-19
- Crate: `crates/aegis-model`
- Rules: A2, A3, A8, A9, A10, P4, D8 in `docs/engineering-standards.md`
- Extends: `0001-arena-and-typed-ids.md`

## Context

Attack trees share subtrees: one step such as "obtain key fob" can sit under
two OR gates. Job 001 only had `add_child`, so the model was always a tree.
Sharing turns it into a DAG, and once edges can point to existing nodes, a
mistake (or a bad import or merge) can close a cycle. The engine evaluates in
topological order (A2) and must never recurse forever, so the model has to
order nodes and report a cycle with its path before evaluation (A3).

## Options

### Where cycles are caught

| Option | Cost | Notes |
|---|---|---|
| **Permissive `link`**, cycles found by `topological_order()` | `link` O(k), k = children of the parent | One check before evaluation catches cycles from every source: editing, file import, merges. |
| Eager rejection in `link` (reachability check from `child` to `parent`) | O(V + E) per `link` call | The model never holds a cycle, but imports and merges still need the full check, so the check exists twice. Building a graph with E links costs O(E (V + E)). |

### Algorithm

| Option | Cost | Notes |
|---|---|---|
| **Kahn (1962)** | O(V + E) time, O(V) space | Iterative (queue), so no stack overflow on deep chains. The order is a by-product, which the engine needs anyway. Leftover nodes prove a cycle exists. |
| Tarjan SCC (1972) | O(V + E) | Finds all strongly connected components, so all cycles at once. Usually written recursively; an iterative version is longer. Reporting all cycles is out of scope. |
| Recursive DFS with colors | O(V + E) | Simple, but recursion depth = graph depth: a 100,000-node chain overflows the default 2 MB test thread stack. |

### Cycle path extraction

Kahn's algorithm says *that* a cycle exists (some nodes keep in-degree > 0), but
not *which* one. After it stops:

1. For each remaining node, record its first remaining predecessor (parents in
   ascending index, children in stored order). Every remaining node has one,
   because its in-degree only counts edges from remaining parents.
2. From the smallest remaining index, walk predecessors, recording each node's
   position in the walk, until a node repeats. Following predecessors forever
   in a finite set must repeat; the part of the walk from the first visit of
   the repeated node is a cycle.
3. Reverse it into edge direction and rotate so the smallest index comes first.

Each step is O(V + E) time and O(V) space (a predecessor array and a
visit-position array). The scan order and the rotation make the result
deterministic (P4): the same graph always reports the same cycle.

## Decision

- `AttackTree::link(parent, child)` is permissive: it rejects unknown nodes,
  step parents, and duplicate edges (`DuplicateEdge`), but accepts edges that
  close a cycle, including self-links. It keeps `parent(child)` as the owning
  parent from `add_child`.
- `AttackTree::topological_order()` uses Kahn's algorithm. The queue starts
  with in-degree-0 nodes in ascending index and children are visited in stored
  order, so the result is deterministic. On a cycle it returns
  `ModelError::Cycle(Vec<NodeIdx>)` with one cycle, extracted as above.
- The path holds internal `NodeIdx` values, like `UnknownNode`; callers map
  them to public IDs with `id_of`.
- Visit marks use plain `Vec`s instead of `fixedbitset` (D8): the walk needs a
  position per node, not a single bit, so a bitset would not save the array,
  and no new dependency is added.

## Complexity

| Operation | Time | Extra space |
|---|---|---|
| `link` | O(k), k = children of `parent` | O(1) amortized |
| `topological_order` (Ok) | O(V + E) | O(V) plus the output |
| `topological_order` (Cycle) | O(V + E) | O(V) plus the path |

Nothing is O(V^2) (A9). Both paths are iterative; a 100,000-gate chain, with
and without a closing link, runs on the default test thread (test
`deep_chain_*` in `tests/cycles.rs`).

## Consequences

- The model can hold a cycle between edits. Every consumer that walks the graph
  must call `topological_order()` first (the engine will, before scoring).
- Only one cycle is reported. Tarjan SCC can be added later if users need all
  cycles at once.
- The order is recomputed from scratch each call. Incremental or cached order
  (A1) is future work.
- No multi-parent query yet: shared nodes have one owning parent plus extra
  parents that are visible only through `children`.
