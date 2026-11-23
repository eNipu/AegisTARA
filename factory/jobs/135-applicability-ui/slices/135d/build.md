Commit: 643f1b8
Files:
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-client/src/exclusion.rs
- crates/aegis-client/src/report/doc.rs
- crates/aegis-client/src/report/xlsx.rs
- crates/aegis-client/tests/report_configuration.rs
- crates/aegis-shell/ui/app.slint
- crates/aegis-shell/translations/ja/LC_MESSAGES/aegis-shell.po
- crates/aegis-shell/translations/pseudo/LC_MESSAGES/aegis-shell.po

Done when:
1. `cargo test --locked -p aegis-client --test report_configuration` — PASS (4 passed, 0 failed)
2. `scripts/ci.sh` — PASS (`workspace fmt`, `workspace clippy`, `workspace test` all passed in 9 crates)

Notes:
- All 135d requirements (locale keys in en/ja/pseudo, `exclusion.rs` helpers `threat_risk_not_applicable` and `excluded_entries`, report cover rows and `Configuration and assumptions` section in `doc.rs`, XLSX `Conditions` sheet and `Excluded` columns + `"not applicable"` risk cells in `xlsx.rs`, and all 4 test cases in `tests/report_configuration.rs`) are implemented.
- Also synced `crates/aegis-shell/ui/app.slint` (`global Pages` and `nav-entries`) and `crates/aegis-shell/translations/{ja,pseudo}/LC_MESSAGES/aegis-shell.po` for `Page::Configuration` added in `135a`, restoring full `scripts/ci.sh` pass.

NEXT: reviewer
