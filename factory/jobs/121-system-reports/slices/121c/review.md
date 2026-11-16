VERDICT: PASS
## For the owner
What changed: The Excel import can now read a "System" column (also "Systems" and "システム").
Each name becomes a system, created once if it is missing, and the row's component is put in it.
It all lands in the same preview and the same single batch, so one undo removes everything.
Look at:
- crates/aegis-client/src/excel.rs:1163-1174 — a component that already sits in another system is moved to the system named in the sheet, with no warning in the preview beyond the link/update count. This is what the prompt asked for; decide whether a customer should see "moved" called out.
- crates/aegis-client/src/excel.rs:1148 — the system is only read when the row names a component, so the exported "Systems" column on the risk register, damage and threat sheets (several names in one cell) is ignored, never split, never turned into a system.
- crates/aegis-client/tests/system_reports.rs:1011-1016 — the round trip uses an explicit mapping for the components sheet; with the default mapping the import still picks the risk register and brings in no systems. An exported workbook therefore does not restore its systems unless the user maps that sheet.
Risk: low. The workbook is untrusted: every new op (add system, set system) goes through the existing simulation before it is kept, and a refused row is rolled back and listed as unmapped with the model's reason (checked by reading lines 1453-1482 and by the child-component test). No new indexing, no new panic path, no new dependency, no cap changed. The name map grows only after a row succeeds, and only by names the model accepted, so it is bounded by the 64-system limit. Not covered by a test, checked by reading only: a refused row that would have created a new system leaves the name map unchanged, and a 65th system is refused by the model.
View: git -C factory/worktrees/121-system-reports diff 5b11bfb7105efa082c1e9ad0fcb0ed99bc48e39f..HEAD
## Findings
- none. All six tests are present and each would fail without the change:
  - detection: three header sets, `system_col == Some(1)`.
  - create and place: counts, the two system names in order, three set ops, the dry run leaves the project empty, membership read back from the project per component name, one undo empties it again. The "brake"/"Brake" pair proves case-insensitive reuse inside one import.
  - reuse: exactly one set op, compared by Valve's ID, and it targets the existing Brake ID.
  - no component: no system op, one asset, column detected at index 2.
  - child component: reason equals the model's `ChildComponentSystem` text for Port's ID; the (Port, Brake) row gives no op, which proves the ancestor's system is used and not the child's own.
  - exported workbook: membership compared between the two projects for three top-level components, one of them in no system; default mapping adds no system.
- Accepted deviation: `a_system_without_a_component_is_ignored` uses headers `["Asset", "Properties", "Systems"]` instead of `["Asset", "Systems"]`, because the model refuses an asset with no security property. The test still proves the item (it also asserts the column was detected), so this is not a defect. The prompt was slightly wrong here.
- Scope: nothing outside the two allowed files; 56 source lines; nothing to delete.
## Lesson
[planner] When a prompt gives literal rows for an import test, check them against the model's rules first (an asset needs at least one security property), or the builder has to deviate.
NEXT: human
