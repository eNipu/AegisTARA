VERDICT: PASS
## For the owner
What changed: Added `RelationKey`, `ColumnKind`, `ColumnSpec`, and `PageLayout` to `aegis-client::view`; implemented `Page::layout()` for all 15 pages; published seven per-page column spec slices; added `table_conventions` integration tests.
Look at: `crates/aegis-client/src/view.rs:278` (`Page::layout` exhaustive match over all variants); `crates/aegis-client/src/view.rs:1580` (column constants and their kind/label sequences); `crates/aegis-client/tests/table_conventions.rs:1` (kind and layout assertions).
Risk: none — additive only, no input handling, paths, or new dependencies; `cargo test --locked -p aegis-client --test table_conventions` and `scripts/ci.sh` passed.
View: git -C factory/worktrees/129-table-conventions diff 5478abfdf2919e166767ba5127388489fea9b1ae..HEAD
## Findings
none
## Lesson
none
NEXT: human
