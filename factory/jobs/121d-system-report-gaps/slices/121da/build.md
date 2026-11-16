Commit: 106a65f Keep figures to the systems filter and stop a report export on a stale system name (121da)

Files:
- crates/aegis-client/src/systems.rs (private `element_selection`, `figure_shows`, `missing_system_names`)
- crates/aegis-client/src/report/doc.rs (figures section built from the kept figures)
- crates/aegis-client/src/report/xlsx.rs (Figures sheet keeps only shown figures; pushed only when one is kept)
- crates/aegis-client/src/report/profile.rs (doc comment of `ReportFilters.systems` only)
- crates/aegis-client/src/session.rs (`export` stops HTML/PDF/workbook on a stale name with a warning)
- crates/aegis-client/tests/system_reports.rs (self-comparison removed; `figures_follow_the_systems_filter`, `missing_system_names_lists_each_stale_name_once`)
- crates/aegis-client/tests/session.rs (`a_stale_system_name_stops_the_report_export`)

Done when:
1. `cargo test --locked -p aegis-client --test system_reports figures_follow` — ok, 1 passed.
2. `cargo test --locked -p aegis-client --test system_reports missing_system_names` — ok, 1 passed.
3. `cargo test --locked -p aegis-client --test session a_stale_system_name` — ok, 1 passed.
4. `grep -c "report_workbook_xlsx(&ws)" crates/aegis-client/tests/system_reports.rs` — prints `1`.
5. `cargo test --locked -p aegis-client --test system_reports --test report_builder --test reduced_by_report` — ok (14, 14, 20 passed); `a_stale_system_name_is_ignored` unchanged and passing.
6. Mutations, each tried once by hand and then restored (checked with `git diff --stat`):
   - doc.rs `.filter(... figure_shows ...)` line removed → `figures_follow_the_systems_filter` FAILED at the report assert (`captions(&["Brake"])`, line 735).
   - xlsx.rs `.filter(... figure_shows ...)` line removed → same test FAILED at the workbook assert (line 742).
   - session.rs whole stale-name block removed → `a_stale_system_name_stops_the_report_export` FAILED (timed out waiting for the "stale warning").

Before you commit:
- `cargo fmt --all` — clean.
- `scripts/ci.sh crates/aegis-client` — fmt, clippy, test all ok.
- the system_reports/report_builder/reduced_by_report run — ok.
- `cargo test --locked -p aegis-client --test session a_stale_system_name` — ok.
- `cargo check --locked -p aegis-shell` — ok.

Notes:
- Source lines changed (without tests): 88 added, 5 removed (`git diff --numstat` on `crates/aegis-client/src`).
- `missing_system_names` uses a `BTreeSet<&str>` to drop repeats, so the doc comment says O(k * s) plus O(k log k) for the repeats, instead of only O(k * s).
- In the doc figures test, `table_under` returns the body rows only (`Block::Table` keeps headers apart), so the captions are read from every row without skipping one.
- The figures test gives each figure a `figure_id` of `fig-<i>`; the prompt did not name the IDs.
- The session test sends the three edits in one loop and then `ShowPage(Page::Item)`, the same way lines 1734-1741 do. It waits for `project_ready` and one component row. `write_private_file` creates the temp directory itself.
- A first try at the session mutation (removing only `return;`) did not compile, because `open` is still borrowed after `self.notice`. So the mutation that counts removes the whole block. I deleted the temp directory that the failing mutation run left behind.

NEXT: reviewer
