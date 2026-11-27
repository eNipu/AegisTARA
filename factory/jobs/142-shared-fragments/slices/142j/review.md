VERDICT: PASS
## For the owner
What changed:
- Added `Edit::MakeSharedFragment { tree, node, id }` and `EditError::AlreadyShared`, extracting any reachable step, gate, or root subtree into a new `TreeRole::Fragment` tree and replacing the host node with a `UseFragment` reference in one atomic op batch.
- Added `tree_diff::collapse_subtree_ops` (`prune_shape_ops(..., keep_selected = true)`) and fixed `kept_edge` (`stays.contains(&g)`) so outside-shared descendants stay and re-parent cleanly without touching host tree metadata or IDs.
- Added `share fragment` undo/redo labeling (`Selection::Tree(id)`), `make-shared-fragment` Fluent entries in `en`/`ja`/`pseudo`, and comprehensive workflow, oracle, undo/redo, and atomic rollback failure tests in `shared_fragments.rs`.
Look at:
- `crates/aegis-client/src/commands.rs:1346` — `make_shared_fragment_ops` ordering: copies subtree, collapses host children, clears selected step metadata and step conditions, converts gates via `SetTreeNodeKind`, and links `UseFragment` while guarding already-shared selections.
- `crates/aegis-client/src/tree_diff.rs:495` and `crates/aegis-client/src/tree_diff.rs:891` — `kept_edge` requiring `stays.contains(&g)` and `prune_shape_ops` reparenting surviving outside-shared descendants originally owned by `node`.
- `crates/aegis-client/src/undo_label.rs:188` — `describe` rule matching `AddAttackTree { role: Fragment }` + `UseFragment` to emit `"share fragment '<name>'"` with `Selection::Tree(id)`.
Risk: Prospective depth/cap/duplicate-ID checks (`FragmentDepth`, `TooManyFragmentReferences`, `DuplicateAttackTree`, `TooLarge`) happen at `Workspace::apply` after planning the new fragment ops, relying on atomic reverse-op rollback (`undo_applied`). Checked by `make_shared_fragment_guards_and_prospective_limits_fail_atomically` (depth-9 on host and inside depth-7 fragment, 256-ref cap overflow and contrast, >520-node `MAX_SUBMIT_OPS` overflow, duplicate tree ID, and pre-existing undo/pending/snapshot/workspace-files/eval equality) plus `make_shared_fragment_workflows_preserve_values_structure_and_loaded_undo` (step/gate/root, AND/OR/SEQ, multi-parent, diamond, outside-shared owned gate with surviving child + linked target, nested refs, >32 nodes, excluded/unrated inputs, independent oracle in both eval modes, and exact byte-level undo/redo).
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff 532863a0f072511a608348d99f76ef91077bcf06..HEAD
## Findings
none
## Lesson
none
NEXT: human
