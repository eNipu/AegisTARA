VERDICT: PASS
## For the owner
What changed:
Replaced the read-only `InspectorPanel` stub with a relation-backed GPUI `InspectorPanel` driven by `aegis_client::relations::RELATIONS`, showing the selected element's header (with applicability exclusion badge), clickable relation chips, and an inline `MultiLinkPickerView`.
Wired `InspectorPanel` into `Workbench` (`select`, `open_target_skipping`, `changed_skipping`, and `files.rs::replace`) with `EntityId` skip guards so live edits, undo, chip navigation, Escape-close, element deletion, and project replacement stay double-lease-safe and never leave stale state.
Extended `tests/picker_view.rs` with headless GPUI tests covering `RELATIONS` rows for Threat/Goal, live keyboard toggle without double-lease or reopening, undo restoration, Escape close, chip navigation, deleted-element and same-UUID project-replace clearing, and read-only refusal.
Look at:
- `crates/aegis-workbench/src/app/inspector.rs:199-245` — `set_selection` (stale-selection clearing and `skip`-guarded picker refresh) and `open_picker` (read-only guard before opening picker, `set_on_close` callback).
- `crates/aegis-workbench/src/app/inspector.rs:272-285` — `open_chip` navigates via `open_target_skipping(..., Some(my_id), ...)` and rebuilds local state to avoid GPUI double-leasing `InspectorPanel`.
- `crates/aegis-workbench/src/app/workbench.rs:533-591` — `changed_skipping` live inspector/picker sync and `open_target_skipping` bypassing `select` so `skip` is honored.
Risk: GPUI entity double-lease panics during live picker toggles, Escape-close, or chip clicks, or stale inspector state after element deletion or project switch; checked by tracing every `Entity::update` / `skip` path across `Workbench`, `InspectorPanel`, and `MultiLinkPickerView` and verified by headless GPUI tests including Escape keystrokes and same-UUID project replacement.
View: git -C factory/worktrees/189-table-tabs diff 3173ca9fc7173a903e58cb146afc03d8e2946512..HEAD
## Findings
none
## Lesson
none
NEXT: human
