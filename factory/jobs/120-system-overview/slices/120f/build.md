Commit: c2202b5 List the easiest attack paths and choke points of the view (120f)
Files: crates/aegis-client/src/top_paths.rs (new, ~200 lines), crates/aegis-client/src/lib.rs, crates/aegis-client/src/pages.rs, crates/aegis-client/tests/top_paths.rs (new)
Done when:
1. `cargo test --locked -p aegis-client --test top_paths` - 8 passed
2. `cargo test --locked -p aegis-client --test combined --test pages` - 19 and 22 passed
3. `cargo check --locked -p aegis-shell` - finished
Before commit: `cargo fmt --all`, `scripts/ci.sh crates/aegis-client` all passed.
Notes:
- Non-test change is about 200 lines in top_paths.rs plus 8 in other files (cap 280).
- `feasibility_label` was not used (FeasibilityLevel is Copy; label via `m.feasibility_level_label`), so it stays private.
- Gate nesting in the test outline uses an undashed `OR Inner` line, as the outline grammar requires.
NEXT: reviewer
