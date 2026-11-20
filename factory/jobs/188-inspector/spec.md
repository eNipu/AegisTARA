## Title
188-inspector — Inspector with Trace Spine Neighbours, Explain-Why Traces, and Go to Cause (WBX-06)

## Design Reference
- `docs/design/workbench-breakdown.md` — Group 5: `WBX-06` ("The inspector shows the selection, its neighbours on the trace spine, and explain-why with go to cause"), job `188-inspector` ("Go to cause from a risk lands on the deciding step")
- `docs/design/workbench-architecture.md` — Section 2 Rule 3 ("Every relation is a row in the relation registry. The inspector, trace views, search and reports read that table"), Section 4 (trace spine: forward and backward neighbours), Section 5.3 (Explain-why)
- `docs/design/project-explorer.md` — Section 4 ("A project-level element shows 'Used in: Gateway, Brake domain' in its inspector"), Section 9a
- `docs/requirements.md` — `WBX-06`, `WBX-10`, `WBX-11`, `UI-02`, `UI-15`, `CFG-01`, `CFG-05`, `APL-06`
- `docs/decisions.md` — `D30` (relation registry), `D33` (scope levels), `D37` (workbench shell), `D40` (GUI first)
- `factory/jobs/207-gpui-spec/spec.md` — Sections 1–4 (core view-model logic in `aegis-client`, thin GPUI views in `aegis-workbench`, headless keyboard tests)
- Existing code:
  - `crates/aegis-client/src/relations.rs` (`RELATIONS`, `RelationKey`, `ElementKind`, `related`, `reverse_related`, `linked`)
  - `crates/aegis-client/src/pages.rs` (`threat_detail`, `component_detail`, `interface_detail`, `control_detail`, `selection_visual_id`, `changed_text`)
  - `crates/aegis-client/src/table.rs` (`LinkChip`, `Tone`, `selection_exclusion`, `selection_label`)
  - `crates/aegis-client/src/view.rs` (`Selection`, `RiskView`, `inspector`, `steps_through`, `through_label`)
  - `crates/aegis-client/src/command_table.rs` (`CommandSpec`, `COMMANDS`)
  - `crates/aegis-workbench/src/app/inspector.rs` (`InspectorPanel`, `InspectorRelationRow`)
  - `crates/aegis-workbench/src/app/workbench.rs` (`Workbench`, `open_target_skipping`, `select_element_skipping`)

## Problem & Goal
Today `InspectorPanel` in `aegis-workbench` (built in job 189 for `UI-15`) only renders the selection's visual ID, name, kind, an optional excluded badge, and forward `RELATIONS` rows with the inline `MultiLinkPickerView`. It does not show:
1. Where the element lives or is used (`Used in: Gateway, Brake ECU`) and its provenance (`Last changed by …`).
2. Backward neighbours on the trace spine (`reverse_related` from `RELATIONS`, plus steps going through a component/interface via `steps_through`, and steps capped by a control via `step_ceilings`).
3. Computed summary facts and ratings (threat/risk inherent & residual risk badges, deciding damage & impact, feasibility, treatment; tree/gate/step evaluated & residual feasibility, attack potential, via, attack type; damage impacts; asset security properties; control preset & rules; party role; condition present state; description text).
4. **Explain-why** derivation traces for threats/risks, attack trees, gates, and steps (including deciding damage × feasibility, gate rules and deciding children along `why_path`, and every security control ceiling with its source).
5. **Go to cause** navigation (`go_to_cause` / `"go-to-cause"` command) that jumps from a threat/risk, tree, gate, step, or excluded element to the deciding step, deciding child, deciding damage scenario, deciding control, or off condition—and reveals `Selection::TreeNode(tree_id, node_id)` in the explorer under its owning tree while inspecting the step itself.

**Business value (P7):** In an ISO/SAE 21434 or UN R155 audit, every risk score and feasibility rating must be justifiable on the spot ("why is this risk R3 after controls, and which attack step or control caused it?"). Providing the full trace-spine neighbourhood, step-by-step explain-why derivation, and one-action "Go to cause" directly in the inspector lets analysts verify causes and navigate the security case in one click or keystroke without hunting through trees and tables.

**Goal:**
1. Build a pure, toolkit-independent inspector view model in `aegis-client::inspector` (`InspectorView`, `InspectorField`, `InspectorExplain`, `CauseTarget`, `inspector_view`, `primary_cause`) covering all 14 `Selection` variants.
2. Add the `"go-to-cause"` command (`CommandGroup::Navigate`, icon `"external-link"`, key `"cmd-alt-g"`, `in_palette: true`) and inspector section/reverse-relation Fluent strings across `en`, `ja`, and `pseudo`.
3. Upgrade `Workbench` and `InspectorPanel` in `aegis-workbench` so selecting any element (including `Selection::TreeNode`) renders its header (with system usage, provenance, and exclusion banner), summary fields, explain-why trace with clickable cause buttons, forward relation rows (with inline `MultiLinkPickerView`), and reverse trace-spine neighbour rows—and `"go-to-cause"` lands on the deciding step or cause without double-leasing GPUI entities.

