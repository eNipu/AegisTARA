Commit: 2e3c96c Integrate assumptions into Word and XLSX reports (136f)
Files:
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-client/src/report/doc.rs
- crates/aegis-client/src/report/xlsx.rs
- crates/aegis-client/tests/assumptions.rs
Done when:
1. `cargo test --locked -p aegis-client --test assumptions` -> exit 0 (6 passed; 0 failed)
2. `cargo test --locked -p aegis-client --test report_configuration` -> exit 0 (4 passed; 0 failed)
3. `scripts/ci.sh` -> exit 0 (`ok workspace fmt`, `ok workspace clippy`, `ok workspace test`, `all steps passed in 9 crate(s)`)
Notes:
- Additional verification after final edit: `cargo fmt --all` -> exit 0; `cargo test --locked -p aegis-client --lib report::doc::tests` -> exit 0 (1 passed; 0 failed, testing `doc::build` across `Language::En`, `Language::Ja`, and `Language::Pseudo`).
- Shared `doc::assumption_report_rows` feeds both the Word/PDF `Assumptions` table (`ID`, `Assumption`, `Holds`, `Claims`, `Excluded while it holds`) and the XLSX `Assumptions` sheet (`ID`, `Assumption`, `Holds`, `Claims`, `Conditions`, `Excluded while it holds`).
- `doc::build` keeps condition-only block offsets unchanged when zero assumptions exist (`report_configuration` passes unchanged).
- Non-test diff size (`git diff --numstat`): 228 added lines (265 added+deleted across non-test files, within `max_lines: 280`).
NEXT: reviewer
