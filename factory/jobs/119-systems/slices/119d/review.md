VERDICT: PASS
## For the owner
What changed: once a project has at least one system, the checker warns about every top-level component that is in no system.
A project with no systems gets no such warning, and child components are never flagged (they belong to their parent's system).
The Problems page shows the warning as "Component "<name>" is in no system." and clicking it opens that component.
Look at: crates/aegis-model/src/project.rs:5702 — the rule itself: skipped when there are no systems, otherwise only components with no parent and no system are flagged.
crates/aegis-model/tests/systems.rs:376-401 — the tests: none without systems, top-level ones flagged, children never flagged, and the warning goes away when a system is set and comes back when it is cleared.
crates/aegis-client/src/pages.rs:2638 — the sentence on the Problems page and where a click goes.
Risk: none. It is a warning, not an error, so it blocks nothing. It reads only data that is already in the project (no new input). Results come out in a fixed order: components are walked in ID order and the findings are sorted.
View: git -C factory/worktrees/119-systems diff 4a62bfc366010e5482419fed80f19738a5677226..HEAD
## Findings
- none. Both "Done when" items are met and the tests would fail without the change. If the early return for "no systems" were removed, `no_finding_without_systems` would fail. If the rule were removed, both the model test and the client test (which checks for exactly one row naming "Secret Gateway") would fail. If children were reported, the expected list `[10, 12]` would no longer match. ops_properties.rs was not touched and no count changed, as build.md says.
## Lesson
none
NEXT: human
