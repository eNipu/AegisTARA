# EGM Spec Audit: 136-assumptions

Audited `spec.md`, `plan.json`, and `prompts/136{a..e}.md` via `egm-codebase-details` and `egm-implementation-readiness`.

## Architectural contradictions
None found against `docs/decisions.md` (D27, D30, D32, D37, D40, D41, D43).
- `136c` places `assumptions` at `systems/main/assumptions.json` in `WorkspaceFile::LAYOUT` and embeds `assumptions` inside `CybersecurityClaim` (`shared/claims.json`).
- `136e` keeps `crates/aegis-shell` limited to index/nav-entry parity (respecting D37).

## Blocking defects

1. **`crates/aegis-client` compilation & `allow` scope in `136a`/`136b` (`undo_label.rs`, `inspector.rs`, `commands.rs`)**
   - `136a` and `136b` run `"ci": ["crates/aegis-model", "crates/aegis-client"]`, while adding 4 + 3 `Op` variants and `ElementRef::Assumption`, yet `crates/aegis-client/src/undo_label.rs` (and any exhaustive matches on `Op`/`ElementRef` in `aegis-client`) is omitted from `136a`/`136b`'s `allow` lists and deferred to `136d`.
   - Fix: verify exhaustive `Op`/`ElementRef` matches in `crates/aegis-client` (e.g. `undo_label.rs`, `exclusion.rs` `reason_target`, etc.) and either include minimal match arms in `136a`/`136b` (adding those paths to `allow`) or adjust `ci` on `136a`/`136b` to `["crates/aegis-model"]` until `136d` wires client ops — wait: check if `aegis-client` tests (`undo_every_op`) must pass when `aegis-client` compiles, and keep `ci` passing at every slice! Putting `undo_label` / minimal `ElementRef::Assumption` stubs where exhaustive matches live (or narrowing `ci` to `["crates/aegis-model"]` for `136a`/`136b` if `aegis-client` does not need partial `view.rs`/`systems.rs` until `136d`) — better: keep each slice cleanly compiling in `ci` and include every file with an exhaustive match in `allow`.

2. **Wrong `Read first` symbols and line ranges in `prompts/136a.md`**
   - `136a.md` cites `crates/aegis-model/src/diff.rs:250-420` for `ProjectDiff::conditions` and `is_empty`, which are actually at `diff.rs:765-854` (`250-420` is `RiskChange`/`RiskDelta`).
   - `136a.md` cites `crates/aegis-model/src/error.rs:636-655` and compares `EntityKind::Assumption` to `Condition` with `check_name`: `EntityKind` (`error.rs:35-94`) has no `Condition` variant (`project.rs:9712` uses `check_condition_name`), and `ConditionInUse` is at `error.rs:705-714` (`637-647` is `TooManyConditions`, `648-655` is `ConditionNotLinked`).
   - `136a.md` cites `check_description_target` and `ProjectEvalCache::eval` instead of `live_target` (`project.rs:8625`) and `ProjectEvalCache::update` (`project.rs:13602`). Also explicitly note `merge_condition_link_changes` (`project.rs:6470`, wildcard `_ => {}`) so `ElementRef::Assumption(id)` is not missed.

