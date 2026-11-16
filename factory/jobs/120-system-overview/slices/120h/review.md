VERDICT: PASS
## For the owner
What changed: after an undo or redo, if the changed element sits in a system the user is not looking at, that system is added to the view.
The toast then ends with " View now includes <system>." so the user sees why the view grew.
The view change stays local to the screen: nothing is sent to the server and no history entry is added.
Look at:
- crates/aegis-client/src/session.rs:3039-3047 — the only behaviour change. The view grows before the toast is posted, and selection and page work as before.
- crates/aegis-client/src/systems.rs:355 — `reveal`: returns nothing when the view is "all", when the element is in no system, when one of its systems is already shown, or when the element is gone. Otherwise it adds the element's first system in ID order.
- crates/aegis-client/tests/session.rs:1775 — the end-to-end test: undo and redo both grow the view from "Brake" to "Brake + ADAS", switching back adds no op, and an undo inside the view leaves the toast and chip unchanged.
Risk: low. The view is display state only. The change adds no op, no server call and nothing to the settings file. Both tests fail without the change (toast suffix and chip). The pure test checks the two-system case by comparing IDs (`adas.min(gateway)`), not names, so it holds in either ID order.
View: git -C factory/worktrees/120-system-overview diff 5e49ce1abf2388bc9812c02f64cfe25b4a6f8b40..HEAD
## Findings
- none blocking.
- Note only: crates/aegis-client/tests/session.rs:1886 checks that `confirmed_seq` has not changed without first waiting for `project_ready`. A stray op could reach the server after the check and the test would still pass. The existing test it copies has the same pattern, and `reveal` cannot send an op because it is a pure function, so this needs no change now.
## Lesson
[reviewer] A "no op was added" check on `confirmed_seq` must first wait for the project to settle (`project_ready`), or a late op slips past the check.
NEXT: human
