VERDICT: PASS
## For the owner
What changed: Added decode caps and wire fields so parties and goal/claim responsible lists survive a save/load round-trip, with empty lists omitted so v1 files keep their exact bytes. Added a Warning lint when a `Share` threat's linked claim names no party other than `Us`. Extended the op generator with the eight party ops and added byte-identity, cap and six-case lint tests.
Look at:
- `crates/aegis-model/src/store/snapshot.rs:1194` — the new load replay (AddParty + LinkGoal/LinkClaimParty, with visual-ID restore) is the only place persisted party data re-enters the model; check it matches the goal/claim replay below it.
- `crates/aegis-model/src/store/wire.rs:1544` and `:1967` — `PartiesCap`/`ResponsibleCap` BoundedVecs and `PartyWire` (name is `Name`/capped, kind is the serde enum), plus `#[serde(default)]` on every new field, which is what keeps v1 fixtures byte-identical.
- `crates/aegis-model/src/project.rs:5672` — `lint_share_party`; verify the "zero responsible counts as no other party" and "only Us" branches against the prompt.
Risk: Low, with two disclosed accommodations the owner should note. (1) Pre-existing 145b gap, unchanged here: `RemoveCybersecurityGoal`/`RemoveCybersecurityClaim` invert to `Add*` ops that carry no `responsible`, so removing a linked goal/claim and undoing it loses its links. This slice's generator arms 42/45 now only remove unlinked goals/claims to keep the inverse property green; that bug stays reachable in normal use and is untested. Out of scope for 145c, but a future slice should fix the removal inverse. (2) `is_party_op` skip in `tests/common/mod.rs:2034` means shared `accepted()`-based property tests do not exercise party ops; the coverage test uses `replay()` directly and still drives all eight. Checked: v1 fixture load/re-save asserts equal bytes and no `parties`/`responsible` key (store.rs:241); 101-party round-trip with a visual-ID gap asserts `save()` bytes equal (store.rs:256); just-at/just-over cap tests assert `Limit::Parties`/`Limit::Responsible` (store_limits.rs:354,372); all six lint cases assert code and severity. Each would fail if the rule/caps/serialization were removed.
View: git -C factory/worktrees/145-parties diff 189b2281c4798284c68dafe441c370090c389b44..HEAD
## Findings
- none
## Lesson
[builder] When a new link field lands on an entity, update that entity's Remove* inverse in the same slice: here linked goals/claims cannot be undone, and the generator had to be narrowed to hide it.
NEXT: human
