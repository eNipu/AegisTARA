VERDICT: PASS
## For the owner
What changed: The client core now gives the item definition page a Systems table, a draft row that creates a system, and the system of each component row.
The component inspector gets the list of systems to pick from, plus a read-only line for a child component ("In Brake, with its parent ECU").
The version comparison says when a component moved to another system, and lists systems that were added, removed or changed. The rework makes the "moved" check compare system IDs, not names.
Look at: crates/aegis-client/src/pages.rs:3085-3105 — the fixed "moved from" check; it now compares system IDs and only then builds the text.
crates/aegis-client/src/systems.rs:31-62 — builds the Systems table in one pass; the count includes child components, the name list only top-level ones.
crates/aegis-client/tests/systems.rs:409-453 — the two new tests (rename is not a move; a move between two systems with the same name is reported). Both would fail with the old name-based check.
Risk: low. A project with no systems shows no system data (tested). The shell still builds unchanged (gate). A child component whose parent moved gets no line of its own; only the parent's line appears, as the prompt asked.
View: git -C factory/worktrees/119-systems diff 9572b7089c66c3e7e81de39a8b7a51b14bedfe28..HEAD
## Findings
none
## Lesson
none
NEXT: human
