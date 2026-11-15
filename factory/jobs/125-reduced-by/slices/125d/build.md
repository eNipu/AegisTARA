Commit: acf7d1f List reduced-by, risks lowered and security settings in the workbook (125d)
Files: crates/aegis-client/src/report/xlsx.rs (40 added lines), crates/aegis-client/tests/reduced_by_report.rs
Done when:
- cargo test --locked -p aegis-client --test reduced_by_report: 14 passed
- cargo test ... --test reduced_by_report --test report_builder --test pages: all ok
- cargo check --locked -p aegis-shell: ok
- scripts/ci.sh: fmt, clippy, test all passed
Notes:
- L09 check: with `cell_text` temporarily changed to skip `guard`, `names_that_look_like_formulas_stay_text` failed (at the earlier read-back assertion on the "'@SUM(A1)" cell, before reaching the raw-bytes checks). Code restored; diff shows no change to guard/cell_text.
- The model accepted all three names; no refusal.
- Test reads Reduced by from the workbook read back, where the guarded cell shows as "'@SUM(A1)".
NEXT: reviewer
