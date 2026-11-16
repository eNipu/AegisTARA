Commit: 4e0b024 Filter every page by the system view without an op (119h)
Files: crates/aegis-client/src/{systems,view,state,commands,session}.rs, crates/aegis-client/tests/systems_view.rs, crates/aegis-client/tests/session.rs
Done when:
1. `cargo test --locked -p aegis-client --test systems_view` - 7 passed.
2. `cargo test --locked -p aegis-client --test session` - 23 passed (includes the new view test, edit and read-only).
3. `scripts/ci.sh` - fmt, clippy, workspace test all ok; aegis-shell unchanged.
Notes:
- Non-test source change is about 125 lines (cap 300).
- tests/systems_view.rs already existed from 119g; the filtering tests were appended in a `filtering` module.
- The Problems filter treats a `TreeNode` by its tree, and rows with no target or an unknown ID are kept.
NEXT: reviewer

## Rework 1
Commit: 535a376 Test the system view filter on every page and the Threats list (119h)
Fixed the one finding: the fixture now gives each component a control (applied to it), and each threat a goal and a claim. `shown` now also covers Controls, Goals (goals and claims) and Problems, and calls `filter_threats` directly (`th:` entries). The test asserts per-kind counts for the all view and the Brake view (the ADAS rows are dropped, "Loose" rows stay) and that Problems shrink under Brake. Only the test file changed.
Not covered: Item data flows and trust boundaries (the fixture has none), and Problems with a `TreeNode` target.
Checks: systems_view 7 passed, session 23 passed, systems 12 passed, scripts/ci.sh all steps passed.
NEXT: reviewer
