VERDICT: PASS
## For the owner
What changed: Rounds 1-2 added sorted/unique/16-capped `responsible: Vec<PartyIdx>` to goals and claims via four typed link/unlink ops with exact inverses, a `RemoveParty` in-use guard, and made `Project::eq` resolve responsible indices to public `PartyId` (`same_goal`, `same_claim`). Round 3 (the single requested fix) changed the `diff()` claim comparison closure from derived `|x, y| x == y` to `self.same_claim(x, other, y)`, so `diff()` is now slot-layout-independent for claims just like `Project::eq`. The reversed party-insertion test was extended with an assertion that the claim diff is empty.
Look at: `crates/aegis-model/src/project.rs:5973` — the `diff()` claim closure now calls `same_claim` (the round-3 fix; mirrors the goal arm); `crates/aegis-model/src/project.rs:11424` — `same_claim` resolves each responsible `PartyIdx` to a public `PartyId` via `party_key`, the helper the fix depends on; `crates/aegis-model/tests/parties.rs:1033` — the slot-order test's new `a.diff(&b)...cybersecurity_claims().is_empty()` assertion.
Risk: none. The round-3 fix is present and its test would fail if removed: the pair inserts the same public parties in opposite arena orders, so the stored responsible `PartyIdx` slots are `0,1` vs `1,0`; derived `PartialEq` would mark the claim modified (non-empty diff), while `same_claim` resolves both to the same public IDs and yields an empty diff. The test only asserts the claims diff, so party-slot differences elsewhere do not mask it. Gate PASS at 507 counted lines (cap 540); no untrusted name in error text (IDs only), cap checked before mutation, no new dependency, no snapshot/envelope/risk change. No already-accepted decision was re-opened.
View: git -C factory/worktrees/145-parties diff 346c4b58a548766793284af2e25f9e7d7d8e7d9b..HEAD
## Findings
none
## Lesson
[builder] When adding a reference-typed field (`Vec<PartyIdx>`) to a goal/claim, wire its content comparator into every comparison path — `Project::eq` and each `diff_arena` closure in `Project::diff` — so a project equal under `==` cannot produce a spurious `diff()`.
NEXT: human
