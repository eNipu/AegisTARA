Commit: 5e49ce1 Limit the Versions comparison to the system view (120g)
Files: crates/aegis-client/src/pages.rs, crates/aegis-client/src/session.rs, crates/aegis-client/tests/combined.rs
Done when:
1. `cargo test --locked -p aegis-client --test combined` - 27 passed (8 new, module `versions`)
2. `cargo test --locked -p aegis-client --test systems --test pages --test session` - all passed unchanged
3. `cargo check --locked -p aegis-shell` - ok
Also: `cargo fmt --all`, `scripts/ci.sh crates/aegis-client` - all steps passed.
Notes:
- Non-test source change is about 66 lines net (cap 220). The `entity!` macro now takes a Selection constructor instead of a cite closure, which shortened the call sites; output text and order are unchanged.
- `push` takes a third `show` argument; hidden lines are counted instead of stored. `system_use` is only computed when the view is not all.
- Risk test uses threats without attack trees, because a tree gives the threat its feasibility and a direct rating would not move the risk.
- Elements in no system always show (empty set), so the "nothing shown" test adds only ADAS elements after base.
NEXT: reviewer
