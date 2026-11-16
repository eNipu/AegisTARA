Commit: 550d275 Add boundary flows and warn when none is analysed (120a)
Files: crates/aegis-model/src/system.rs, crates/aegis-model/src/lint.rs, crates/aegis-model/src/project.rs, crates/aegis-model/tests/systems.rs, crates/aegis-client/src/pages.rs, crates/aegis-client/tests/systems.rs
Done when:
1. cargo test --locked -p aegis-model --test systems --test ops_properties --test project: pass
2. cargo test --locked -p aegis-client --test systems --test pages: pass
3. cargo test --locked -p aegis-model --doc boundary_flow and --lib lint: pass
Before commit: cargo fmt --all; scripts/ci.sh crates/aegis-model crates/aegis-client: all passed.
Notes: Non-test source change is about 88 lines (including the doc test). The existing lint looks only at the direct end components (not ancestors), so the "cleared by a threat" test binds the asset to the child component that is the flow's end. The new lint reuses that rule unchanged.
NEXT: reviewer
