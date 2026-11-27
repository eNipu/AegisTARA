VERDICT: PASS
## For the owner
What changed: Added `TableId::Fragments` (`ALL` length 10, `Page::SharedBlocks` tables `[SharedSteps, Fragments]`) and `Folder::Fragments` under project `SharedAnalysis`, backed by live `TreeRole::Fragment` trees, root evaluation feasibility, direct `FRAGMENT_USED_IN` chips, and `trees/tt*.json` source view. Wired `NewFragment`, `RenameFragment`, `RemoveTree`, fill/paste, `SharedBlocks` counts/find/descriptions, EN/JA/pseudo `folder-fragments` strings, and `Workbench` `TableTab` support, rejecting Threat-role trees on the Fragments table.
Look at:
- `crates/aegis-client/src/table.rs:100-119` — `table_source_json` collects `trees/tt*.json` files from `to_workspace_files()` into a deterministic JSON array without inventing `shared/fragments.json`.
- `crates/aegis-client/src/grid/edit.rs:40-57` — `target_matches_table` validates `TreeRole::Fragment` for `(TableId::Fragments, Selection::Tree(_))` across cell editor, edit, fill, and paste.
- `crates/aegis-client/src/explorer.rs:169-189` — `collect` partitions Threat-role trees (`Folder::Trees`) from Fragment-role trees (`Folder::Fragments`) while keeping fragments project-level only.
Risk: none — all gate checks, full workspace CI, persistence/compatibility regressions, and 3 new integration tests plus workbench `table_view` parity pass after commit `8b09a1b`.
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff 4b73f3764ca87c216443c05618f78fe397eae406..HEAD
## Findings
none
## Lesson
none
NEXT: human
