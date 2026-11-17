Commit: 0f3b22a2cdacea89e0c360a42018a3771e6c9a82 Export and import parties with responsible links in reports and Excel (145e)

Files:
- crates/aegis-client/src/excel.rs (ExcelMapping.responsible_col + en/ja aliases, is_empty; lowercase party name map; Responsible cell parsing with ', ' and '、' separators, case-insensitive dedup and MAX_RESPONSIBLE cap; Op::AddParty kind Other + Op::LinkGoalParty/Op::LinkClaimParty; new ops added to updates_and_links; unit test)
- crates/aegis-client/src/report/xlsx.rs (Responsible header and g/c.responsible.join(", ") cell on Goals and Claims; Parties sheet from project.parties() with ID/Name/Kind and party_visual_id, counted in counts; unit test with a formula-guard cell)
- crates/aegis-client/src/report/doc.rs (catalog.tr("responsible") header and responsible.join(", ") cell on the goals and claims tables)
- crates/aegis-client/tests/report_builder.rs (Parties in the sheet-name check; new test asserting the report tables carry the Responsible header)

Done when:
1. cargo test --locked -p aegis-client --lib excel — PASS, 7 passed (new responsible_headers_and_import_link_parties_to_goals_and_claims test covers en/ja auto-detect, two names -> one new party each linked to goal and claim, duplicate and over-cap skipped, mapped ops asserted).
2. cargo test --locked -p aegis-client --lib report::xlsx — PASS, 5 passed (new parties_sheet_and_responsible_columns_are_exported_and_guarded checks the Parties sheet, both Responsible columns and a leading-'=' name guarded as "'=Bosch Corp").
3. cargo test --locked -p aegis-client --test report_builder — PASS, 15 passed (new goals_and_claim_report_tables_carry_the_responsible_header test).

Before you commit:
- cargo fmt --all — PASS.
- scripts/ci.sh — PASS ("all steps passed in 8 crate(s)": fmt, clippy, test).
- The three Done-when commands — all PASS (above).

Notes:
- Net source change is 296 added / 3 deleted lines (numstat includes tests), under the slice max_lines of 520; no #[rustfmt::skip] used (L28).
- The Parties sheet is built from project.parties() with no system-view filter, so its count is counted where the sheet is built (L22). Its count is pushed as the last entry of `counts`; the Overview has no Parties row, so the zip leaves that trailing entry unused, matching the prompt which does not add an Overview row.
- Import deduplicates by lowercase party name within a row and stops at MAX_RESPONSIBLE (16). As in the model's link guards, a row whose goal/claim already carried responsible links from existing project state is not pre-counted; the prompt's test fixtures import into a fresh project. New parties use PartyKind::Other because Excel carries no kind.
- All cells still pass through guard/xml_cell; no new guard or locale key was added.

NEXT: reviewer
