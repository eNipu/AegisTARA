Commit: e68be9f Make the whole overview follow the system view (120e)
Files: crates/aegis-client/src/pages.rs, src/systems.rs, src/session.rs, tests/combined.rs, tests/combined_properties.rs
Done when:
- Overview follows the view: cargo test --locked -p aegis-client --test combined -> 19 passed
- Risk number same in every view: --test combined_properties -> 2 passed (64 cases each)
- Existing tests: --test pages --test systems_view --test session -> all ok
- cargo check --locked -p aegis-shell -> ok
- scripts/ci.sh crates/aegis-client -> all steps passed
Notes:
- Source change about 190 lines (cap 240). `selection_systems` extracted; `filter_page` unchanged in behaviour.
- errors/warnings now come from `problems(ws)` rows (`error` flag) in both the all and filtered views, not from lint severity directly; the existing tests pass, so the two agree.
- An element in no system shows in every view (empty set), so the Brake view also counts the loose threat, control and orphan damage scenario; tests expect that.
- A threat with an attack tree is rated by the tree, so the fixture adds a tree-less "Rated attack" threat in Brake to get a rated risk.
NEXT: reviewer
