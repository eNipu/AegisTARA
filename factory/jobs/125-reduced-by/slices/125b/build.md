Commit: f859af5 Show what reduces each risk and how many risks each control lowers (125b)
Files: crates/aegis-client/src/pages.rs, crates/aegis-client/tests/reduced_by.rs
Done when:
1. cargo test --locked -p aegis-client --test reduced_by: 7 passed
2. cargo test --locked -p aegis-client --test pages: 22 passed
3. cargo check --locked -p aegis-shell: ok
Also: cargo fmt --all, scripts/ci.sh crates/aegis-client: all passed.
Notes: pages.rs changed 169 added / 17 removed lines (tests excluded). ceiling_lines now uses project.deciding_step; text unchanged. Test fixture gives the asset every property because category 0 needs a matching property on the asset.
NEXT: reviewer
