## Title

Bypass trees for security controls (`BLK-04`)

## Design Reference

- `docs/requirements.md`: `BLK-04`
- `docs/decisions.md`: `D28` (`ControlStatus`), `D29` (`Fragment` and bypass trees)
- `docs/design/reference-ui-review.md`: §3.12 (canvas shield badge / hatching), §3.14 (circumvent / bypass trees)
- `docs/design/workbench-breakdown.md`: §3 (keep model and client parts of bypass trees; no new `.slint` screens)
- `docs/architecture.md`: §III.7 (Shared building blocks — bypass trees, evaluation order, cache, bypass ceiling)

## Problem & Goal

Today a security control caps a mitigated attack step only with a manually typed fixed `FeasibilityLevel` (`StepMitigation::feasibility`). When a team analyses how an attacker could circumvent a control, they have no way to attach that attack tree directly to the control as its mitigation ceiling, so the analysis either drifts from manual guesses or duplicates control logic across every threat tree.

Adding an optional bypass tree (`TreeRole::Fragment`) to `SecurityControl` lets analysts express how to defeat a control once, evaluate its residual feasibility dynamically from that fragment (including nested fragments, shared steps, applicability conditions/assumptions/profile, and any controls inside the bypass tree), keep attack-type rule ceilings unchanged, name `"bypass <Level>"` (no colon inside `"bypass <Level>"`) across explain-why, reports, control uses, and canvas tooltips, edit and create bypass trees from the control inspector, and visually distinguish inactive bypass trees whose control is `Off` with `"control off"` hatching and accessible text.

## Decisions

