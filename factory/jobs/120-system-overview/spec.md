# Spec: 120-system-overview

## Title

Combined view across systems: boundary flows, the overview's per-system rows, top attack paths and choke points, the Versions comparison and the undo reveal, all within the view.

## Design Reference

- Requirements: `docs/requirements.md` section 2.5a, SYS-04 and SYS-05 (lines 170 and 171).
- Decision: `docs/decisions.md` D26 (line 203): a view never changes a number; no tree-to-tree links, a step goes "via" a component of another system.
- Design: `docs/brainstorm-systems-config.md` sections 5.2 (line 175, boundary flow), 5.3 (lines 190 to 205: undo, baselines, diff) and the test list for 120 (lines 488 to 495); `docs/design/systems-config.md` sections 2 (item definition), 7 (via chip) and 10 (combined view).
- Scope limit: `factory/jobs/119-systems/rework.md`, section `## Answers`, answer 1 (no new Slint screen work; test the client instead of the screen).
- Code: `crates/aegis-model/src/system.rs`, `lint.rs`, `project.rs` (`lint_data_flows`), `eval.rs` (`attack_paths`, `choke_points`); `crates/aegis-client/src/systems.rs`, `pages.rs` (`data_flow_rows`, `problems`, `overview`, `comparison`), `view.rs` (`tree_scene`, `inspector`), `session.rs` (`history`, `page_data`).

## Problem & Goal

Job 119 lets a project hold systems and lets the user look at one, a few or all. What is still missing is the picture across systems: where the systems touch, how each one stands next to the others, and which attack paths matter in the part the user looks at. After this job the client data gives all of that, and every part of it follows the view without changing a number.

Business value (P7):

| Part | Value |
|---|---|
| Boundary flows, with the "no threat" check | The links between systems are where a vehicle-level TARA finds what a per-ECU TARA misses. A link nobody analysed is flagged. |
| A step via another system's component shows that system | A reviewer of the Brake TARA sees where an attack leaves Brake and whose settings cap it. |
| Per-system rows that add up | One table for the customer's steering meeting: size and worst risk per system, and a total that matches. |
| Shared damage scenarios and controls counted once | The combined numbers are not inflated; no copies per system that drift apart. |
| Top attack paths and choke points in the view | Says where one control removes most of the attacker's best options. Bounded work, so the page stays fast. |
| Versions comparison within the view | The Brake owner reads only the Brake changes since the baseline, and is told how many more lie outside. |
| Undo reveals a hidden element | An undo never changes something the user cannot see. |
| Property test: same risk number in every view | The evidence rule of D26, checked on random projects. |

## Definitions decided here

- **Boundary flow**: a data flow whose two end components both have a system (through their top-level ancestor) and the two systems differ. A flow with one end in no system is not a boundary flow; the lint of job 119 already flags that component.
- **Threats of a flow**: the threat scenarios whose asset is bound to the flow's source or destination component. This is what the existing trust boundary lint uses.
- **"No threat" check**: a boundary flow with no threats gives the lint warning `SystemBoundaryFlowWithoutThreat`. A flow that also crosses a trust boundary gets only the existing `BoundaryCrossingDataFlowWithoutThreat`, so one flow never gives two findings for one cause.
- **Another system (step via)**: the step's via item belongs to a component whose system is not among the systems of the step's tree, and the tree has at least one system.
- **Rows of the overview table**: one per system in the view, in ID order, then a "No system" row when anything is in no system, then a total row ("All" or "This view"). Components, assets and threats are each in one row, so these three columns add up. The boundary flow column counts the flows that touch the system; the total is the number of different flows, so with all systems in view the column sums to twice the total.
- **Top attack paths**: over the attack trees linked by a threat that shows in the view, by residual feasibility, easiest first. Only the 5 trees with the easiest root value are searched (the 5 easiest paths cannot be anywhere else), 5 paths each, and the best 5 are kept. Ties keep tree order, then path order.
- **Choke points**: the steps of those same trees that lie on most of their 5 easiest paths (`Evaluation::choke_points`), best 5.
- **Comparison within the view**: an added or changed line shows when its element shows in the view in the current project; a removed line when it showed in the baseline; a moved component when its old or new system is in the view; a system line when that system is in the view. The summary counts the shown lines and says how many are outside the view.
- **Undo reveal**: after an undo or redo, when the element it selects does not show in the view, the first system of that element (ID order) is added to the view and the toast says so.

