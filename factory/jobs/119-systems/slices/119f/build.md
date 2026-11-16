Commit: ec38680 Give the item definition page system rows, a picker and comparison lines (119f)
Files: crates/aegis-client/src/{systems.rs,lib.rs,pages.rs,view.rs,state.rs,session.rs}, crates/aegis-client/tests/{systems.rs,session.rs}
Done when:
1. cargo test --locked -p aegis-client --test systems --test pages: ok (10 and 22 passed)
2. cargo test --locked -p aegis-client --test session: ok (22 passed, incl. a_system_is_created_from_a_draft_and_undone)
3. cargo check --locked -p aegis-shell: ok (no exhaustive DraftKind match there)
Also: cargo fmt --all, scripts/ci.sh crates/aegis-client: ok.
Notes: Non-test source diff about 125 changed lines plus the 83-line systems.rs, under the 320 cap. tests/systems.rs already existed (119d/119e); new tests were appended. The "moved from" lines come right after the component lines; system lines come after the trust boundary lines.
NEXT: reviewer
## Rework 1
Commit: 6e06e7d Compare system IDs, not names, when reporting a component move (119f)
Fixed: the "moved from" check in pages.rs now compares the system IDs (component_system of base vs current) and builds the text only when they differ. Added two tests in tests/systems.rs: renaming a component and its system gives no "moved from" line; a move between two systems with the same name is reported.
Done when: scripts/ci.sh crates/aegis-client ok; systems/pages/session tests ok; cargo check -p aegis-shell ok.
NEXT: reviewer
