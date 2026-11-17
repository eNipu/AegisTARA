Commit: 7b3d9bc9a889b4af3b04ba29908f76cf27ab4ff7 Link responsible parties on goals and claims and guard party removal (145b)

Files:
- crates/aegis-model/src/party.rs (new MAX_RESPONSIBLE = 16, independent of MAX_GOAL_CONTROLS)
- crates/aegis-model/src/lib.rs (re-export MAX_RESPONSIBLE)
- crates/aegis-model/src/control.rs (private `responsible: Vec<PartyIdx>` on goal and claim, `responsible()` accessors)
- crates/aegis-model/src/op.rs (four link ops, inverse table, provenance match lists, fixed the stale removal-variant count doc comment)
- crates/aegis-model/src/error.rs (PartyInUse, Duplicate/GoalPartyNotLinked, Duplicate/ClaimPartyNotLinked, the two TooMany…Responsible errors, each with a doc test)
- crates/aegis-model/src/project.rs (goal/claim construction, validate/apply/commit arms, sorted insert and cap checks, RemoveParty in-use scan, eval-cache no-op arms)
- crates/aegis-model/src/store/wire.rs (four OpWire variants)
- crates/aegis-model/src/store/op_log.rs (four OpView variants and both conversions)
- crates/aegis-model/tests/parties.rs (inverses by save() bytes, per-guard errors, blank actor, sorted/unique, cap 16/17, RemoveParty guard, no risk delta, op-log round trip)
- crates/aegis-client/tests/undo_every_op.rs (four new ops exercised in walk_parties)

Done when:
- `cargo test --locked -p aegis-model --test parties` — PASS: 13 tests, 0 failed; four exact inverses by `save()` bytes, every guard with its error variant, sorted/unique order, 16/17 cap, goal/claim/both `PartyInUse` cases and post-unlink removal, no risk delta, op-log round trip.
- `cargo test --locked -p aegis-model --test store` — PASS: 18 passed, 1 ignored (fixture regenerator).
- `cargo test --locked -p aegis-client --test undo_every_op` — PASS: 2 tests, 0 failed; all declared Op variants covered including the four new ones.

Before you commit (all run from the worktree root, all passed after the final format):
- `cargo fmt --all` — PASS.
- `scripts/ci.sh crates/aegis-model crates/aegis-client` — PASS: fmt, clippy (`-D warnings`) and tests for both crates.
- the three checks above — PASS (re-run after the final format).

Notes:
- `RemoveParty` now scans every live goal and claim for the party index and returns `PartyInUse { party, goals, claims }`; the client is expected to clear links first in one undo step, so the removal inverse stays exact (L04). Inverse-table cost for `RemoveParty` updated to `O(log n + goals + claims)`.
- Sorted insert uses `binary_search_by` on `PartyId` via `party_key`, mirroring `find_goal_control`; the cap is checked before insert in both validate and commit paths. No ref-count arena was added — the removal guard is the scan.
- No snapshot fields, envelope change, risk-number change, new dependency or client-code change; `AddCybersecurityGoal`/`AddCybersecurityClaim` are untouched (L24).
- The 145a review's stale doc comment ("seven entity-removal variants") is fixed to "eight", adding `RemoveParty` to the list.
- Source diff is 480 counted lines against the slice's 480-line cap (tests excluded by the verifier); optional doc tests were trimmed to fit without `#[rustfmt::skip]` (L12/L14/L28).
- Blank-actor tests place each unlink op against an existing link so the provenance check, not the link guard, is what fires (L09/L15).

## Rework 1
Commit: 799ab20595a74b22eaeb775f8eba30929dec09a1 Resolve responsible party links by public id in project equality (145b)

Review findings fixed:
- `same_goal` now compares the goal's `responsible` list resolved to public `PartyId`, mirroring the `control_key` resolution used for `controls`, so two projects differing only in a goal's responsible set compare unequal. Added `project_eq_sees_goal_responsible_difference`.
- Claims are no longer compared by derived `PartialEq` over raw `PartyIdx`: added a `same_claim` helper that resolves each responsible `PartyIdx` to `PartyId` (also comparing name/provenance) and used it in `Project::eq`. Added `project_eq_ignores_party_insertion_order_for_claims`, where the same public parties/claim are inserted and linked in opposite orders (different arena slots, same public IDs).

Regression proof: with the two source fixes reverted (`git checkout` of `project.rs`) both new tests fail (`assert!(without != with)` and `assert!(a == b)`), then pass with the fixes restored.

Done when (re-run after the fix):
- `cargo test --locked -p aegis-model --test parties` — PASS: 15 passed, 0 failed (13 original + 2 new).
- `cargo test --locked -p aegis-model --test store` — PASS: 18 passed, 1 ignored.
- `cargo test --locked -p aegis-client --test undo_every_op` — PASS: 2 passed, 0 failed.

Before you commit (re-run after the fix, all passed):
- `cargo fmt --all` and `cargo fmt --all --check` — PASS.
- `scripts/ci.sh crates/aegis-model crates/aegis-client` — PASS (fmt, clippy, test for both crates).

Notes: no new dependency, no snapshot/envelope/risk change. Source change is confined to `same_goal`, a new `same_claim` helper, and the claim arm of `Project::eq` in `project.rs`; tests are in `tests/parties.rs`. New commit added, nothing amended.

NEXT: reviewer

## Rework 2
Commit: 189b2281c4798284c68dafe441c370090c389b44 Wire same_claim into Project::diff claim comparison (145b)

Review finding fixed (the single remaining defect):
- `crates/aegis-model/src/project.rs` — `diff()` compared claims with derived `PartialEq` (`|x, y| x == y`), so the new `responsible: Vec<PartyIdx>` was compared by raw arena slot index. Changed the closure to `|x, y| self.same_claim(x, other, y)`, mirroring the goal arm, so `diff()` is slot-layout-independent like `Project::eq`.
- `crates/aegis-model/tests/parties.rs` — extended `project_eq_ignores_party_insertion_order_for_claims` with `assert!(a.diff(&b).expect("diff").cybersecurity_claims().is_empty())`: the reversed party-insertion pair (equal under `Project::eq`) now yields an empty claim diff too.

Before you commit (re-run after the fix, all passed):
- `cargo fmt --all` — PASS (with `CARGO_TARGET_DIR` pointed at the shared target).
- `scripts/ci.sh` — PASS (workspace: fmt, clippy `-D warnings`, tests; 8 crates, all steps passed).
- `cargo test --locked -p aegis-model --test parties` — PASS: 15 passed, 0 failed.

Notes: source change is one closure in `Project::diff` plus one test assertion; no new dependency, no snapshot/envelope/risk change. New commit added, nothing amended.

NEXT: reviewer