## Acceptance Criteria

1. `Project::boundary_flow(idx)` returns the two systems of a boundary flow and `None` otherwise (same system, one end in no system, no systems). The linter gives `SystemBoundaryFlowWithoutThreat` once for a boundary flow with no threats, nothing when a threat exists at either end, nothing for a project with no systems, and only the trust boundary finding when the flow also crosses a trust boundary. Check: `cargo test --locked -p aegis-model --test systems --test ops_properties --test project`.
2. The Problems rows name the flow and select it. Check: `cargo test --locked -p aegis-client --test systems`.
3. Data flow rows and the data flow detail carry the boundary text ("ADAS → Gateway"), the threat count and the "no threat" mark; the Systems table rows carry the boundary flow count and text ("Gateway 2", "3 systems 6"). A project with no systems gives no boundary text. Check: `cargo test --locked -p aegis-client --test combined`.
4. A step via a component or interface of another system shows that system on the tree's via chip ("GW · Gateway") and in the inspector; a step via its own system shows none. The step is capped by that component's settings and the threat's risk row is equal in the view of its own system and in all systems. Check: `cargo test --locked -p aegis-client --test combined`.
5. The overview has per-system rows; components, assets and threats add up to the total row and to the project counts when the view is all; the highest residual risk of a row equals the highest residual risk the model computes for that system's threats; this holds on random projects and views. Check: `cargo test --locked -p aegis-client --test combined --test combined_properties`.
6. With a view other than all, every part of the overview (the eleven counts by kind, both risk distributions, errors, warnings, the five highest risks, the system rows, the boundary flow list) covers only what shows in the view; a damage scenario or control used by two systems shows on each system's page and is counted once in a view that holds both. With all systems or no systems the overview equals `pages::overview(ws)`. Check: `cargo test --locked -p aegis-client --test combined --test pages`.
7. Property test: for random projects and random views, every risk row a view shows equals the row of the same threat in the all-systems view. Check: `cargo test --locked -p aegis-client --test combined_properties`.
8. The overview lists the top attack paths and the choke points of the view as defined above; an unrated tree is skipped; the order is deterministic; each list holds at most 5. Check: `cargo test --locked -p aegis-client --test top_paths`.
9. `pages::comparison_in` filters the lines by the view as defined above, gives the same result as `pages::comparison` for all systems, and the summary names the number of lines outside the view. The session uses it on the Versions page. Check: `cargo test --locked -p aegis-client --test combined --test systems --test pages`.
10. Headless: an undo and a redo of an edit to an element outside the view add its system to the view, select the element, open its page, say so in the toast, and add no op of their own; an undo inside the view leaves the view alone. Check: `cargo test --locked -p aegis-client --test session --test systems_view`.
11. The whole workspace passes the gate, including `aegis-shell`, which this job does not change. Check: `scripts/ci.sh`.

## Out of scope

- **The screen, left for the GPUI shell** (owner rule): the ⇄ mark and the Boundary flows column on the item definition page, the overview table, the path and choke point lists, the "· Gateway" chip drawing, the scope text on the Versions page, and the English and Japanese strings. No `.slint` file and no Slint glue in `aegis-shell` changes.
- Tree-to-tree links across systems (dropped by D26).
- Reports, exports and Excel import per system (job 121, SYS-06). `pages::overview(ws)` and `pages::comparison(..)` keep their signatures, so the report builder is untouched.
- Filtering the sidebar counts by the view.
- Revealing a hidden element when the user picks it from the command palette or the Problems page.
- Choke points grouped by component or interface across trees.
- Loading and saving the remembered view when a project opens and closes (left for the shell by job 119).
