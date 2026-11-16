VERDICT: PASS
## For the owner
What changed: Four new edits let the app add, rename and remove a system, and move components into or out of one, each as one undo step.
Removing a system first takes its components out (and clears its settings), so undo brings everything back; giving a component a parent or deleting it also takes it out of its system first.
Undo labels now read "System 'Backend' removed and 2 components now in no system" and "move 2 components".
Look at: crates/aegis-client/src/commands.rs:1386 — when a component in a system gets a parent, the batch now starts with a "take out of system" op, so its undo label becomes "move component 'A'" instead of "change parent of component 'A'". Not asked for and not wrong, but you may prefer the old wording.
crates/aegis-client/src/commands.rs:1417 — RemoveSystem clears the system's attributes before removing it. The prompt asked for this, but no test covers a system that has attributes.
crates/aegis-client/src/undo_label.rs:175 — any edit made only of two or more system moves gets the bulk label "move N components", which also raises the toast in session.rs.
Risk: low. Undo is checked against the project before the edit (`==`) for add, rename, remove, move, set parent and remove component. Unknown IDs and misuse-case IDs give `Missing`. Children and components already in the target system are skipped, and a repeated ID is moved only once. Other element kinds keep their old "change project" label.
View: git -C factory/worktrees/119-systems diff 4d0e2d7235985ca7cc0d6a1cc0f09e26f1d89779..HEAD
## Findings
- none blocking. Optional: add a test that removes a system with an attribute and checks that undo restores it (crates/aegis-client/tests/systems.rs).
## Lesson
[planner] When an existing edit gets a new op in front of it, say which undo label it should keep, because describe() names an edit after its first op when it has no removal or addition.
NEXT: human
