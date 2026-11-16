Commit: 364578c Show boundary flows on the item definition rows (120b)
Files: crates/aegis-client/src/combined.rs, src/lib.rs, src/pages.rs, src/systems.rs, tests/combined.rs
Done when:
1. `cargo test --locked -p aegis-client --test combined` - 5 passed
2. `cargo test --locked -p aegis-client --test systems --test pages` - 22 and 13 passed
3. `cargo check --locked -p aegis-shell` - ok
Also: `cargo fmt --all --check` ok; `scripts/ci.sh crates/aegis-client` all steps passed.
Notes: Commit already existed in the worktree when this run started; I verified it and did not change it. Non-test source lines changed: 211 (cap 240).
NEXT: reviewer
