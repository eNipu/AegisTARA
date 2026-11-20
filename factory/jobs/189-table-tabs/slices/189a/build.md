Commit: 4a67828 Add table link chips, exclusions, owner scoping, filters, settings, and source JSON (189a)
Files: crates/aegis-client/src/table.rs, crates/aegis-client/src/table_filter.rs, crates/aegis-client/src/settings.rs, crates/aegis-client/src/lib.rs, crates/aegis-client/tests/table_features.rs
Done when:
- Existing table convention tests still pass untouched — `cargo test --locked -p aegis-client --test table_conventions`: PASS (9 passed, 0 failed)
- New table feature tests pass for all 8 `TableId`s, link chips, exclusions, filters, settings, and source JSON — `cargo test --locked -p aegis-client --test table_features`: PASS (6 passed, 0 failed)
- Client crate CI passes clean — `scripts/ci.sh crates/aegis-client`: PASS (`fmt`, `clippy`, `test` all ok)
Notes:
- Changed non-test source lines: 460 insertions, 6 deletions (466 total, under the 480 cap).
- For `Owner::Unassigned`, `table_rows_for_owner` excludes shared-element kinds (`Damage`, `Controls`, `Goals`, `Claims`) both for those four tables and when filtering `Problems` targets, matching `ExplorerIndex::marks`.
- `table_source_json` returns exact UTF-8 bytes from `Project::to_workspace_files()` for present files, a pretty-printed empty `aegis-workspace` v1 envelope when absent, and `"{}\n"` for `TableId::Problems`.
NEXT: reviewer
