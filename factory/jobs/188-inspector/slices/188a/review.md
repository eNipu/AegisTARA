VERDICT: PASS
## For the owner
What changed: Registered the `"go-to-cause"` command in `command_table`, added 30 inspector/reverse-relation/go-to-cause Fluent keys across `en`, `ja`, and `pseudo`, and introduced `aegis-client::inspector` with toolkit-free inspector view types, `reverse_relation_label`, `exclusion_causes`, `explain_selection`, and `primary_cause`.
Look at:
- `crates/aegis-client/src/inspector.rs:215-448` — `push_tree_deciding_step`, `explain_threat`, `explain_tree`, `explain_tree_node`, and `primary_cause` cause-target ordering across residual/inherent evaluations and exclusions.
- `crates/aegis-client/tests/inspector_view.rs:80-299` — unit tests covering Fluent keys, reverse relation labels, threats with trees, direct-feasibility threats, trees, gates, capped steps, and condition exclusions.
Risk: none
View: git -C factory/worktrees/188-inspector diff fd80e966d1cb9e5df0aba86d8dd73eb95e1d805e..HEAD
## Findings
None.
## Lesson
none
NEXT: human
