# Rework notes for 002-cycle-detection

## Rework 1

The owner answered the spec's three open questions. These answers replace the spec's defaults.
Update ADR 0002 to record them, with the reasons below, and set its status to "accepted".

### Q1: `link` rejects an edge that would close a cycle

`AttackTree` keeps the invariant "always a DAG". `link(parent, child)` rejects the edge if
`parent` is reachable from `child`, with an iterative DFS over `child`'s descendants (no recursion,
no global state, visited as `Vec<bool>` or a bitset without a new dependency). Cost
O(descendants of child), at most O(V + E); a 100,000-node graph is well under the 50 ms budget.

Why:
- A cycle has no meaning in an attack tree: feasibility (AND/OR aggregation) has no defined value,
  and ISO/SAE 21434 attack paths are acyclic. The model should not hold a state the engine can't
  evaluate (D12).
- The error appears at the edit that causes it, not later at evaluation, far from the cause.
- Future merges and co-editing replay operations through `link`: two edits that are fine alone but
  form a cycle together get a clean conflict on the second one, instead of a broken saved project.

Edge cases that must have tests:
- self-link (`link(a, a)`) → rejected;
- link a node under its own descendant (A→B→C, `link(C, A)`) → rejected;
- link the root under any gate → rejected (the root reaches every node);
- diamond / shared subtree (two gates share one child) → allowed;
- shortcut edge to an existing descendant (A→B→C, `link(A, C)`) → allowed;
- a rejected `link` leaves the tree unchanged.

Keep `topological_order()` and its `Cycle` error unchanged in behavior: it gives the engine its
evaluation order (A2) and stays the cycle check before evaluation (A3), as defense in depth.
Out of scope: a bulk import path that adds many links and validates once with Kahn's algorithm
(O(V + E) total instead of one DFS per link). Mention it in the ADR as the upgrade path.

### Q2: errors carry stable `NodeId`s, not `NodeIdx`

- `Cycle` holds `Vec<NodeId>` (same order rules as now: edge order, smallest first, no repeat).
- New `WouldCreateCycle { parent: NodeId, child: NodeId, path: Vec<NodeId> }` from `link`, where
  `path` is the existing chain from `child` down to `parent`; the new edge would close it.
- `DuplicateEdge { parent, child }` uses `NodeId` too.
- Display shows the hyphenated UUIDs. Never put labels in errors: labels are customer data and
  errors reach logs.

Why: `NodeIdx` is only valid inside one in-memory tree. `NodeId` is the same in project files,
logs, the server, and after a reload, so a bug report can be traced to the exact nodes. The UI
gets the index back with `idx_of` in O(1). `UnknownNode` stays a `NodeIdx` (it has no ID).
Job 001's `NotAGate(NodeIdx)` is out of scope.

### Q3: rename `parent()` to `owner()`, add `parents()`

- `owner(idx) -> Option<NodeIdx>`: the gate the node was created under (`add_child`). Used later for
  tree layout and nested serialization.
- `parents(idx) -> impl Iterator<Item = NodeIdx>`: every gate that holds the node, owner first,
  then linked parents in link order (deterministic, P4). O(number of parents).
- Store linked parents as a per-node `Vec<NodeIdx>` kept by `link` in O(1); an empty `Vec` does not
  allocate, so plain tree nodes cost nothing extra.
- Extend the invariants check: for every edge parent→child, `parent` is in `parents(child)` exactly
  once, and the reverse.

Why: in a DAG a node can have several parents. The engine's incremental recompute (A1) must
re-evaluate *all* ancestors of a changed node, and future delete and change-impact features need
all parents too. A `parent()` that silently returns one of them is a trap that gives stale scores
(P1), and finding all parents without an index costs O(V + E) per query.
