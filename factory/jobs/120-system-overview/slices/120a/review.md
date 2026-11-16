VERDICT: PASS
## For the owner
What changed: The model can now tell when a data flow links two different systems (a "boundary flow"), computed on the fly and never stored.
The linter warns about such a flow when neither end component has a threat scenario. This is the same rule as the existing trust-boundary warning, and one flow never gets both warnings.
The Problems page shows the new warning as "Data flow "<name>" links two systems but has no threat scenario." Clicking it selects the flow.
Look at:
- crates/aegis-model/src/project.rs:5716 — the early return now runs when there are no flows, or when there are neither trust boundaries nor systems. Before, a project with no trust boundaries skipped this lint completely. Now, once systems exist, it scans the flows.
- crates/aegis-model/src/project.rs:5745 — the trust-boundary warning wins when a flow is both. That keeps one finding per flow, as the prompt asked.
- crates/aegis-model/src/system.rs:293 — `boundary_flow` puts the source system first. The doc test checks the order with a flow from ADAS to Brake.
Risk: low. The existing trust-boundary rule fires exactly as before. The only change on that path is the early return. Tests cover these cases: a flow in a project with no trust boundary, a flow inside one system, a project with no systems, a flow that crosses a trust boundary and links two systems (one finding, the old code), and the warning going away once a threat is added. The lint looks only at the flow's two end components, not their parents, the same as the existing rule. The builder noted this.
View: git -C factory/worktrees/120-system-overview diff 7b2bf4adde7bd57d69f5476d171732bd2eadd606..HEAD
## Findings
- none blocking. Minor: crates/aegis-client/tests/systems.rs:104 checks the quoted name and "links two systems but has no", but not the leading "Data flow" or the trailing "threat scenario." Optional: assert the full sentence.
## Lesson
none
NEXT: human
