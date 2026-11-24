## Title

137 Attacker profile (`APL-05`)

## Design Reference

- `docs/requirements.md` (`APL-05`, `APL-06`, `G50`, `G53`, `FND-10`, `RPT-09`)
- `docs/design/reference-ui-review.md` (Section 3.4 *Attack filter*, lines 281–305)
- `docs/design/workbench-breakdown.md` (Section 3 backlog split rule, lines 193–207)
- `docs/design/workbench-architecture.md` (workspace `aegis.json` layout)
- `docs/decisions.md` (`D27` unrated never excludes, `D30`, `D32`, `D37`, `D41`)
- `docs/engineering-standards.md` (Sections 1 and 2)

## Problem & Goal

An analyst often needs to scope a TARA to a specific class of attacker (for example excluding attacks that require bespoke hardware or multiple experts, or restricting to scalable fleet-wide attacks) without deleting attack paths from the tree or fabricating feasibility ratings.

Every moving part carries its own concrete value (P7):
- **`AttackFilter` & `ScalabilityFilter` on `Project`**: Stores excluded factor levels across the pack's attack potential factors and a scalability mode (`Any`, `ScaledOnly`, `UnscaledOnly`), defaulting to all allowed in O(1) without rewriting any step rating.
- **`Op::SetAttackFilter`**: Atomic, validated project edit returning the previous filter and provenance as its exact inverse so undo, redo, baseline diffs (`ProjectDiff::attack_filter_changed()`), snapshots, op logs, and `aegis.json` workspace files persist and collaborate cleanly.
- **`Reason::OutsideProfile` & `Reason::Scalability` in `Project::applicability()`**: Automatically excludes attack steps whose rated factor levels fail the profile (`OutsideProfile`) and threats whose explicit scalability fails the profile (`Scalability`), rolling exclusions cleanly up `AND`/`OR` gates and into residual risk/goals/claims while keeping unrated steps and unrated threats (`scales() == None`) included (`D27`).
- **Attacker profile view model (`attacker_profile.rs`), page (`Page::AttackerProfile`), and sidebar dot**: Gives analysts toggle chips per factor column, "Select all" per factor column, scalability filter controls, live excluded count and undo toast, and a sidebar accent dot (`nav-dots`) whenever the filter is non-default.
- **Report cover, Method section, and XLSX export**: Documents non-default attacker profiles on the report cover, in a dedicated `Method` section (with excluded-item fallback when no conditions or assumptions exist), and in the Excel workbook so exported reports are self-explanatory and auditable.

## Acceptance Criteria

1. `AttackFilter` and `ScalabilityFilter` live in `aegis-model` with default = all levels and all scalability allowed (`is_default() == true`), validated per project methodology (`Methodology::check_attack_filter`, rejecting unknown factor or level indices and non-default factor filters on packs with zero attack potential factors such as `eu_cra()`).
2. `Op::SetAttackFilter { filter, provenance }` validates provenance via `check_provenance` (accepting empty `""` default actor and rejecting whitespace-only blank or over-long actors), commits cleanly, returns `Op::SetAttackFilter { filter: old_filter, provenance: old_provenance }` as its exact inverse, updates `ProjectDiff::attack_filter_changed()` and `ProjectDiff::is_empty()`, and round-trips identically through snapshots (`save`/`load`), op logs (`append`/`replay`), and workspace files (`aegis.json`, omitting `"attack_filter"` when default) while rejecting untrusted malformed JSON, unknown fields, unknown factor/level keys (`VocabKind::FeasibilityFactor` / `VocabKind::FactorLevel`), duplicate keys (`StoreError::DuplicateEntry`), and excess counts (`Limit::FEASIBILITY_FACTORS` / `Limit::LEVELS`).
3. `Project::applicability()` records `Reason::OutsideProfile { factor, level }` on rated attack steps matching excluded bits and `Reason::Scalability` on threats with explicit `scales() == Some(...)` rejected by `ScalabilityFilter` (`ScaledOnly` rejects `Some(false)`; `UnscaledOnly` rejects `Some(true)`); unrated factors on steps and unrated threats (`scales() == None`) never exclude (`D27`). Gate propagation (`AllPathsExcluded`) and owned-tree propagation (`OwnerExcluded`) compose with existing conditions and assumptions.
4. For every attack potential factor in `Methodology::iso21434_r155()`, an exclusion filters only steps rated on that factor at the excluded level, leaving unrated steps and other-factor steps included; scalability filtering uses `Op::SetThreatScaling` (job 138).
5. `ProjectEvalCache::update` keeps single-step edits O(1) when `attack_filter.is_default()` (checked via `rating_edit_with_default_filter` on `large` `100,000` nodes for the Section 2 `<= 5 ms` edit budget), updates applicability and tree/threat evaluations on `Op::SetAttackFilter`, `Op::SetThreatScaling` (when `scalability != Any`), and `Op::SetStepFactor` (when `has_excluded_levels()`), matches `ProjectEvalCache::new` after edits, and runs `benches/applicability::set_attack_filter` on the `large` (`100,000`-node) benchmark within the Section 2 full-recheck budget (`<= 200 ms`).
6. `aegis-client` provides `Edit::SetAttackFilter`, undo labels (`undo_label.rs`), human-readable `Reason::OutsideProfile` / `Reason::Scalability` explanations and `ConditionSnapshot` post-edit toast (`exclusion.rs`), baseline diff summary in `comparison_in` (`pages.rs`), and headless `attacker_profile::AttackerProfileView` (factor columns, per-level chips, per-column select-all helper, scalability selector, excluded count) localized in `en`, `ja`, and `pseudo`.
7. `Page::AttackerProfile` sits in group 4 (`PageGroup::Feasibility`) right after `Page::Trees` (`Page::ALL.len() == 18`), `ProjectView::nav_dots` marks `Page::AttackerProfile` whenever `!attack_filter.is_default()`, and `aegis-shell` renders the sidebar entry (`@tr("Attacker profile")` in `en`/`ja`/`pseudo`) and accent dot cleanly across all 18 pages.
8. When `!project.attack_filter().is_default()`, `report_doc_with_options` includes an attacker profile row on the cover table and a `Method` heading (`section-method`) with the methodology line, factor/scalability profile table, and excluded-items table (when no conditions/assumptions emit one), HTML/PDF/XLSX previews reflect those additions, and all existing default-filter report offsets remain unchanged.
9. `scripts/ci.sh` passes across the full workspace with zero warnings or test failures.

