VERDICT: PASS
## For the owner
What changed: the model can now work out, on demand, which elements and attack tree nodes are switched off by a condition that is off, and give the reason for each.
A new "lifted" rating domain adds an "excluded" value to any rating scale: an OR gate skips excluded children, and an AND or sequence gate is excluded if any child is excluded.
Nothing is stored and no risk number changes yet. Slice 134b connects this to evaluation.
Look at:
- crates/aegis-model/src/applicability.rs:389-438 (`propagate_owners`): the owner rules. Components are sorted by depth so parents come before children. A tree is excluded only when every threat that uses it is excluded. Owner exclusion of a tree does not mark its nodes (the prompt did not ask for that). Check this is what 134b expects.
- crates/aegis-model/src/applicability.rs:491: if `evaluate` fails (for example on an empty gate), the tree keeps its step reasons but gets no gate reasons. The prompt asked for this, and the oracle copies it.
- crates/aegis-model/src/eval.rs:1823-1852 (`lift_or` / `lift_and`): the `Kept` answers match the table in the prompt exactly, including (Excluded, Excluded) -> Acc.
Risk: low. The new code is read-only and nothing calls it from existing evaluation yet, so current risk numbers cannot change. Checks:
- A property test compares the lifted domain with a separate memoised evaluator over random trees (up to 30 nodes, with DAG links), checking every node value and that an OR gate never selects an excluded child.
- A second property test compares `applicability()` on random projects with an oracle that walks owner chains upward. It also checks that every reason list is sorted and unique and that the ConditionOff reasons are exactly the off conditions.
- Hand-built tests cover each rule, each element kind and each edge.
View: git -C factory/worktrees/134-applicability-engine diff 04954c42c11c82229aca62fec1a5562c032992cc..HEAD
## Findings
- None blocking.
- Note: `node_excluded_at` (applicability.rs:273) carries `#[allow(dead_code)]` until 134b uses it. 134b must remove the attribute.
- Note: the property oracle compares sets of excluded elements and nodes plus ConditionOff reasons. It does not compare OwnerExcluded, ViaExcluded or AllPathsExcluded reasons; the hand-built tests check those exactly.
- Note: the `shape` and `salt` helpers (tests/applicability.rs:623, 659) skip refused ops. The generator from tests/common is used unfiltered, and `check` also runs before the helpers, so they hide nothing. The build notes say they raised coverage of owner, via and gate reasons.
## Lesson
none
NEXT: human
