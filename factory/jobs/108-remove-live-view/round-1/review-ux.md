# UX Review — 108-remove-live-view (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## UX Evaluation
- Read-only reviewers are never surprised by tables or attack trees shifting underneath their cursor mid-review; instead, a calm `"A newer version is available"` notice with a `Reload` button lets them refresh when ready.
- Clicking `Edit` on an older view automatically reloads the latest committed version before acquiring the lease.
