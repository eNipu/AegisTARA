Commit: 3b9abd6 Add condition edits, Configuration page view, and conditions columns (135a)
Files:
- crates/aegis-client/src/commands.rs
- crates/aegis-client/src/configuration.rs
- crates/aegis-client/src/lib.rs
- crates/aegis-client/src/pages.rs
- crates/aegis-client/src/session.rs
- crates/aegis-client/src/table.rs
- crates/aegis-client/src/view.rs
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-client/tests/configuration.rs
- crates/aegis-client/tests/table_conventions.rs
Done when:
1. `Edit::AddCondition`, `RenameCondition`, `SetConditionPresent` work, undo in one step — `cargo test --locked -p aegis-client --test configuration` -> ok (5 passed)
2. `Page::Configuration` is in group 1 and `process_map` still passes — `cargo test --locked -p aegis-client --test process_map` -> ok (5 passed)
3. Shell and workbench still compile with their tests — `cargo check --locked -p aegis-shell -p aegis-workbench --tests` -> ok
4. Full client CI — `cargo fmt --all` and `scripts/ci.sh crates/aegis-client` -> ok (fmt, clippy, test all passed)
Notes:
- Updated `crates/aegis-client/tests/table_conventions.rs` (`only_inputs_are_editable`) to include the new editable `"conditions"` link column on `Assets`, `Threats`, `Goals`, and `Claims`.
- No exhaustive `Page` match edits were needed in `aegis-shell` or `aegis-workbench`; both compiled cleanly with `--tests`.
NEXT: reviewer
