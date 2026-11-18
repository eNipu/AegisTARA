# Build 182b2

Commit: c886292 Add explorer marks, filter, keyboard moves, reveal and the drop rule (182b2)
Files:
- crates/aegis-client/src/explorer.rs
- crates/aegis-client/tests/explorer.rs

Done when:
1. `cargo test --locked -p aegis-client --test explorer`: 8 passed, 0 failed.
2. `git diff 88ef378 HEAD -- crates/aegis-client`: empty (0 lines). Both files were taken from 88ef378 with `git checkout 88ef378 -- ...`; `cargo fmt --all` changed nothing.
3. `git diff --numstat HEAD~1 HEAD -- crates/aegis-client/src`: 225 + 11 = 236 lines (under 320).

Before you commit: `cargo fmt --all` (no changes), `scripts/ci.sh crates/aegis-client` (fmt, clippy, test all ok), explorer tests 8 passed.

Notes:
- No other file differed from 88ef378 under crates/aegis-client (182b did not change lib.rs or main.ftl relative to it), and no fixes were needed.
- Marks: the error is T2 without feasibility (Brake has another: the attack tree root gate has no children); the warning is "Loose is in no system".
- A finding on a shared kind (damage, control, goal, claim) also counts in the Problems folder of every system it belongs to and marks that folder, so the Problems count and its dot agree.
- A finding on a claim with no systems (CL00) marks Project > Claims only and counts in no owner's Problems folder.
- With a filter, ancestors of matches show as expanded whatever the state says.
- `right` and `left` do nothing when nothing is selected.
- The filter checks the visual ID and the name separately (case-insensitive), so a match cannot span the space between them.

All criteria met.

NEXT: reviewer