3. **`136b.md` inverted boolean typo, `within_record_cap` ambiguity, and L09/L15/L30 test gaps**
   - `136b.md` line 50 writes `"include !self.assumptions.iter().any(...) in the rated / blocked guard"` — the `!` inverts the guard and would block non-excluded steps! Remove `!` so excluded steps are blocked with `ModelError::NodeIsRated(node_id)`.
   - `136b.md` line 38 leaves `"and within_record_cap check if needed"` and exact `StoreError` variant (`StoreError::Limit(Limit::ASSUMPTION_EXCLUDES)`) unspecified. State explicitly whether `within_record_cap` handles `SetAssumptionExcludes` and the exact `StoreError` returned on combined `> 256` overflow.
   - `136a.md` & `136b.md` must explicitly require:
     - **L15**: blank-actor `Provenance` rejection tests for all 6 new provenance-carrying `Op`s (`AddAssumption`, `RenameAssumption`, `SetAssumptionHolds`, `SetAssumptionExcludes`, `LinkClaimAssumption`, `UnlinkClaimAssumption`) and `check_provenance` in `136a`.
     - **L09**: single-guard isolation tests asserting the exact `ModelError` variant (e.g., `AssumptionInUse` with only a claim link; `ElementNeedsConditions` with only a condition link; combined `<= 256` trees + `<= 256` steps summing to `> 256`, e.g., 130 + 127 = 257, returning `TooManyAssumptionExcludes { max: 256 }` / `StoreError::Limit(Limit::ASSUMPTION_EXCLUDES)`).
     - **L30**: full-`Project` equality (`assert_eq!(before, after)`) after clearing links/exclusions, removing `Assumption`, `CybersecurityClaim`, `AttackTree`, and `TreeNode`, and undoing.

4. **`136c.md` ambiguities, `ConditionVisual` line range, and inverted feasibility test**
   - `136c.md` line 60 contains a raw scratchpad note (`"Notice: compute these tree exclusions BEFORE propagate_owners? Wait..."`); specify the exact ordering inside `Project::applicability` (`tree_exclusions` vs `propagate_owners`, and whether `tree_exclusions` skips or evaluates trees already excluded by `AssumptionHolds`).
   - `136c.md` lines 43, 47, 50: give the exact line range for `ConditionVisual` in `wire.rs`, decide explicitly whether `holds` uses `skip_serializing_if` or explicit boolean (or matches `Condition::present`), and specify `Project::load` claim-link collection directly.
   - `136c.md` line 68 asserts `"raises gate feasibility"` — excluding a step **lowers or eliminates** attack feasibility, never raises it! Require hand-calculated expected `Feasibility` and `RiskLevel` values before toggle, while `holds == true`, after `holds == false`, and after re-enabling `holds == true`.
   - `136c.md` line 71: enforce L09 combined `> 256` cap test (130 live trees + 127 live steps = 257).

