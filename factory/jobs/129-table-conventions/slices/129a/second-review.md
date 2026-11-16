VERDICT: CHANGES (nothing is broken; the convention cannot be used as built)
Reviewed: 5478abf..cd27263 (merged as 1b80c94), against spec.md, the backlog line, architecture III.9.1 and UI-14.
CI: scripts/ci.sh green on master including this merge (run during 126b).

## What is fine

- Cutting the Slint work was the right call under the owner rule (no new Slint work).
- The change is additive and nothing reads it, so no page, report or export changed.

## Findings

1. Nothing connects a column to a value. `ColumnSpec` has no column key and no way to get a cell from a row, and no page builds its rows from its spec. The workbench (189) cannot render a tab from it and the column chooser (129b) has nothing stable to remember. AC8 ("every table page's columns come from its spec") is not met: `constants_match_page_layouts` compares each constant with itself.
2. The labels cannot resolve. The field is documented as a Fluent key, but none of the `column.*` keys exist in `locales/`, and a dot is not valid in a Fluent message name, so `Catalog::tr` would show "[column.id]". Several headings (asset, damage, component) are also names the methodology pack owns; a fixed catalog key would bypass the pack and break the methodology-neutral rule.
3. The specs do not match the pages they describe, so rendering from them would lose columns:
   - Assets: the per-property columns (`AssetRow.properties`, one per pack property) are missing.
   - Damage: one "impact" column, where the page has one per pack impact category.
   - Risks: asset, damage, impact, feasibility, backing and reduced-by are missing.
   - Problems: a "name" column that `ProblemRow` does not have.
4. `Table(&'static [ColumnSpec])` cannot express columns that come from the pack (properties, impact categories, attack factors for job 152). The convention needs a column kind that expands per pack vocabulary, or a layout built from the methodology. This also needs a line in architecture III.9.1.
5. `editable()` is wrong for derived pages: every Problems column and the Risks name and treatment report as editable. The spec's own claim is "computed columns are non-editable by construction".
6. The tests cannot fail for a real mistake: `exact_kind_sequences` restates the constants, `every_page_has_a_layout` asserts nothing, and the Risks and Problems specs are not tested at all.
7. Dropped scope is tracked nowhere. The backlog line is ticked, but UI-14.1 to UI-14.3 (computed header mark, link chips, excluded-row hatch) and "pages moved with no visible loss" appear in no open backlog item and not in the workbench breakdown for 189. Job 135 expects the hatch from this job.
8. Smaller: `visible` is always true; "pixels (or arbitrary units)" should be one unit; the seven specs are 220 lines of repeated struct literals at the end of `view.rs` with no doc comments, where one `const fn` would make them about 50; the relation keys ("asset-component") are invented here and must match the registry of job 149.

## Process note

Same pattern as job 126: the review returned "Findings: none" and "Risk: none". It checked that the types exist, not that anything can use them. The planner's reduced spec also turned "pages moved onto the convention" into "constants published", which removed every check that the specs are true.

NEXT: planner (follow-up 129c-table-convention-rework)
