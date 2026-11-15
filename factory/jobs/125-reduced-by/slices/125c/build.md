Commit: 425a918 List reduced-by, risks lowered and security settings in the report (125c)
Files: crates/aegis-client/src/pages.rs, src/report/doc.rs, locales/{en,ja,pseudo}/main.ftl, tests/reduced_by_report.rs
Done when:
1. cargo test --locked -p aegis-client --test reduced_by_report: 9 passed
2. cargo test ... --test report_builder --test pages: passed (22 + 14)
3. scripts/ci.sh crates/aegis-client: all steps passed
Notes: none of the eight keys existed before, so all were added at the end of each catalog (not next to through/kind). Non-test, non-locale diff is 108 lines. The risk-table cell for a threat with no tree and no reducers is empty.
NEXT: reviewer
