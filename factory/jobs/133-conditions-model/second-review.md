VERDICT: CHANGES (fixed and finished by Claude on factory/133-conditions-model; merged with review/post-129-fixes)
Reviewed: 133a (007c564) and 133b (958b5f7) built by hy4; 133c and 133d built in this review.
CI: scripts/ci.sh green on the merged branch (fmt, clippy, tests, 8 crates).

## 133a: fine
The Condition element, CF visual IDs, the four element ops and their exact inverses, op log, diff.
One gap: `MAX_CONDITIONS_PER_ELEMENT` was not exported, which AC1 requires. Fixed.

## 133b: CHANGES (undo data loss, the same fault as jobs 145 and 146)
1. None of the seven removal guards (asset, component, interface, threat, tree, goal, claim) looked at condition links. Removing an element that needs a condition left a dangling side-table entry, and undo (`Add…`) could not restore the link. Now refused with `ElementNeedsConditions`.
2. Removing an attack step ignored the step's conditions (`NodeIsRated` now counts them).
3. `linked_steps()` ignored step conditions, so removing a tree lost them (`AttackTreeHasStepLinks` now counts them).
4. The gate failed on size (667 lines against 600). It was reviewed in full instead; nothing in it is padding.

## 133c: built here
`conditions`, `condition_links` and `step_conditions` are written only when non-empty; the v1 fixture keeps its bytes. Decode caps `Limit::Conditions` and `Limit::ConditionLinks` have at-cap and over-cap tests. A round trip has 102 conditions with a visual-ID gap and links on every kind plus a step. A link to an unknown condition is refused.
The random op generator covers all eight ops. Its arm 73 was a duplicate (unreachable), and unneeds never succeeded until the generator seeds links first. Coverage is now 72 and 69 op kinds.

Also found: the workspace file format had no place for parties (job 145), and the 145 builder made the property test skip party ops to hide it. Parties and conditions now go in `shared/parties.json` and `shared/conditions.json`, and the skip is gone.

## 133d: built here
`Selection::Condition`; eight relation rows (seven element kinds and steps) that share one read and one link function; the edits `SetConditionLink`, `SetStepConditionLink` and `RemoveCondition` (one undo step); undo labels; en, ja and pseudo keys.
Beyond the prompt: every removal edit now unneeds conditions first (through the shared pre-removal hook), so deleting an asset, step or tree that needs a condition is still one undo step. Tested for all four cases.

## Edge cases checked
Conditions only on steps (`NotAStep`); a step-to-gate change goes through node removal, which is guarded; the model invariants now check condition links (live element, live condition, sorted, unique, slot length, none on a gate).

## Lesson
For every new link or field on an element, check each removal and unlink inverse and each storage format (snapshot, op log, workspace files), not only the new op's inverse. Jobs 145, 146 and 133 all missed this.
