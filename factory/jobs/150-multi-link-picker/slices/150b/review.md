VERDICT: PASS
## For the owner
What changed: Adds one new headless integration test file (445 lines, 7 tests) for the 150a picker: pick/unpick one-undo-step checks for threat damages and goal controls, chip removal, Escape-close filter, a 200-item window, and IME preedit behaviour.
What changed: No source, dependency, snapshot, op-log, model or UI code touched; only the new test file exists in this slice (gate counted 0 source lines).
What changed: All seven tests pass and the script gate is green; each test was checked for whether it fails if its behaviour is removed.
Look at: crates/aegis-client/tests/multi_link_picker.rs:156,218 (and :251,302) — the "one undo step" proof: `ops.len()==1` plus `Project == before` plus `!can_undo()`. Verified `Project::eq` does compare threat `damages` and goal `controls` (project.rs:11033-11060), so undo restoration is a real assertion, and the fixtures rebase onto a clean `Workspace` so `can_undo()` is genuinely false at the end.
Look at: crates/aegis-client/tests/multi_link_picker.rs:400-432 — IME test: `set_composition("デ")` leaves 200 visible, commit narrows to "デ", and a preedit added on top of a committed "漏" leaves `visible()` unchanged before the second commit. This would fail if preedit refiltered (the L05 "right condition" check).
Look at: crates/aegis-client/tests/multi_link_picker.rs:347-357 — Escape-close: meaningful because `set_filter` does not clear composition (picker.rs:181-184), so `close()` is the only thing that can make `composition().is_empty()` pass.
Risk: none — tests-only; the undo and IME assertions were traced against `Project::eq`, `Workspace` undo semantics and the picker's `set_filter`/`commit`/`refilter` implementation and shown to fail if the covered behaviour regresses.
View: git -C factory/worktrees/150-multi-link-picker diff 8ed03a8e97b04bea632607135db18ef5f027a293..HEAD
## Findings
- none
## Lesson
none
NEXT: human
