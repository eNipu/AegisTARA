# Build 146c

Commit: 550f2459b445082e850962af9e7cbd8cffe628bb Carry pair rationale into client views and reports (146c)

Files:
- `crates/aegis-client/src/pages.rs` — added `rationale` to `RiskRow` and `ThreatDetail`, plus private `pair_rationale` helper used by both `risk_rows` and `threat_detail` (deciding entry → damage idx → `t.damage_rationale`).
- `crates/aegis-client/src/view.rs` — added visible `rationale` column to `TableId::Risks` with `msg("pair-rationale")`, width 200, after `feasibility`.
- `crates/aegis-client/src/table.rs` — added `"rationale" => text(&r.rationale)` cell lookup.
- `crates/aegis-client/src/commands.rs` — added `Edit::SetDamageLinkRationale` with `to_ops` → `Op::SetDamageLinkRationale` (`text.trim().into()`) and `unchanged` comparison against `t.damage_rationale(project.damage_scenario_idx(damage)?)`.
- `crates/aegis-client/src/undo_label.rs` — added the new `Op` to the `Selection::Threat` match and the `"change", "pair rationale of threat scenario", threat(t)` label arm.
- `crates/aegis-client/src/report/doc.rs` — added `catalog.tr("pair-rationale")` header and `r.rationale.clone()` row cell after `feasibility`, aligned.
- `crates/aegis-client/src/report/xlsx.rs` — added `"Pair rationale"` header after `"Feasibility rationale"` and `r.rationale` cell after the feasibility-rationale cell, so header and row stay aligned.
- `crates/aegis-client/locales/en/main.ftl`, `ja/main.ftl`, `pseudo/main.ftl` — added hyphenated `pair-rationale` (en `Pair rationale`, ja `組み合わせの根拠`, pseudo `⟦Pâîr râtîônâlé␣⟧`).
- `crates/aegis-client/tests/146-pair-rationale.rs` — new job-named test (4 tests).
- `crates/aegis-client/tests/table_conventions.rs` — added `r.rationale` to the expected `RiskRow` fields so `every_table_shows_what_its_page_shows_today` accounts for the new visible column (CI required).
- `crates/aegis-client/tests/undo_every_op.rs` — exercised the model's `Op::SetDamageLinkRationale` in `walk_elements` so the "every op kind" coverage test passes (CI required).

Done when:
- `cargo fmt --all` — passed.
- `scripts/ci.sh` — passed (`ok workspace fmt`, `ok workspace clippy`, `ok workspace test`; all steps passed in 8 crates).
- `cargo test --locked -p aegis-client --test 146-pair-rationale` — passed, 4/4 tests.

Notes:
- The fixture uses a direct feasibility rating plus a rated damage link, so `deciding_entry()` is populated and the rationale resolves without needing an attack tree.
- XLSX: the prompt's "push `r.rationale` after `r.feasibility`" was placed immediately after the feasibility-rationale cell so the value sits under the `"Pair rationale"` header (which the prompt places after `"Feasibility rationale"`) — header/row order stays aligned, and the test reads the cell by header name. Placing it literally before the feasibility-rationale cell would have swapped the two columns' values.
- Two existing test files were touched beyond the prompt's "Do" list because the workspace-wide CI gate (`scripts/ci.sh`, a required Done-when item) fails otherwise: the new visible column and the new `Op` (introduced in 146a) must be accounted for. Neither touches forbidden files, model source, risk numbers or sort order.
- No new dependency, no `unwrap`/`expect` in library code, no `unsafe`, no dotted Fluent id.

NEXT: reviewer
