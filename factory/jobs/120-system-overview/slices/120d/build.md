Commit: 2d3cc15 Add per-system rows to the overview that add up (120d)
Files: crates/aegis-client/src/combined.rs, crates/aegis-client/src/pages.rs, crates/aegis-client/tests/combined.rs, crates/aegis-client/tests/combined_properties.rs
Done when:
1. cargo test --locked -p aegis-client --test combined --test combined_properties: ok (15 + 1 tests)
2. cargo test --locked -p aegis-client --test pages: ok (22)
3. cargo check --locked -p aegis-shell: ok
Before commit: cargo fmt --all, scripts/ci.sh crates/aegis-client (fmt, clippy, test ok).
Notes:
- 168 source lines changed outside tests (cap 280).
- Unrated threats make evaluate_residual_risk return Err(NoAttackTree); both the code and tests treat that as "not rated".
- The "No system" row is added only when something is in it; with a subset view, elements of systems outside the view are in no row.
- Property generator is pub fn spec()/build() in tests/combined_properties.rs for reuse.
NEXT: reviewer
