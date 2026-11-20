Commit: cd3ee88 Add TableTab UI-14 presentation and scroll layout (189d1b)
Files:
- crates/aegis-workbench/src/app/table_tab.rs
- crates/aegis-workbench/tests/table_view.rs

Done when:
1. UI-14 rendering, link chip navigation, excluded row, and light/dark theme tests pass — `cargo test --locked -p aegis-workbench --test table_view` -> PASS (7 passed, 0 failed)
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench` -> PASS (`ok aegis-workbench fmt`, `ok aegis-workbench clippy`, `ok aegis-workbench test`)

Notes:
- Kept `TableTab` strictly driven by `ColumnSpec` and `TableRow` with zero per-table branching.
- Added `pub(super) fn render_cell(...)` looking up each column in `self.table.columns()`, rendering shaded computed cells (`tokens.raised` / `tokens.ink_muted`), clickable `LinkChip` pills calling `cx.stop_propagation()` + `open_target_skipping(..., Some(my_id), ...)`, WCAG-AA-contrasted light/dark `Tone` pills (all pairs >= 5.9:1), truncated plain text, `accent_wash` selection highlight, and crisp `accent` border on the active cell.
- Header cells display `IconName::Asterisk` (`xsmall`) with `grid-computed-tip` tooltip on `ColumnKind::Computed` columns, aligned with a 4 px leading slot that renders a hatched/striped left indicator on `row.excluded` rows plus a compact `grid-excluded-badge` chip with `row.excluded_reason` tooltip.
- Wrapped header + body inside `.flex_1().min_h_0().overflow_scroll()` with an inner `.min_w_full().w(table_width).flex_shrink_0()` container and fixed `col.width` `.flex_shrink_0()` cells so wide tables (`Risks`, 1206 px) scroll in both axes without squishing columns.
- Changed source lines (excluding `tests/`): 295 added + 59 deleted = 354 (< 360 cap).

NEXT: reviewer
