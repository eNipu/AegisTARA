## Title
Process map on the Overview (UI-18), client level: page-group boxes with counts, linter marks, clause_refs in packs

## Design Reference
- `docs/requirements.md` UI-18
- `docs/ui-ux-requirements.md` UI-18.1
- `docs/design/reference-ui-review.md` section 3.1 (process diagram)
- `docs/decisions.md` D24 (packs), D30 (page groups), D37 (workbench), owner rule 2026-11-11 (no new Slint work)
- `docs/design/workbench-breakdown.md` section 3 (process map dropped from Slint; moves to workbench)
- `factory/jobs/129-table-conventions/spec.md` (precedent for client-level scoping)
- Code: `crates/aegis-client/src/view.rs` (`Page`, `PageGroup`), `crates/aegis-client/src/pages.rs` (`overview`, `problems`, `nav_counts` logic in session), `crates/aegis-model/src/methodology/mod.rs`, `crates/aegis-model/src/methodology/pack.rs`

## Problem & Goal
The Overview has no navigation/progress view: the user cannot see the TARA phases as boxes with element counts and a completeness mark, and cannot jump to a phase. Clause references (ISO RQ numbers / CRA Annex I) are not pack data today, so packs cannot carry them. Business value: a single source of truth for the process map in `aegis-client` that the GPUI workbench (job 189) renders without per-screen code, and pack data that carries clause references while old packs still load (rule P7: derived/computed view, never stored as project state).

## Acceptance Criteria
1. `MethodologyDef` and `Methodology` carry `clause_refs: BTreeMap<Arc<str>, Vec<Arc<str>>>` (page key -> refs), optional in the pack format; packs without it decode and yield an empty map; packs with it decode and expose the refs; round-trip preserves them.
2. `Methodology::clause_refs(page_key: &str) -> &[Arc<str>]` returns the refs for that page (empty when absent).
3. `Page::key() -> &'static str` exists for every `Page` variant.
4. `PageGroup` is re-exported/usable from `aegis-client::pages` process map.
5. `aegis-client::pages::ProcessMapBox` struct exists with fields: `group: PageGroup`, `label: String`, `count: usize`, `findings: usize`, `errors: usize`, `mark: String`, `clause_refs: Vec<String>`, `accessible_name: String`, `target: Page`.
6. `aegis-client::pages::process_map(ws: &Workspace, baselines: usize) -> Vec<ProcessMapBox>` returns one box per `PageGroup` in sidebar order, with `count` = sum of that group's page counts by `nav_counts`'s rule, including `Versions` = `baselines`, so the Project box equals the sidebar's Project group; `findings`/`errors` = lint findings whose target page is in the group, `mark` = `"✓"` when `findings == 0` else `"⚠ {findings}"`, `clause_refs` = the union of the pack's refs for the group's pages, and `accessible_name` containing the label, count and mark.
7. `target` is the first page (in `Page::ALL` order) belonging to the group (or `Page::Overview` for none).
8. The per-page count match names every page explicitly; no wildcard arm returns 0 for a page that `nav_counts` counts (no silent 0).
9. A test with one asset and one threat scenario without a feasibility rating asserts the Assets-and-threats box's `errors == 1`, `findings == errors` plus the group's warning rows counted in the test from `problems()` and `page_of`, and `mark == "⚠ {findings}"`; deleting the error increment fails it.
10. The `MethodologyDef::clause_refs` doc in `crates/aegis-model/src/methodology/mod.rs` names real `Page::key()` values (`"assets"`, `"threats"`, ...) and points a reader at `Page::key()` in plain text (no cross-crate doc link).
11. `cargo test --locked -p aegis-model` and `cargo test --locked -p aegis-client --test process_map` pass.

## Out of scope
- Slint/`aegis-shell` rendering of the process map, clickable widgets, light/dark screenshots. These are UI work and, under the owner rule of 2026-11-11, are left for the GPUI workbench (job 189 / WBX-07).
- Hiding pages via a pack `pages` list (separate, already in backlog).
- Changing the snapshot or op-log format.

## Decisions
- 2026-11-16: scope reduced to the toolkit-independent client level plus the pack data, following the precedent of job 129: no `.slint` or `aegis-shell` changes, because the owner ruled no new Slint work and future UI is GPUI. The process map (`ProcessMapBox`) is kept early because the workbench Overview (job 189) reads it.
- 2026-11-16: `clause_refs` is an optional `BTreeMap<page key, Vec<ref>>` in the pack; a missing field decodes to an empty map, so every existing pack loads unchanged. The ISO pack may be updated to carry RQ numbers in a follow-up; this job only adds the capability.
- 2026-11-16: `Page::key()` returns the page's snake-ish lowercase key used as the pack's page key (`"assets"`, `"threats"`, ...); `Page::Overview` maps to `"overview"`.
- 2026-11-16: completeness mark counts all lint findings in the group (errors and warnings); `errors` is reported separately for the GPUI shell.
- 2026-11-17: finding 1 is fixed by widening the API to `process_map(ws, baselines)` rather than dropping baselines from the sidebar sum, because a box must show the same number as its sidebar group and only `session::Open` holds the baseline list; the caller passes `open.baselines.len()` and version counting is never silently 0.
- 2026-11-17: findings stay attributed by the lint target's page (`page_of`): a treatment error on a threat counts in "Assets and threats", and a row with no target counts in no box; the mark reports findings, not evaluation blockers, so the two rules can legitimately differ.
- 2026-11-17: the final gate runs with base `ae151cf` after the builder merged master into the branch (8172f68, keeping both sides), so 131b's allow is widened to the job union (131a's plus 131b's); `max_lines` stays 320, still above the union's ~244 counted source lines.