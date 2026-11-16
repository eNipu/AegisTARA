Commit: eb53cff Warn when two systems share a name and test the report and import guards (121db)
Files:
- crates/aegis-model/src/lint.rs (new `LintCode::SystemNameShared { system, first }`, Warning severity, Display, added to the all-variants unit test)
- crates/aegis-model/src/project.rs (`lint_systems`, called after `lint_components`)
- crates/aegis-model/tests/systems.rs (`shared_names` helper, 2 tests)
- crates/aegis-client/src/pages.rs (`problem` arm + `shared_system_name` helper, target `None`)
- crates/aegis-client/tests/system_reports.rs (4 tests, `column_1` helper; imports `pages`, `EntityKind`, `MAX_SYSTEMS`)

Done when:
1. `cargo test --locked -p aegis-model --test systems shared_name` → ok, 1 passed (see Notes); `... --test systems shared` → ok, 2 passed (both new tests).
2. `cargo test --locked -p aegis-model --lib lint` → ok, 2 passed (size test ≤ 40 bytes still passes).
3. `cargo test --locked -p aegis-client --test system_reports the_problems_table` → ok, 1 passed.
4. `cargo test --locked -p aegis-client --test system_reports the_comparison` → ok, 1 passed.
5. `cargo test --locked -p aegis-client --test system_reports refused` → ok, 3 passed (the two new tests plus the existing child-component test).
6. Trials, each reverted with `git checkout crates/aegis-client/src/report/doc.rs` and not committed:
   - problems filter in `report/doc.rs` replaced by `.filter(|_p| true)` → `the_problems_table_follows_the_systems_filter` FAILED at the "Brake report has no cell containing `ADAS camera`" assertion.
   - `comparison_in(..., &crate::systems::SystemView::all())` instead of `&view` → `the_comparison_follows_the_systems_filter` FAILED at the "Brake comparison has no `ADAS only radar`" assertion.
7. `scripts/ci.sh` → exit 0, "all steps passed in 8 crate(s)" (fmt, clippy, test).
Before you commit: `cargo fmt --all` ok; `cargo test --locked -p aegis-model --test systems` 23 passed; `--lib lint` 2 passed; `cargo test --locked -p aegis-client --test system_reports --test pages` 24 + 22 passed.

Notes:
- The prompt names the first model test `a_shared_system_name_warns_once_each_after_the_first`, but the Done-when 1 filter `shared_name` does not match it ("shared_system_name"), so that filter runs only `distinct_system_names_give_no_shared_name_finding`. I kept the prompt's test name; `cargo test --locked -p aegis-model --test systems shared` runs both, and both pass.
- `Project::lint` sorts findings; `SystemNameShared` is the last variant, so in "lint order" the findings follow `(system, first)`, i.e. later system ID order, as the test expects.
- `shared_system_name` takes the name from `project.element(system)` and falls back to `"?"` like `named` does; the visual IDs fall back to an empty string.
- Changed source lines (without tests): 54 (pages.rs 21, lint.rs 18, project.rs 15).
- `report/doc.rs` and `excel.rs` are unchanged in the commit.

NEXT: reviewer
