Commit: aab5c31 Read a System column in the Excel import (121c)
Files:
- crates/aegis-client/src/excel.rs (+53 -3)
- crates/aegis-client/tests/system_reports.rs (+248, tests)

Done when:
1. `cargo test --locked -p aegis-client --test system_reports`: 18 passed (6 new).
2. `cargo test --locked -p aegis-client --test report_builder --test reduced_by_report`: 14 + 14 passed; `cargo test --locked -p aegis-client --lib excel`: 6 passed.
3. `scripts/ci.sh`: exit 0 (fmt, clippy, test; all steps passed in 8 crates). `cargo fmt --all` run before commit.

Notes:
- `system_col` in `ExcelMapping` (not in `is_empty`), auto-detect arm `system | systems | システム`; `systems_to_add` in `ExcelPreview` and in `summary` after the components part.
- `preview_import`: `systems` name map (lower-cased) from `project.elements(ElementKind::System)`. Only when the row has a component and a non-empty system name: reuse or add the system (`AddElement` kind System), current system is `None` for a component added in this row (`comp_added > 0`), otherwise `sim.component_idx(&cid).and_then(|i| sim.component_system(i))`; `SetComponentSystem` only when different. The map is updated only after the row succeeds; `SetComponentSystem` counts in `updates_and_links`.
- Deviation in `a_system_without_a_component_is_ignored`: the prompt's headers `["Asset", "Systems"]` give an unmapped row, because the model refuses an asset with no security properties ("asset ... has no security properties"), which is outside this slice. The test uses `["Asset", "Properties", "Systems"]` with row ("Brake pressure", "Integrity", "Brake, ADAS") and asserts `system_col == Some(2)`; everything else as written.
- The refused child test compares the reason with `ModelError::ChildComponentSystem { component: port }.to_string()` and a second preview of (Port, Brake) gives no op and no unmapped row.
- The exported workbook test also asserts `systems_to_add == 2` and that the default mapping picks "Risk register".
- Source lines changed outside tests: 56 (cap 200).

NEXT: reviewer
