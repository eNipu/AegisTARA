VERDICT: PASS

No UI, CLI, or API surface in this change. It adds library errors only, so I reviewed those messages.

- crates/aegis-model/src/error.rs (Cycle, DuplicateEdge) — The messages state what went wrong and give the node indices ("cycle in attack tree: 0 -> 1 -> 0"). Nothing fails silently. The docs point callers to `id_of` to get public IDs. Non-blocking: the Display text shows internal indices, not node titles or IDs. The future UI must map the path to node names before showing it to users. Wording is fixed by the spec, so no change is needed now.
- The cycle path is deterministic and runs in O(V + E). It does no heavy work on a UI thread.
