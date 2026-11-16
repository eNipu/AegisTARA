# Build 121a

Commit: 9400add Filter the report by the systems a profile names (121a)

Files:
- crates/aegis-client/src/report/profile.rs (`ReportFilters.systems`, `MAX_AUDIT_DETAIL_BYTES`, `audit_token`, ` systems=` in `audit_summary`, final cut to 256 bytes)
- crates/aegis-client/src/systems.rs (`view_of_names`, `view_names`, `Scope`)
- crates/aegis-client/src/report/doc.rs (scope in `build`: cover row, overview_in, scoped counts, threat filter via `threat_shows`, row filters on every table, `comparison_in`)
- crates/aegis-client/src/session.rs (`export` keeps only live system names before `audit_summary`)
- crates/aegis-client/locales/{en,ja,pseudo}/main.ftl (`report-systems`, `report-all-systems`)
- crates/aegis-client/tests/system_reports.rs (new, 6 tests)

Done when:
1. `cargo test --locked -p aegis-client --test system_reports`: 6 passed
2. `cargo test --locked -p aegis-client --test report_builder --test reduced_by_report`: 14 + 14 passed
3. `cargo check --locked -p aegis-shell`: Finished, no errors
Before you commit: `cargo fmt --all` ok; `scripts/ci.sh crates/aegis-client` fmt/clippy/test all ok.
Changed lines outside tests: 232 insertions, 34 deletions (cap 360).

Notes:
- Threat filtering is done in one place, as the prompt asks: `threat_shows(project, scope, id)` is added to `matching_threats` and to the two `row_matches` call sites. `filter_on` is `has_filter(..) || scope.is_on()`; `has_filter` itself is unchanged.
- When the scope is off, `Scope::new` returns early without calling `system_use()`, and `scope.shows` returns `true` before running the closure, so no row is dropped. The only extra work is the `retain` passes, which are O(n) no-ops.
- The new `systems` filter does not appear in `has_filter`, so the per-component asset filter and other filters behave as before.
- `audit_summary` now cuts the whole string to 256 bytes. A profile with no systems gives the same string as before unless its name was already over the cap, in which case the server would have refused it anyway.
- Test `a_report_for_one_system_holds_only_that_system`: a name is "present" or "absent" by checking whether a heading or cell ends with `" · <name>"` (the element's own entry), not by plain substring. The required Brake to ADAS data flow shows in the Brake report, and its "to" cell names the ADAS component without a visual ID. A plain substring check would therefore find "ADAS camera" in a correct Brake report. The test also checks that every ADAS-only name is present in the unfiltered report, so the absence check can fail.
- `MAX_AUDIT_DETAIL_BYTES` is `pub` in `report::profile` but not re-exported from `report/mod.rs`, because that file is outside the allowed list.

## Rework 1
Commit: 1bcd981 Test every system filter of the report and its summary counts (121a)
Files: crates/aegis-client/tests/system_reports.rs (test only; no src change)
Fixes:
- Fixture: ADAS-only trust boundary "ADAS only zone" (member: ADAS camera), attack tree "ADAS only blinding", goal "ADAS only clear view" and claim "ADAS only lens is sealed", all linked to the ADAS threat. Added to `ADAS_ONLY`, so the presence-in-full and absence-in-Brake checks cover them. All are written as "<visual id> · <name>", so `has_entry` applies.
- Security settings: the setting cell names the control, which the control table also shows. The test therefore checks column 2 of the table under the "Security settings" heading directly. The Brake report holds the shared filter and not "ADAS only lens check"; the full report holds both.
- Executive summary: `summary_counts` reads the "The project has ..." paragraph after stripping Fluent isolation marks. Brake gives (2 threats, 2 assets) and the full report gives (3, 3).
- Stale name: on `fixture()`, `["Gone"]` gives the same HTML as no filter, and `["Brake", "Gone"]` gives the same HTML as `["Brake"]`. The test also asserts that the Brake HTML differs from the full HTML, so the equality checks cannot pass trivially.
Mutation check (each filter replaced by `true ||` one at a time, then restored): trust boundary, settings, trees, goals, claims, summary threat count and summary asset count each make `system_reports` fail. The problems filter (doc.rs `p.target.is_none_or`) still makes no test fail. The fixture raises no problem on one system alone, and adding one cheaply would need a problem kind chosen for the purpose. It stays untested, as review.md allows.
Done when:
- `cargo fmt --all`: no change
- `scripts/ci.sh crates/aegis-client`: all steps passed
- `cargo test --locked -p aegis-client --test system_reports --test report_builder --test reduced_by_report`: 6 + 14 + 14 passed
- `cargo check --locked -p aegis-shell`: Finished

NEXT: reviewer