## Out of scope

- Full `.slint` interactive page body screen for `AttackerProfileView` (`docs/design/workbench-breakdown.md` Section 3 defers `.slint` page screens; headless view model, edits, sidebar entry, and non-default accent dot are in scope).
- Per-factor qualitative attack-potential threshold rules (handled by pack feasibility tables).
- Editing `aegis-workbench` (verified: does not consume `Page::ALL`, `PageData`, `ProjectView`, `Edit`, `Op`, or `Reason` exhaustively) or `crates/aegis-shell/src/shortcuts.rs` (`palette_entries` iterates `Page::ALL` dynamically; `page_for` only maps `SHORTCUTS` table IDs).

## Decisions

- **Unrated threats under scalability filter (`ScalabilityFilter`)**: `scales() == None` never excludes under `ScaledOnly` or `UnscaledOnly` (`ScaledOnly` excludes only `Some(false)`; `UnscaledOnly` excludes only `Some(true)`), following `D27` ("We do not guess") and matching unrated factor semantics.
- **Methodology-independent storage on `AttackFilter`**: Store `[u16; MAX_FEASIBILITY_FACTORS]` bitmasks of excluded levels (`0` = all allowed) plus `ScalabilityFilter` (`Any` default) so `AttackFilter` is `Copy`, `Default`, canonical with no normalization needed, O(1) to inspect in `is_default()`, and validated against `Methodology` before commit.
- **Workspace storage (`aegis.json`) and equality**: Store optional `"attack_filter"` under `"aegis.json"` (`"workspace"`) alongside `"languages"`, omitted when default (provenance is not serialized in snapshots or `aegis.json`, so `Project::eq` compares `self.attack_filter() == other.attack_filter()` ignoring provenance, matching `languages()`).
- **Full `.slint` screen split**: Implement full `AttackerProfileView` + edits (`Command::Edit(Edit::SetAttackFilter { .. })`) in `aegis-client` and sidebar entry + `nav-dots` accent dot in `aegis-shell`, deferring the Slint page body widget per `docs/design/workbench-breakdown.md` Section 3.
- **Diff emptiness & baseline comparison**: Include `&& !self.attack_filter_changed` in `ProjectDiff::is_empty()` so `ops_properties`'s `fwd.is_empty() == (base == curr)` invariant holds when `Gen::op` generates `Op::SetAttackFilter`, add `ProjectDiff::attack_filter_changed()`, and emit `"Attacker profile changed"` in `pages::comparison_in` when `attack_filter_changed()`.
- **Report placement**: Omit the cover row, `section-method`, and XLSX `"Attacker profile"` sheet when `attack_filter().is_default()` so existing default-report block offsets stay invariant; when non-default, render `Heading(2, "Method")` right after `section-configuration` and update `report/xlsx.rs` (`has_applicability` and `"Attacker profile"` sheet).