## Acceptance Criteria
1. **Complete toolkit-free inspector view model (`aegis-client`):** `aegis_client::inspector::inspector_view(ws, sel, catalog)` returns `Some(InspectorView)` for every live `Selection` across all 14 `Selection` variants (and `None` for stale/deleted IDs), populating:
   - Header: `visual_id` (`"AT00"` for `TreeNode`), `name`, `kind`, `systems: Vec<String>` (via `SystemUse`), `excluded_reason: Option<String>`, ` exclusion_causes: Vec<CauseTarget>` (off conditions / excluded owners / excluded via items), `changed: String`, and `description: String`.
   - Summary `fields: Vec<InspectorField>` per element kind (including `Tone` on rated risk/feasibility fields).
   - `relations: Vec<InspectorRelationRow>` (forward rows from `RELATIONS`, preserving `editable: rel.link.is_some()`).
   - `reverse_relations: Vec<InspectorRelationRow>` (non-empty reverse rows from `RELATIONS` via `reverse_related`, plus steps through a component/interface and steps capped by a control).
   - `explain: Option<InspectorExplain>` and `primary_cause: Option<Selection>` for `Threat`, `Tree`, `TreeNode`, and any excluded element.
   Check: `cargo test --locked -p aegis-client --test inspector_view`.
2. **Go-to-cause resolution (`aegis-client`):** `primary_cause(ws, sel)`:
   - For a `Threat` with an evaluated attack tree (residual if available, else inherent), returns `Some(Selection::TreeNode(tree_id, step_node_id))` for the deciding leaf step (`Project::deciding_step` on residual or `why_path` leaf on inherent), falling back to `Some(Selection::Damage(deciding_damage_id))` when the threat uses direct feasibility and has a deciding damage entry.
   - For a `Tree(tree_id)`, returns `Some(Selection::TreeNode(tree_id, step_node_id))` for the root's deciding leaf step.
   - For a `TreeNode(tree_id, node_id)`: for a gate, returns `Some(Selection::TreeNode(tree_id, decisive_child_id))` from `Evaluation::explain`; for a capped step, returns `Some(Selection::Control(deciding_control_id))` from `step_ceilings().first()`; for an uncapped step with a `via` item, returns the `Component` or `Interface` selection.
   - For an element or node excluded by an off condition (`Reason::ConditionOff(cid)`), returns `Some(Selection::Condition(cid))` when no evaluation cause exists.
   Check: `cargo test --locked -p aegis-client --test inspector_view`.
3. **Command table & i18n (`WBX-11`):** `"go-to-cause"` is registered in `COMMANDS` (`Navigate`, icon `"external-link"`, context `None`, keys `Some("cmd-alt-g")`, `in_palette: true`) with translated label and tooltip in `en`, `ja`, and `pseudo`, plus all new `inspector-*` and `inspector-rev-*` Fluent keys. Check: `cargo test --locked -p aegis-client --test command_table && cargo test --locked -p aegis-client --test palette`.
4. **TreeNode explorer reveal & GPUI inspector (`WBX-06`, `UI-15`):**
   - `Workbench::open_target_skipping` on `Selection::TreeNode(tree_id, node_id)` reveals and opens the owning `Selection::Tree(tree_id)` row in the explorer while setting `InspectorPanel`'s selection to `Selection::TreeNode(tree_id, node_id)`.
   - `InspectorPanel` renders header metadata (systems, provenance, excluded badge), summary fields (with risk/feasibility tone pills), the Explain-why section with clickable cause buttons and a primary "Go to cause" button, forward relation rows (with inline `MultiLinkPickerView`), and reverse trace-spine neighbour chips.
   - Running `"go-to-cause"` (via shortcut, palette, `Workbench::run("go-to-cause", ...)`, or clicking a cause button in `InspectorPanel`) from a risk/threat lands on the deciding `Selection::TreeNode(tree_id, step_id)` in the inspector and opens the tree tab, without any GPUI double-lease panic.
   Check: `cargo test --locked -p aegis-workbench --test inspector_view && cargo test --locked -p aegis-workbench --test picker_view`.
5. **Workspace gate:** Full workspace passes formatting, clippy, and all tests. Check: `scripts/ci.sh`.
6. **Owner visual check (L11):** Visual inspection in a live window (light & dark themes, English & Japanese) of the inspector's trace-spine neighbours, explain-why card, tone badges, and go-to-cause navigation.

## Out of scope
- Interactive attack tree canvas rendering and node context menus (job `187-tree-tab`).
- Editing attack step factors, gate logic, or control rules inside the inspector (belongs to `187-tree-tab` and `190-settings-layers`; `InspectorPanel` edits N:M links via `MultiLinkPickerView` as built in 189).
- Markdown live-preview editor for descriptions (job `195-item-tab`).

## Decisions
- **`Selection::TreeNode(tree_id, node_id)` navigation in `Workbench::open_target_skipping`:** The project explorer indexes attack trees at `Selection::Tree(tree_id)` (`ExplorerIndex` does not create explorer tree rows for individual attack tree nodes). When `open_target_skipping` is called with `Selection::TreeNode(tree_id, node_id)`, `Workbench` reveals and opens the `Selection::Tree(tree_id)` explorer element row in the center dock, and passes the exact `Selection::TreeNode(tree_id, node_id)` to `select_element_skipping` so the inspector focuses the deciding step.
- **Keep `InspectorRelationRow` re-exported from `aegis_workbench::app`:** Defined in `aegis_client::inspector` and re-exported by `aegis_workbench::app::inspector` and `aegis_workbench::app` so all existing `picker_view` tests continue to compile without change.
- **Reverse relation labels:** Each `RelationKey` gets a dedicated reverse Fluent key (`inspector-rev-<relation-key>`, including `inspector-rev-step-via` for steps through a component/interface and `inspector-rev-step-mitigations` for steps capped by a control), all in `en`, `ja`, and `pseudo` with hyphens only (L27).
- **Double-lease safety on cause navigation:** Clicking any cause chip/button inside `InspectorPanel` calls `Workbench::open_target_skipping(target, Some(cx.entity_id()), window, cx)` and then updates `self` from `wb.read(cx)` after `wb.update` returns—identical to `InspectorPanel::open_chip`.
