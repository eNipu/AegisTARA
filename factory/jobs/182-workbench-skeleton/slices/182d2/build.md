# Build 182d2

Commit: cdb8348 Show the explorer tree with icons and marks and open list and element tabs by key (182d2)

Files:
- crates/aegis-client/locales/{en,ja,pseudo}/main.ftl (taken whole from factory/182d-full)
- crates/aegis-workbench/src/app/icons.rs (taken whole)
- crates/aegis-workbench/src/app/mod.rs (`mod icons;`, `pub use icons::icon;`)
- crates/aegis-workbench/src/app/panels.rs (branch version minus DragLabel, drag/drop in `row`, Escape key handler, `text_panel!::set`; unused imports `Folder`, `Selection`, `AppContext` removed)
- crates/aegis-workbench/src/app/workbench.rs (branch version minus keys/local/drop_edit/command imports, `Body`, `actor`/`picked`/`note`, `now_ms`, `picked`, `changed`, `drop_on`, `let_go`, `rows_then`, `on_*`, `status_text`, key context and actions; status bar shows `self.status.clone()`; unused `InteractiveElement` import removed)
- crates/aegis-workbench/tests/explorer_view.rs (new test `opening_rows_adds_tabs_by_key`, self-contained imports)

Done when:
1. `cargo test --locked -p aegis-workbench --test explorer_view` -> 2 passed (opening_rows_adds_tabs_by_key, a_closed_dock_stays_closed_on_the_next_start)
2. `cargo test --locked -p aegis-workbench` -> all pass, 0 failed
3. `git diff --quiet factory/182d-full HEAD -- icons.rs layout.rs locales settings.rs local.rs` -> exit 0
4. `git diff --numstat HEAD~1 HEAD -- . ':!crates/*/tests'` -> 393 added + 30 deleted = 423 counted lines (< 580)
5. owner check: `cargo run -p aegis-workbench` built and the window started and stayed up for 6 s with no output on stderr (then closed by me). I cannot look at the screen or double click from this session, so the visual part (tree with icons, counts, system squares; double click a folder opens a list tab) is left for the owner. The test covers opening a folder and an element by key.

Before you commit: `cargo fmt --all` clean; `scripts/ci.sh crates/aegis-workbench crates/aegis-client` -> all steps passed in 2 crates; `cargo test --locked -p aegis-workbench` -> pass.

Notes:
- Remaining diff to factory/182d-full is exactly the 182d3 parts: keys.rs, mod.rs keys lines, panels.rs drag/drop/Escape/set, workbench.rs edit/undo/actions, and the branch test file.
- One wording change beyond the cut: the `ExplorerPanel` doc comment said "Clicks, drags and drops go to the Workbench"; it now says "Clicks and double clicks", since drags are not in this part.
- In `open`, the `ElementTab`/`ListTab` entity is passed straight into `panel_handle(e)` (no `.clone()`, since the `Body` that held the second copy is gone).
- `Workbench::row` stays public as listed in the keep set; in this part only tests or 182d3 use it.

NEXT: reviewer
