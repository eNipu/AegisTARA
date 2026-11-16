Commit: db8b3ff Name the other system on a step's via chip and inspector (120c)
Files: crates/aegis-client/src/view.rs, crates/aegis-client/tests/combined.rs
Done when:
1. chip and inspector name the other system: cargo test --locked -p aegis-client --test combined -> 10 passed
2. via tests: --test step_via --test view -> 5 and 8 passed
3. cargo check --locked -p aegis-shell -> ok
Also: cargo fmt --all, scripts/ci.sh crates/aegis-client -> all passed
Notes: Non-test change is 48 added lines in view.rs. system_use is computed once per tree_scene (only when the project has a system) and once per inspector call. The capped test compares the risk row from risk_rows with the row filter_page keeps for the Brake view.
NEXT: reviewer