- `StepCeiling` gains `bypass: Option<TreeId>` (`None` for rule ceilings and unbypassed step mitigations, `Some(fragment_id)` for bypassed step mitigations) because `StepCeiling` is `Copy` and already carries public `control_id: ControlId`.
- Graph validation (`Project::check_fragment_graph`) includes `(host_tree -> ctrl.bypass)` edges across all live controls regardless of `ControlStatus`, so switching `SetControlStatus` can never fail with `FragmentCycle` or `FragmentDepth`.
- Removing a control with a bypass tree returns `ModelError::SecurityControlHasBypass { control }` (and `Edit::RemoveControl` clears `SetControlBypass { tree: None }` first); removing a fragment referenced as a bypass tree returns `ModelError::FragmentInUse` (keeping `error.rs`'s existing display string unchanged while counting both fragment-step and bypass users).
- `Op::SetControlBypass` attaches only `TreeRole::Fragment` trees; snapshot serialization omits `bypass` when `None` (preserving all existing fixtures) and replays `pending_bypasses` after `attack_trees` load using the control's provenance so loaded provenances and visual IDs match byte-for-byte; workspace `blame` blames `"shared/controls.json"` when control bypass validation fails.
- When a counting control has a bypass tree whose evaluation is `Lifted::Excluded`, a mitigated step evaluates to `Lifted::Excluded` (mathematically `step AND bypass_tree`), unless another counting mitigation's bypass tree fails with a missing leaf/empty gate (`None`), in which case missing-input error takes precedence over exclusion; `step_ceilings` omits an unevaluable or excluded bypass ceiling while `residual_step_feasibility` returns `None`.
- Every explain-why and report surface uses `"bypass <Level>"` (no colon): step explain-why and Slint node inspector emit `"{name}: bypass {lvl}"`; threat why lines emit `"Step \"{step}\" is capped at bypass {lvl} by {name}."`; `control_uses` emits `"in \"{title}\", caps feasibility at bypass {lvl}"`; `ReducedByEntry` puts `"bypass {lvl}"` in `on` and `"{lvl}"` in `ceiling` (so `reduced_by_text` also carries `"bypass {lvl}"`); `tree_scene` `capped` emits `"{name} (bypass {lvl})"`.
- `Edit::CreateBypassFromSelection` copies the selected tree or node subtree (`CopyMode::PreserveReferences`) into a new `TreeRole::Fragment` and links it via `SetControlBypass` in one undo step without mutating or removing the source tree branch; `Edit::DuplicateControl` leaves `bypass` uncopied (consistent with `Edit::DuplicateThreat`).
- `Workbench` remembers the latest live `Selection::Tree` or `Selection::TreeNode` in `last_tree_selection` so `"Create from selection"` in the `InspectorPanel` `"Bypass tree"` row works in GPUI without adding `.slint` UI (`workbench-breakdown.md` §3); `ControlDetail` in `aegis-client::pages` gains `bypass: Option<LinkRow>` to stay aligned, and `Selection::Control` in `inspector_view` reads `ws.cache().evaluate_residual_tree(project, b_id)` (showing `"Not applicable"` with `None` tone on `TreeNotApplicable`, `"Not rated"` with `None` tone on missing-leaf/empty-gate errors).
- A fragment tree is hatched with `"control off"` on the canvas (`NodeStyle::excluded_label`) and carries `"control off"` in `aegis_canvas::access` when at least one control references it as `bypass`, every such control has `ControlStatus::Off`, and no `UseFragment` step references it.

## Acceptance Criteria

1. `SecurityControl::bypass(&self) -> Option<TreeIdx>` and `Op::SetControlBypass { control, tree, provenance }` exist with exact inverses, `validate`/`apply` agreement, `FragmentRole`/`FragmentCycle`/`FragmentDepth` checks on `SetControlBypass` and `SetStepMitigation`, removal guards (`SecurityControlHasBypass`, `FragmentInUse`), `same_control` comparison by `TreeId`, snapshot/op-log/workspace persistence (omitted when `None`) with `"shared/controls.json"` workspace `blame`, and pass `ops_properties`, `store`, `store_limits`, and `workspace`.
2. While a control counts per D28 (`Implemented` in initial+residual, `Planned` in residual only, `Off` in neither), its step mitigations take the bypass tree's evaluated feasibility in that mode instead of `m.feasibility`; `ControlRule` ceilings are unchanged; `StepCeiling::bypass(&self) -> Option<TreeId>` names the bypass fragment on bypassed step mitigations and `None` otherwise.
3. Edits inside a bypass tree (including via nested fragments, shared steps, controls, and conditions/assumptions/profile) update step ceilings, threat risks, `Project::diff`, and `ProjectEvalCache` incrementally; `ControlStatus::Off` removes the bypass ceiling; a proptest cache property test compares `ProjectEvalCache` against an independent recursive oracle and a fresh `Project` across random bypass edits, `SetStepMitigation` mutations, and status switches.
4. `StepCeiling::bypass` drives `"bypass Medium"` (and pack feasibility labels) in `aegis-client` inspector, threat why lines, `reduced_by` / `reduced_by_text`, `control_uses`, and canvas shield `capped`, and adds a bypass-tree cause link in `InspectorExplain::causes`.
5. The relation registry has `RelationKey::CONTROL_BYPASS` (`"Bypass tree"`, single-target fragment picker + clickable chip + reverse relation `"Bypass tree for"` on fragments), `Selection::Control` in `InspectorView` shows a `"Bypass tree"` field and `ControlDetail` populates `bypass`, `diff_lines` reports `"bypass tree changed"`, `Edit::SetControlBypass` and `Edit::CreateBypassFromSelection` execute and undo atomically in one step, and the GPUI `InspectorPanel` offers `"Create from selection"` on the `"Bypass tree"` row (tested headless in `aegis-workbench`).
6. A bypass tree whose control(s) are `Off` (and not used elsewhere) renders hatched with `"control off"` in `aegis-canvas` / `tree_scene` and carries `"control off"` in accessible text (`aegis_canvas::access::describe` / `sentence`); existing `excluded` hatching and tests remain unchanged.
7. Full workspace CI (`scripts/ci.sh`) and crate checks exit 0 with zero clippy warnings and zero unapproved dependencies (`new_deps: false`).

## Out of scope

- Multi-step or direct-from-gate bypass attachment outside `SecurityControl::bypass`.
- New `.slint` screens or buttons in `crates/aegis-shell/ui/` (`docs/design/workbench-breakdown.md` §3).
- Report/XLSX export sections for shared blocks and bypass trees (Job `144-blocks-in-outputs`).
