VERDICT: PASS
## For the owner
What changed: each risk row now lists the settings that lower the step deciding its risk after controls (or names that step when nothing lowers it) and says whether the risk went down.
Each control row now counts how many risks it lowers. Three small text helpers make the table cell ("Secure boot +1", "— (Inject on CAN)"), the full list for reports, and "Lowers N risks".
The threat detail lines (`ceiling_lines`) now use the model's `deciding_step` too, so the rule is defined in one place. Their text is unchanged.
Look at: crates/aegis-client/src/pages.rs:1494-1557 (`reduction`): the one place that decides what "reduced by" and "lowered" mean.
crates/aegis-client/src/pages.rs:1840-1848 (`control_rows`): a control only counts if the risk really went down and the control is listed on the deciding step.
crates/aegis-client/src/pages.rs:1263-1308 (`ceiling_lines`): the rewrite. It was checked by comparing it with the model's `deciding_step` (same filter: ceilings below the rating), and the 22 existing page tests still pass.
Risk: low. `lowered` is false whenever there is no deciding step (no tree, or an evaluation that does not fit the tree), even before the risk levels are compared. Today controls only lower risk through tree steps, so this changes nothing. The control count only uses `lowered` together with entries, which are empty in that case anyway. Each step of the prompt is covered by a test that would fail without the change: the dedup test really produces two ceilings for one control (component and interface, see `step_ceilings`); the lowers test expects 1/1/0; and the first test checks order, `on`, ceiling labels, tree and node ids, and that `lowered` is true. No new dependency. The HashMap is keyed by ControlId and only used for lookups, so the output does not depend on its order.
View: git -C factory/worktrees/125-reduced-by diff 6fcc1fe9a5bebbd793c7bdae60f083c2e39b4ce3..HEAD
## Findings
- none
## Lesson
none
NEXT: human
