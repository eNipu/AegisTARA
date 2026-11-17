VERDICT: PASS
## For the owner
What changed: switching a condition on or off no longer rebuilds the whole evaluation cache. Only the steps whose exclusion changed (and their parents) and the threats whose exclusion changed are evaluated again.
Edits that cannot change applicability (rating edits, renames and so on) no longer work out applicability again at all.
A new bench shows a condition switch at 0.52 ms and a rating edit at 0.6 us on a 100,000-node project. The budget is 5 ms.
Look at:
- crates/aegis-model/src/project.rs:13395 — `update` now relies on `may_change_applicability` to skip working out applicability. If a future op is wrongly put on the "false" side, the cache would show a stale exclusion. The match has no catch-all arm, so every new op needs an explicit choice. The coverage proptest (tests/applicability_cache.rs `cache_applicability_follows_every_op`) checks the cache's applicability after every generated op and every undo.
- crates/aegis-model/src/project.rs:14121 (`apply_applicability_change`) — the core diff. It skips the tree the op is reshaping, because that op's own arm rebuilds it. It rebuilds any tree whose own exclusion flipped or whose node count no longer matches. All other trees get index-based dirty-step updates.
- crates/aegis-model/src/applicability.rs:287 `touched_trees` — a justified departure from the prompt. A tree excluded only at tree level (all its threats excluded) has no `trees` entry. It would have been missed if only that map were read.
Risk: low. A missed recompute would leave a stale risk on screen. Three tests check this. The shuffled proptest compares the cache with a fresh full cache, and every risk with `Project::evaluate_risk`, after every switch and every undo. The fixture covers nested OR/AND/SEQ gates, steps via a component and an interface, and conditions on every element kind. `ops_properties` compares cache and project for every generated op. The builder reports that turning off the dirty-step update or the threat recompute made these tests fail. Arenas are slot-based, so removals do not shift tree or threat indices under the new code.
View: git -C factory/worktrees/134-applicability-engine diff 14d7573cdc3f9fa270608e01065d6165ea948253..HEAD
## Findings
- none
## Lesson
none
NEXT: human
