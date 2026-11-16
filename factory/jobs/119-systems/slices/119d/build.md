Commit: 4d0e2d7 Flag a top-level component that is in no system (119d)
Files: crates/aegis-model/src/lint.rs, crates/aegis-model/src/project.rs, crates/aegis-client/src/pages.rs, crates/aegis-model/tests/systems.rs, crates/aegis-client/tests/systems.rs
Done when:
1. `cargo test --locked -p aegis-model --test systems --test ops_properties` - ok (11 and 17 passed)
2. `cargo test --locked -p aegis-client --test systems --test pages` - ok (22 and 1 passed)
Also: cargo fmt --all and scripts/ci.sh for both crates pass.
Notes: 39 non-test lines changed. ops_properties.rs not touched (no count changed). The new variant follows BoundaryCrossingDataFlowWithoutThreat, which is the last variant in the enum (LibraryRatingNotReviewed sits earlier). Model tests cover: no systems, top-level reported, child of a system component not reported, child of a component in no system not reported, finding clears after SetComponentSystem.
NEXT: reviewer
