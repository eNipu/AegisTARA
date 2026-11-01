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

### Not built yet (later slices)
Row actions (+ and the menu) are not wired to any table yet (S2); the keyboard handler
exists in the widget but has no controller callbacks attached (S3); read-only suppression
and the 5,000-row measurement (S4). The `Table` component is still exported but unused;
it will be removed once S2 wires every element table to `DataTable`.
