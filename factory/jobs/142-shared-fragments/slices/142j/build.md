Commit: e9f51b4b0148f0ec0052cc2a13be9f5ee51cdcf7 Extract selected attack-tree subtrees into shared fragments as one undo batch (142j)
Files: crates/aegis-client/src/commands.rs, crates/aegis-client/src/tree_diff.rs, crates/aegis-client/src/undo_label.rs, crates/aegis-client/locales/en/main.ftl, crates/aegis-client/locales/ja/main.ftl, crates/aegis-client/locales/pseudo/main.ftl, crates/aegis-client/tests/shared_fragments.rs
Done when:
1. All specified regression/property/workflow cases pass — `cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded`: PASS (remove_loaded: 3 passed; shared_fragments: 13 passed; undo_every_op: 6 passed).
2. Scoped CI passes — `scripts/ci.sh crates/aegis-client`: PASS (`ok aegis-client fmt`, `ok aegis-client clippy`, `ok aegis-client test`).
Notes:
- Production line delta vs `532863a0f072511a608348d99f76ef91077bcf06`: 134 insertions / 10 deletions (144 changed lines, well within 560 cap).
- Fixed two `tree_diff.rs` edge cases caught by outside-shared descendant collapse tests: `kept_edge` now requires `stays.contains(&g)` so re-added gates keep linked edges to surviving targets, and `prune_shape_ops` does not treat `keep_selected` `node` as a surviving owner so outside-shared children owned by `node` are properly reparented before `SetTreeNodeKind`.
- Added `EditError::AlreadyShared` guard rejecting already shared-step or fragment-linked selections (while keeping nested references inside selected subtrees legal and leaving existing `Edit::MakeShared` behavior unchanged).
- Added `make-shared-fragment` Fluent labels in `en`, `ja`, and `pseudo`, plus `undo_label::describe` recognition for `MakeSharedFragment` (`share fragment '<name>'`, `is_removal() == false`, `Selection::Tree(id)`).
NEXT: reviewer
