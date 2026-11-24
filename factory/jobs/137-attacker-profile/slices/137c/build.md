Commit: 8fc274b Add attacker profile page, nav dot, baseline diff, and report outputs (137c)
Files:
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-client/src/pages.rs
- crates/aegis-client/src/report/doc.rs
- crates/aegis-client/src/report/xlsx.rs
- crates/aegis-client/src/session.rs
- crates/aegis-client/src/state.rs
- crates/aegis-client/src/view.rs
- crates/aegis-client/tests/attacker_profile.rs
- crates/aegis-client/tests/configuration.rs
- crates/aegis-client/tests/report_configuration.rs
- crates/aegis-shell/src/controller.rs
- crates/aegis-shell/tests/nav_groups.rs
- crates/aegis-shell/translations/ja/LC_MESSAGES/aegis-shell.po
- crates/aegis-shell/translations/pseudo/LC_MESSAGES/aegis-shell.po
- crates/aegis-shell/ui/app.slint
Done when:
1. Page::AttackerProfile in group 4, Page::ALL.len() == 18, baseline diff, and report cover / Method / HTML / PDF / XLSX in en and ja — `cargo test --locked -p aegis-client --test attacker_profile --test configuration --test report_configuration --test report_builder --test combined`: passed (5 + 6 + 4 + 15 + 27 = 57 tests, 0 failed).
2. Sidebar shows Attacker profile in group 4 with accent dot only while non-default, translations match across en/ja/pseudo, and all 18 pages open — `cargo test --locked -p aegis-shell --test nav_groups --test standalone --test ui --test i18n`: passed (2 + 5 + 15 + 1 = 23 tests, 0 failed).
3. Full workspace CI passes with zero warnings — `scripts/ci.sh`: passed (`ok workspace fmt`, `ok workspace clippy`, `ok workspace test`, all steps passed in 9 crates).
Notes:
- Added `Page::AttackerProfile` at `Page::ALL[9]` (right after `Page::Trees`, shifting `Risks..Admin` to `10..=17`) and mirrored the indices in `crates/aegis-shell/ui/app.slint` `global Pages` and `nav-entries`.
- Shared `pub(crate) fn attacker_profile_table` in `crates/aegis-client/src/report/doc.rs` between the Method report section and the XLSX `Attacker profile` sheet so headers/rows cannot drift; all default-filter outputs and `report_configuration.rs` block offsets are unchanged.
- Interactive `.slint` page body for `Page::AttackerProfile` remains deferred per `docs/design/workbench-breakdown.md` Section 3.
NEXT: reviewer
