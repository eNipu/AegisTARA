VERDICT: PASS
## For the owner
What changed: a new read-only calculation, `Project::system_use()`, works out which systems every element belongs to or is used by, starting from the system set on top-level components.
It returns a small 64-bit set per element, which the client can filter on ("show Brake", "show Brake and Gateway", "show all"). Nothing is stored and no risk number changes.
Also new: a benchmark (24 µs median on the typical project with 64 systems; the limit is 5 ms) and a test file.
Look at: crates/aegis-model/src/system.rs:318-346. A control gets its systems in three ways: what it is applied to, the attack trees where it mitigates a step, and the goals that list it. The code is correct, but no test checks the tree or goal ways (see Risk).
crates/aegis-model/src/system.rs:242-251. `component_system` walks up the parents at most MAX_COMPONENT_DEPTH + 1 steps and returns None on a broken link instead of panicking. Good.
crates/aegis-model/tests/systems_properties.rs:185-242. Removing a system after unsetting its components changes only those components and no risk, and undo gives back the same project.
Risk: low. The calculation is read-only and deterministic: it uses Vecs indexed by arena index, has no hash map and does not depend on iteration-order quirks. `set_of` uses binary search on `elements(System)`, which yields systems in ID order. The random properties check the component rule against `component_system`, the at-most-one-system rule, the view algebra and the empty-set rule. Only the three plain tests check the other rules: threat, tree, damage, and control-by-target. Nothing tests an interface, data flow, trust boundary, goal or claim, or a control reached through a step mitigation or a goal. I read those paths and they follow the prompt. A later regression there would not be caught. This is not a defect against the prompt, which listed exactly these tests.
View: git -C factory/worktrees/119-systems diff 704a17f0f8b5d8d291cbcb313b7f602d00dc8f10..HEAD
## Findings
- none blocking.
- (optional, follow-up) crates/aegis-model/tests/systems_properties.rs:58 — `check` does not compare the interface, data-flow, trust-boundary, tree, damage, goal, claim and control sets with a direct recomputation from the links. Add a slow reference version (for each element, union over its links) and assert it equals `system_use()` for every kind.
## Lesson
[planner] When a slice derives values by several rules, ask for a property that checks every rule against a naive reference calculation, not only invariants such as "at most one". Otherwise the rules with fan-in (controls via mitigations or goals) go untested.
NEXT: human