5. **`136d.md` missing outline step-move handling (`tree_diff.rs`) and `Edit::RemoveClaim` undo test**
   - `crates/aegis-client/src/tree_diff.rs` (`replace_tree_ops` / `transfer_step_ops`) moves steps via `RemoveTreeNode` + `AddTreeNode`. Because `clear_descriptions_before_removal` strips `excludes_steps` before `RemoveTreeNode`, `tree_diff.rs` must also re-emit `SetAssumptionExcludes` after `AddTreeNode` (and add `crates/aegis-client/src/tree_diff.rs` to `136d`'s `allow` list), with a test confirming outline step-move preserves assumption exclusions.
   - Add an L30 `Edit::RemoveClaim` 1-step removal + undo + full-`Project` equality test on a claim with linked assumptions.

6. **`136e.md` wrong `PageLayout::Split`, report/XLSX assumption-only guards, missing Fluent keys, `AssumptionsView` ambiguity, `app.slint` icon, and `aegis-workbench` `allow` check**
   - **`PageLayout::Split` does not exist**: `PageLayout` only has `Table`, `Canvas`, and `Custom`. `Page::Assumptions` must use `PageLayout::Custom` (`Page::tables()` maps `None => &[]`, matching `Page::Configuration`).
   - **`doc.rs` section guard**: `crates/aegis-client/src/report/doc.rs:1256` gates `"Configuration and assumptions"` (`section-configuration`) on `project.conditions().next().is_some()`. Widen to `project.conditions().next().is_some() || project.assumptions().next().is_some()`, guard the `"Conditions"` sub-table on `project.conditions().next().is_some()`, keep `"Excluded elements and steps"`, and append `"Assumptions"` when `project.assumptions().next().is_some()` (preserving block indices for condition-only `report_configuration.rs`).
   - **Fluent keys for reports**: `doc.rs` uses Fluent `catalog.tr` / `catalog.tr_with` across `locales/{en,ja,pseudo}/main.ftl` (no German locale exists). Specify exact hyphenated Fluent keys and `en`/`ja`/`pseudo` strings for the report cover/table headers.
   - **`xlsx.rs` second guard**: `crates/aegis-client/src/report/xlsx.rs:379-380` (`risk_register_sheet`) independently checks `let has_conditions = p.conditions().next().is_some();`. Update both `risk_register_sheet` (`379-380`) and `report_workbook_xlsx_in` (`461-462,977`) to `has_applicability = p.conditions().next().is_some() || p.assumptions().next().is_some()`.
   - **`AssumptionsView` / `affects_count` & `session.rs` draft fallback**: Specify exact field names/types on `AssumptionsView` (and any item/detail/link structs) and define `affects_count` unambiguously. Ensure `Page::Assumptions` selection / `BeginDraft(DraftKind::Claim)` cannot fall back visually to the first live assumption while `open.ui.selected_assumption` remains `None`.
   - **`app.slint` icon**: Explicitly name verified icon `icons/list-checks.svg` (`crates/aegis-shell/ui/app.slint:531`) instead of asking the builder to search for one.
   - **`aegis-workbench` cross-crate check**: Check whether `crates/aegis-workbench` matches `Page` / `Command` exhaustively or constructs `UiState` / `PageData` / `ProjectView` literals; if so, include the exact workbench files in `allow` (note: `plan.json` `"ci": []` runs full-workspace `scripts/ci.sh`, which checks `aegis-workbench`).

7. **Unrealistic `max_lines` caps / overloaded slices (`136a`, `136c`, `136d`, `136e`)**
   - **`136e` (`max_lines: 520`)** bundles 4 subsystems (`assumptions.rs`, `exclusion.rs`, `view/state/pages/commands/session`, `report/doc.rs` + `report/xlsx.rs`, plus `app.slint` + locales), projected at ~465–650 counted lines (risking >600 cap). **Split reports (`doc.rs`, `xlsx.rs`, and report Fluent keys) into `136f`**, giving `136e` and `136f` clean headroom (`<= 450` / `<= 350`). Move `"ci": []` to the final slice (`136f`), and set `136e` `"ci"` appropriately (`["crates/aegis-client", "crates/aegis-shell"]` or `[]` if `aegis-workbench` is touched).
   - **`136a` (`max_lines: 480`)**, **`136c` (`max_lines: 360`)**, and **`136d` (`max_lines: 510`)** also have tight headroom (remembering `locales/*.ftl` and inline `#[cfg(test)]` inside `src/*.rs` are counted by `verify.py`). Raise `136c` `max_lines` (e.g., `460`–`500`) and check `136a`/`136d` caps (up to `560`–`590`, keeping every slice comfortably ≤600, or split if needed). Also note D41's 400-line preference where easy to trim without `#[rustfmt::skip]` (L28).

## Non-blocking suggestions
- **`ConditionSnapshot` shape (`136e`)**: `crates/aegis-client/src/exclusion.rs:139-143` stores `condition: ConditionId`; generalize via an enum target (`Condition(ConditionId)` / `Assumption(AssumptionId)`).
- **L20 test coverage in `136e`**: Test `AssumptionsView` rows/fallback/`affects_count`/conditioned-off flags, `find_rows` & `nav_counts` on `Page::Assumptions`, `ConditionSnapshot` toast producing 0 changes when an assumption's own condition is off, and toggling `holds` while conditioned off producing zero `Reason::AssumptionHolds`.
- **Exact text specs (`136d`/`136e`)**: Spell out exact `en`/`ja`/`pseudo` strings for all new Fluent keys, `undo_label.rs` strings, `unchanged()` rules for new `Edit`s, `xlsx.rs` `Holds` formatting (`Yes`/`No`), and `CancelDraft` / unknown-ID handling on `CreateClaimFromAssumption`.
- **L09 guard test hygiene (`136b`)**: Test `NodeIsRated` on an otherwise completely unrated/unlinked step so only the assumption guard triggers.
