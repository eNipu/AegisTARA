Commit: fa3c651aaaba16254ec7511960f0bb6ab7123656 Add shared-step edits and inspector routing (141e)
Follow-ups: 9877d39 Handle unused shared vias and missing edit targets (141e); 13270f6 Restore direct shared-step unlinking (141e)
Files: crates/aegis-client/locales/{en,ja,pseudo}/main.ftl; crates/aegis-client/src/{view,relations,commands,tree_diff,undo_label,picker,inspector,pages,table,systems}.rs; crates/aegis-client/src/grid/edit.rs; crates/aegis-client/tests/{shared_steps,relations,inspector_view,table_features,undo_every_op}.rs; crates/aegis-workbench/src/app/{inspector,workbench}.rs; crates/aegis-workbench/tests/inspector_view.rs
Done when:
- cargo fmt --all: PASS
- cargo test --locked -p aegis-client --test shared_steps --test relations --test inspector_view --test undo_every_op: PASS (30 tests)
- cargo test --locked -p aegis-workbench --test inspector_view: PASS (5 tests)
- scripts/ci.sh crates/aegis-client crates/aegis-workbench: PASS (fmt, clippy, test for both crates)
- python3 factory/verify.py 141-shared-steps 141e 284d1d41c5ec7d0d94942c2fc485c20986045d39: GATE PASS (700 counted source/locale lines, cap 720)
Notes:
- Implemented all 11 edits as atomic ordered to_ops arms; shared values are read from the same pre-batch project and Project::shared_step_via is used publicly.
- Exact single-undo coverage includes MakeShared, Unshare, direct UnuseSharedStep, RemoveSharedStep with description/translation/users, direct and gate-subtree node removal, RemoveTree, and ReplaceTree moves/drops.
- Review fixes: clear_via_ops includes zero-user shared steps for component/interface removal; missing remove/unuse targets return EditError::Missing; valid direct UnuseSharedStep reaches the unlink logic and preserves all four rating fields.
- Temporary SharedStep page/table/system stubs remain for 141f. The legacy headless Inspector uses English because UiState carries no locale; InspectorView is localized.
- No push or merge performed.
NEXT: reviewer
