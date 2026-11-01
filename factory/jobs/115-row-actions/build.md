# Build — 115-row-actions

Built directly on master (matching jobs 114 and 116), test-first in four slices.

## Slice 1 (DataTable)

### Red first
The slice's red was the build itself: `app.slint` now names `DataTable` (all 24 `Table`
call sites were swapped to it), and that component did not exist in `widgets.slint`, so
`cargo test -p aegis-shell` could not compile. The existing `undo_scrolls_the_restored_row_into_view_in_a_long_list`
test (typing_undo.rs) is the behaviour guard for the new table: it proves the selected
row is still drawn in the native accent and still scrolled into view after the swap.

### Built
- **`DataTable`** in `widgets.slint`, built on Slint's `ListView` (the same internals
  `StandardTableView` uses) instead of `StandardTableView`, because the standard table
  cannot host per-row buttons, a fixed 28/32 px row height or a tooltip on an elided
  cell. It keeps the exact `Table` API: `columns`, `rows`, `selected`, `label`,
  `picked(int)`. Added inputs (opt-in, all off by default): `editable`, `compact`,
  `show-add`/`add-verb`, `show-open`, `show-duplicate`, `show-remove`, and callbacks
  `activated`, `add-child`, `open-row`, `duplicate-row`, `remove-row` (used from S2/S3).
- Rows are 32 px (28 px with `compact`); the header is a fixed bar above the scroll; the
  name cell (column 1) elides and shows the full name in a `Tooltip`. Selected rows use
  `Palette.accent-background`/`accent-foreground` (the OS accent), hover uses
  `Palette.selection-background`, even rows use `Palette.alternate-background`; this keeps
  the long-list undo test's accent-blue pixel check green on macOS.
- The scroll-into-view fix from the old `Table` is kept verbatim in spirit: a `Timer`
  (60 ms) re-applies `set-current-row` once the layout has a size, so a row selected from
  outside (after Undo) is brought into view. `set-current-row` only fires `picked` when
  the row differs from `selected`, so the controller's round-trip does not re-trigger it.
- `app.slint` imports `DataTable`; every `Table {` became `DataTable {`. Non-element
  tables (risks, baselines, history, figures, excel, admin, library, catalogs, palette,
  shortcuts) get the same widget with no actions (all action inputs default off).
- Strings: ja and pseudo catalogs got the new keys (`More actions for {}`, `Open {}`,
  `Duplicate`, `Duplicate {}`, `Remove {}`). No new dependency.

### Verified
`cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`;
`cargo test --workspace --locked` (638 passed, 0 failed); `scripts/deny.sh` (all crates
ok). The shell suite specifically stays green including
`undo_scrolls_the_restored_row_into_view_in_a_long_list`, `dialogs.rs`, `drafts.rs`,
`typing_undo.rs`, `toast.rs`, `ui.rs`, `pages.rs`, `small_windows.rs`.

### Defect found in S4 (slice 1 was not looked at)
Slice 1 was verified by tests only; nobody had opened a screenshot of the new table. The
first picture taken in S4 showed the header bar drawn at the bottom of the table and the
rows starting at the very top: `header` and `scroll` were loose children of a `Rectangle`,
not in a layout. S4 wraps them in a `VerticalLayout`. The slice 1 test run could not catch
it because no test aimed a click at a row.

## Slice 2 (row actions)

