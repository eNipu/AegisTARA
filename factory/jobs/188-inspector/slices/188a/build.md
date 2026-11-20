Commit: c7ea5ac30796e1285d4a1644f98c40e9fdecf0a2 Add go-to-cause command, inspector Fluent keys, and core cause resolution (188a)
Files:
- crates/aegis-client/src/command_table.rs
- crates/aegis-client/src/inspector.rs
- crates/aegis-client/src/lib.rs
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-client/tests/command_table.rs
- crates/aegis-client/tests/inspector_view.rs

Done when:
1. `"go-to-cause"` is in `COMMANDS` with translated labels/tooltips in all 3 locales — `cargo test --locked -p aegis-client --test command_table && cargo test --locked -p aegis-client --test palette`: PASS (5 passed in `command_table`, 7 passed in `palette`).
2. `explain_selection`, `exclusion_causes`, `reverse_relation_label`, and `primary_cause` pass unit tests for threats, trees, gates, capped steps, direct-feasibility threats, and off conditions — `cargo test --locked -p aegis-client --test inspector_view`: PASS (2 passed).

Notes:
- `cargo fmt --all` and `scripts/ci.sh crates/aegis-client` pass cleanly.
- Changed source + locale lines total 538 (within `max_lines: 560`), with no `#[rustfmt::skip]`.

NEXT: reviewer
