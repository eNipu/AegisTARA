## Rework 102a

Owner: fix again. Fix the three items in `slices/102a/review.md` (second review): the listen-address statement in chapter 1 (all interfaces in the container; the compose file publishes on loopback; a `Customer` duty; add `deploy/compose.yaml` to 1.9), and the two test cases that do not prove their rule (heading gap, ja section removed).

## Orchestrator note 102c

Chapter 3 and chapter 5 disagree on identity-provider groups (second review item "Owner decision needed" in `slices/102c/review.md`). Decision, by the spec's truth rule: the code decides. In this slice also correct `docs/security-pack/en/03-authentication-and-access.md` (the lines that say project membership works for a group from the identity provider): company login fills no groups in this release (`crates/aegis-server/src/auth_layer.rs:79`), so membership by identity-provider group is `Planned`, and membership is by user name. Keep chapter 5 PD-08 as it is.

## Orchestrator note 102c scope

The gate failed 102c as out of scope because the note above had the builder correct `docs/security-pack/en/03-authentication-and-access.md`, which the plan does not allow in 102c. Planner: add that one file to slice 102c's allowed files in `plan.json` (and a line in `prompts/102c.md` saying why). Change nothing else.

## Rework 102c

Owner: fix again. Fix the item in `slices/102c/review.md` (second review): the event log claim "never holds a password" in chapter 4 (and LG-01, PD-04). Say the log never stores the password field or a token, and add one line in 4.3 and in PD-04 that the typed user name is recorded as typed on a failed login, so a password typed there by mistake is kept until retention removes it.