Commit e44319a (with S3's client-core part).

- **+ and ⋯** on a row that is hovered or selected (not for a read-only user). `+` has the
  accessible name `add-verb + " " + name` ("Add threat scenario to ECU"); `⋯` opens an inline
  menu inside the table (no modal): Open, Duplicate, Remove, each with an accessible name.
- **+ starts a draft with the row as the parent** (job 114's draft flow): the controller selects
  the row, then sends `BeginDraft`. Component to asset, asset to threat, damage to threat,
  threat to attack tree, control to goal. `UiState` got `draft_damage`, `draft_threat`,
  `draft_control` (`draft_component` reused); the draft commits as one edit
  (`AddAssetBound`, `AddThreatLeading`, tree linked to the threat, `AddGoalFor`), one undo step.
- **Duplicate**: ten `Duplicate*` edits in the client core (component, interface, data flow,
  trust boundary, asset, damage, threat, control, goal, claim), fresh ids, a " copy" name suffix.
- **Remove** from the menu is immediate with the undo toast (job 116).
- Tests: `aegis-client/tests/session.rs` (+233 lines: draft context, each duplicate, one undo
  step each) and `aegis-shell/tests/row_actions.rs` (the + draft, duplicate, remove with toast).

## Slice 3 (keyboard)

- The `DataTable` `FocusScope` handles Up/Down/Home/End/PageUp/PageDown (move), Return
  (`activated`), Control+Return (add child), Backspace/Delete (remove), Control+D (duplicate).
- **Gap closed this round: real key events.** The earlier tests called the controller's pick
  callbacks and never pressed a key, because a headless click did not reach the table. The cause
  was not focus: `ListView` is a flickable and holds a press back for 100 ms to see whether it
  becomes a drag, so the harness's instant press-and-release was never delivered to the row's
  `TouchArea`. The test `click_row` now holds the press for 150 ms like a hand. (The focus scope was
  already full size and the row `TouchArea` already focuses it on pointer-down; no change was
  needed there.)
- New window property `table-x/y/w`, filled by a `laid-out` callback on `DataTable`, so a test can
  aim a click at a row. Test `real_keys_drive_the_table`: click row 0, Down, Up, End move the
  selection; Control+D duplicates; Delete removes with the undo toast; Control+Return starts a threat
  draft. All with `press_key`/`press_chord`, no callback invoked.
- **Key guards added**: the key handler now does nothing without a selected row, and for
  Delete, Control+D and Control+Return does nothing when the table is not `editable`.

## Slice 4 (read-only and 5,000 rows)

- **Read-only** (`a_read_only_user_gets_no_row_actions`): with a positive control. In editing mode
  the right end of the selected row holds the + and ⋯ buttons (more than two distinct colours);
  after `end-edit` the same strip is one colour. Delete and Control+D do nothing, and the controller
  callbacks (`remove_row_asset`, `duplicate_asset`) called directly change nothing (the client core
  refuses an edit outside Editing mode).
- **5,000 rows** (`five_thousand_rows_scroll_and_select`, run with `--nocapture`): 5,000 assets
  added through the real session (3.6 s of setup), then 200 wheel events with a frame drawn after
  each, then 50 Down presses with a frame each, then End. First attempt scrolled the wrong way
  (the list sat clamped at the bottom); the test now asserts the picture changed and a screenshot
  (`target/shell-screenshots/datatable-5000-scrolled.png`) shows rows around 3,150.

  Measured on the author's Mac, software renderer, headless window (1360 x 860), 5,000 rows:

  | Action | Median | Worst |
  |---|---|---|
  | Scroll: wheel event + one frame | 9.5 ms | 10.5 ms |
  | Select next row (key, controller, frame) | 9.2 ms | 17.6 ms |
  | End: jump to row 5,000 and draw | 10.3 ms | n/a |

  This is a CPU renderer in a headless window, not a GPU frame rate. 9.5 ms is inside a 16.7 ms
  frame at 60 fps, but a person must confirm the feel on a real window with the GPU renderer
  (follow-up, not claimed here). The assertion in the test is only a loose 250 ms ceiling.
- **ID column** widened from 56 to 76 px on all twelve tables: four-digit IDs ("A3161") were
  elided to "A31...".
- The unused `Table` component and its import were deleted.

## Gate
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --workspace --locked` (1,630 tests, 0 failed) and `scripts/deny.sh` pass on the tree
as of the new tests and the VerticalLayout fix. After that run, two small edits (the callback
assertions in the read-only test, deleting the unused `Table`) were checked with fmt, clippy on
`aegis-shell` and the whole `aegis-shell` test suite; the full workspace run was not repeated.

## Not done
- **Return does not move focus into the inspector** (UI-09.4): it re-picks the row. Each of the
  eleven inspectors is a conditional panel that appears after the pick, so focus needs a
  "focus when it exists" flag per page.
- **The draft row still shows row actions** (a draft row is a table row; + and ⋯ on it do nothing useful).
- **Duplicate's " copy" suffix is English** and lives in the client core, which has no i18n.
- The ⋯ menu is operated by pointer; its keyboard path is the Control+D / Delete keys, not the menu.
- UI-09.2 "on keyboard focus": the actions show on the selected row, which keyboard movement
  follows; there is no separate per-button focus ring or Tab order inside a row.
