Commit: 704a17f Put a top-level component in a system (119b)
Files: crates/aegis-model/src/{item,op,error,element,project,lib?}.rs (lib.rs untouched), src/store/{snapshot,op_log,wire,workspace}.rs, tests/systems.rs (new), tests/common/mod.rs, tests/ops_properties.rs, crates/aegis-client/tests/undo_every_op.rs
Done when:
1. `cargo test --locked -p aegis-model --test systems` - 15 passed
2. `cargo test --locked -p aegis-model --test ops_properties` - 11 passed (counts 56 and 53; share and error count unchanged, no other number edited)
3. `cargo test --locked -p aegis-model --test store --test workspace --test store_properties --test workspace_properties` - all passed
4. `cargo test --locked -p aegis-client --test undo_every_op` - 2 passed
Before-commit: `cargo fmt --all` and `scripts/ci.sh crates/aegis-model crates/aegis-client` all passed.
Notes:
- Non-test source change: 200 lines added, 8 removed (cap 400).
- Blank actor error is `ModelError::EmptyActor`; the check runs last in `check_set_component_system` (L15), also when `system` is `None`.
- Snapshot load applies pending systems after the element loop and after pending parents, so a system on a child gives `ChildComponentSystem`.
- `describe` in tests/common does not list components, so nothing was added there.
- `Project::element` picker now includes systems; op 58 adds a System when `s.c % 3 == 0`.
- `ElementKind` and `ElementId` imports added to project.rs; `ElementId` to item.rs.
NEXT: reviewer
