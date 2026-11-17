VERDICT: CHANGES (data loss on undo; fixed on review/post-129-fixes, 3da7075)
Reviewed: a44be6b..276be4b against spec.md and FND-08.

Fine: party ops and their inverses, guards, op log, snapshot (links are replayed through checked ops), lint, diff.

Fixed:
1. Data loss on undo. Removing a goal or claim that lists responsible parties, then undoing, brought it back without its parties: the inverse `AddCybersecurityGoal`/`AddCybersecurityClaim` has no responsible field. The model now refuses that removal (`CybersecurityGoalHasParties`/`CybersecurityClaimHasParties`), the same pattern as descriptions. `Edit::RemoveGoal`/`RemoveClaim` unlink the parties first, so the removal is still one undo step and undo restores the links. Tests cover the model and the client.
2. The Excel import refused any row whose goal or claim already listed the party, for example two threat rows sharing one goal and owner, or importing the same workbook twice. It now skips a link the simulated project already has, with a test.
3. `Project::party_id` is public like the other arenas; `relations.rs` no longer scans every party for each lookup.
