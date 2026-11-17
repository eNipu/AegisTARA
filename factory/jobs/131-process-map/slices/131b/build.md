# Build Report — 131b

Commit: 353d511 Publish the process map as page-group boxes (131b)

Files:
- crates/aegis-client/src/view.rs: `Page::key()` for all 15 pages, `Page::first_in_group()`, `PageGroup::label()`.
- crates/aegis-client/src/pages.rs: `ProcessMapBox`, `process_map(ws)`, private `page_count`.
- crates/aegis-client/tests/process_map.rs: box per group, counts/mark/accessible name, clause_refs union, page keys.

Done when:
1. `cargo test --locked -p aegis-client --test process_map` — PASS (3 tests).

Before you commit (from worktree root, CARGO_TARGET_DIR shared with the main checkout):
- `cargo fmt --all` — PASS
- `scripts/ci.sh` — PASS (workspace fmt, clippy, test; 8 crates)
- `cargo test --locked -p aegis-client --test process_map` — PASS

Notes:
- `process_map` returns one box per `PageGroup::ALL` entry in sidebar order. Counts follow
  `nav_counts` (Item = components + interfaces + data flows + trust boundaries; Assets;
  Damage; Threats and Risks = threats; Trees; Controls; Goals + claims; Problems = lint
  count) with one exception: the Versions page counts 0, because the baseline list lives
  in `session::Open`, not in `Workspace`, and `process_map` only receives `&Workspace`.
  The code comments this.
- Findings/errors come from `problems(ws)`; only rows with a `target` can belong to a
  group (a group-less row such as a shared system name is not counted). `mark` is `"✓"`
  when findings is 0, else `"⚠ n"`, and `accessible_name` is `"{label} {count} {mark}"`.
- `clause_refs` unions `methodology().clause_refs(page.key())` over the group's pages in
  `Page::ALL` order, dropping duplicates while keeping first-seen order; builtin packs
  have none, so the builtin case yields an empty vec (tested).
- Test uses a custom def (`read_pack` of the ISO pack with three page keys) to pin the
  union and dedup, and a one-asset project to pin the `⚠ 1` warning mark; tests are not
  in the line budget.
- Gate size: `git diff --numstat` shows 151 added source lines (max_lines 320), plus the
  new test file.
- No dependency, Slint or `aegis-shell` change; no file outside the allow list touched.

NEXT: reviewer

## Rework 1

Commits: 353d511 Publish the process map as page-group boxes (131b)
86f7c6c Count baselines in the Project box and pin the error mark (131b)

Files:
- crates/aegis-client/src/pages.rs: `process_map(ws, baselines)` and
  `page_count(p, page, baselines, lint)` with an explicit arm for every page
  (`Versions => baselines`, no wildcard).
- crates/aegis-client/tests/process_map.rs: `box_of` takes `baselines`; new
  `the_project_box_counts_baselines_like_the_sidebar` and
  `an_unrated_threat_is_one_error_in_assets_and_threats`.
- crates/aegis-model/src/methodology/mod.rs: the `clause_refs` doc names
  `"assets"`/`"threats"` and points at `Page::key()` in plain text.

Done when:
1. `cargo test --locked -p aegis-client --test process_map` — PASS (5 tests).
2. `cargo test --locked -p aegis-client --test process_map an_unrated_threat` —
   PASS; with `errors += usize::from(row.error)` temporarily replaced by
   `errors += 0` the same test FAILS (`after.errors` 0 vs 1); the file was
   restored before the commit.
3. `rg -n "threat_scenario|Page::key" crates/aegis-model/src/methodology/mod.rs`
   — `rg` is not installed on this machine, so the same search ran through
   grep: `Page::key()` is at mod.rs:162 and the doc example reads `"assets"` /
   `"threats"`; the only `threat_scenario` hits left are the
   `ElementNameDef::threat_scenario` field and a names-table entry, not the
   doc example.

Before you commit (from worktree root, CARGO_TARGET_DIR shared with the main checkout):
- `cargo fmt --all` — PASS
- `scripts/ci.sh` — PASS (workspace fmt, clippy, test; 8 crates)
- `cargo test --locked -p aegis-client --test process_map` — PASS (5)
- `cargo test --locked -p aegis-model` — PASS (709, doctests)

Notes:
- Review finding 1: `process_map` takes `baselines` and documents it as
  `open.baselines.len()`; `page_count` counts `Page::Versions => baselines`, so
  the Project box equals the sidebar's Project group. Every page has its own
  arm, so the `_ => 0` wildcard is gone.
