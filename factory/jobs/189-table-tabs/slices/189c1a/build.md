Commit: 9496c74 Register grid commands, i18n keys, icons, empty-op guard, and workbench hooks (189c1a)
Files: crates/aegis-client/src/command_table.rs, crates/aegis-client/src/i18n.rs, crates/aegis-client/locales/en/main.ftl, crates/aegis-client/locales/ja/main.ftl, crates/aegis-client/locales/pseudo/main.ftl, crates/aegis-client/tests/command_table.rs, crates/aegis-client/tests/palette.rs, crates/aegis-workbench/src/local.rs, crates/aegis-workbench/src/app/icons.rs, crates/aegis-workbench/src/app/commands.rs, crates/aegis-workbench/src/app/workbench.rs, crates/aegis-workbench/tests/command_wiring.rs
Done when:
1. Command table tests pass across en, ja, and pseudo (including ⌘⌫ and Ctrl+Del shortcut labels) — `cargo test --locked -p aegis-client --test command_table` -> 5 passed, 0 failed
2. Palette tests pass with zero regressions and verify toggle-developer-source vs grid-* visibility — `cargo test --locked -p aegis-client --test palette` -> 7 passed, 0 failed
3. Workbench command wiring passes (every command has icon, handler, and binding) and empty-op short-circuit preserves redo — `cargo test --locked -p aegis-workbench --test command_wiring` -> 4 passed, 0 failed
4. Client and workbench CI pass clean — `scripts/ci.sh crates/aegis-client crates/aegis-workbench` -> ok (fmt, clippy, test passed in both crates)
Notes:
- Changed source lines (excluding `tests/`, including `locales/*`): 227 insertions + 10 deletions = 237 lines (under `max_lines: 360`).
- `Workbench::changed_skipping` calls `self.set_note(res.err(), cx)` and skips updating any child tab or explorer entity whose `entity_id()` matches `skip`, avoiding double-lease panics while exercising `set_note` without `#[allow(dead_code)]`.
- Added GPUI integration coverage in `crates/aegis-workbench/tests/command_wiring.rs` for `toggle-developer-source` persistence/reload, `apply_edit`, and `open_target`, plus the empty-op `Edit::Batch(vec![])` undo/redo preservation test.

## Rework 1
Commit: 2d09684 Tighten open_target assertions and test skip and read-only guards (189c1a)
Files: crates/aegis-workbench/src/app/workbench.rs, crates/aegis-workbench/tests/command_wiring.rs
Fixes:
- `crates/aegis-workbench/tests/command_wiring.rs`: asserted `state.selected` starts at `Some(ExplorerKey::Root)` before `open_target`, changes away from `ExplorerKey::Root` to `ExplorerKey::Element(_, _, s) if s == sel`, and opens the element tab (`has_tab(key, cx)`).
- `crates/aegis-workbench/tests/command_wiring.rs`: added read-only coverage calling `set_read_only(Some("locked for review"), cx)` and asserting `apply_edit` returns `false`, sets `note()` to `"locked for review"`, and leaves both project description and `confirmed_seq()` unchanged.
- `crates/aegis-workbench/src/app/workbench.rs`: added GPUI unit test `apply_edit_skipping_avoids_double_lease_and_refreshes_siblings` calling `apply_edit_skipping` from inside `ElementTab::update`, `ListTab::update`, and `ExplorerPanel::update` with `Some(entity_id)`, verifying no double-lease panic, zero notifications on the skipped entity, and live refresh on sibling tabs/explorer.
Done when:
1. Command table tests pass across en, ja, and pseudo — `cargo test --locked -p aegis-client --test command_table` -> 5 passed, 0 failed
2. Palette tests pass with zero regressions — `cargo test --locked -p aegis-client --test palette` -> 7 passed, 0 failed
3. Workbench command wiring passes — `cargo test --locked -p aegis-workbench --test command_wiring` -> 4 passed, 0 failed
4. Client and workbench CI pass clean (including `apply_edit_skipping_avoids_double_lease_and_refreshes_siblings`) — `scripts/ci.sh crates/aegis-client crates/aegis-workbench` -> ok (fmt, clippy, test passed in both crates)
Notes:
- Total changed source lines vs `de2095a4` (excluding `tests/`, including `locales/*`): 337 insertions + 10 deletions = 347 lines (under `max_lines: 360`).
NEXT: reviewer