- Review finding 2: the fixture that yields exactly one error is an asset with
  property 3 (authenticity) and a `Spoofing` threat (category 0) with no attack
  tree and no direct feasibility rating, which is
  `LintCode::ThreatWithoutFeasibility`. Observed in the Assets-and-threats box:
  errors 1, warnings 2 (`ThreatWithoutDamages`, `ThreatWithoutTreatment`),
  findings 3, so the mark is `⚠ 3`; the warnings are counted from `problems()`
  in the test, not written as a number.
- Review finding 3: the doc is fixed as described above.
- Deviation: the prompt's `properties: vec![0]` in the error test was rejected
  by the model on apply (`PropertyNotOnAsset { property: 3 }`) because category
  0 is Spoofing, which violates authenticity, property 3 of the ISO pack. The
  test declares `vec![3]` and comments why; the rest of the fixture follows the
  prompt.
- Size: `git diff --numstat 353d511` shows 22 added source lines (18 in
  pages.rs, 4 doc lines in the methodology module) plus the tests; cap 320.
- No dependency, Slint or `aegis-shell` change; no file outside the allow list.

NEXT: reviewer

## Integration

Merge commit: 8172f6836e697d6690c841a67fd1b97823089dc2
"Merge master into factory/131-process-map: keep 129c table rework and process map"
Parents: 86f7c6c (branch HEAD) and ae151cf (master). One merge commit; master
itself untouched, nothing pushed.

What conflicted and how it was resolved:
- `crates/aegis-client/src/view.rs` — the only content conflict. Both sides
  inserted code at the same spot in `impl Page` (after `from_index`, before
  `layout`): this branch added `first_in_group()` and `key()`, while 129c
  added `tables()` and rewrote `layout()` to delegate to it. Resolution keeps
  BOTH: the branch's `first_in_group()` and `key()` verbatim, followed by
  master's `tables()` verbatim, then master's new `layout()`. The branch's
  `PageGroup::label()` auto-merged cleanly next to master's unchanged
  `PageGroup` impl. 129c's keyed `ColumnSpec`, `ColumnLabel`, `ElementType`,
  `TableId`, the `col`/`hidden` helpers and the new `table.rs` are all kept
  as master wrote them; the old `*_COLUMNS` statics stay deleted (nothing on
  this branch referenced them — verified by grep).
- `crates/aegis-model/tests/pack_limits.rs` — auto-merged, no conflict: the
  branch's `clause_refs_caps` test (line ~272) and master's comment fix in
  `control_kind_conflicts_cap` (line ~672) are in different regions; both
  present in the merge.
- No renames affected this branch's code: `process_map`/`ProcessMapBox` in
  `pages.rs` use `Page::key()`, `PageGroup::label()`, `Page::first_in_group()`
  and `methodology.clause_refs()`, none of which 129c renamed or moved, so no
  adaptation was needed beyond the conflict resolution above. All other
  master changes (locales, `table.rs`, `lib.rs`, rewritten
  `table_conventions.rs`/`setting_conflicts.rs`/`control_kinds.rs`,
  `tests/pages.rs`, docs, factory records) merged in untouched.

Command results (from the worktree root, with
`CARGO_TARGET_DIR="$(git rev-parse --git-common-dir)/../target"`):
- `cargo fmt --all` — PASS, no changes needed.
- `scripts/ci.sh` (whole workspace) — PASS: `ok workspace fmt`,
  `ok workspace clippy`, `ok workspace test`; "all steps passed in 8 crate(s)".
- `cargo test --locked -p aegis-client --test process_map --test table_conventions`
  — PASS: process_map 5/5 (incl. `clause_refs_are_the_groups_refs_unique_and_in_page_order`,
  `every_page_has_a_unique_key`, `the_project_box_counts_baselines_like_the_sidebar`,
  `an_unrated_threat_is_one_error_in_assets_and_threats`); table_conventions 8/8
  (129c's keyed-column suite, incl. `columns_have_unique_keys_and_start_with_the_id`,
  `every_table_belongs_to_one_page_and_layouts_agree`).
- `cargo test --locked -p aegis-model --test pack_limits --test builtin_pack`
  — PASS: pack_limits 23/23 (incl. the branch's `clause_refs_caps` and master's
  `control_kind_conflicts_cap`); builtin_pack 15/15.

Neither side's behavior or tests were dropped or simplified.

NEXT: reviewer
