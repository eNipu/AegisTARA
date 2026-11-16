Commit: 11736f0 Add the system view, its menu, palette entries and remembered setting (119g)
Files: crates/aegis-client/src/systems.rs, crates/aegis-client/src/settings.rs, crates/aegis-client/tests/systems_view.rs
Done when:
1. cargo test --locked -p aegis-client --test systems_view: 4 passed
2. cargo test --locked -p aegis-client --lib settings: 2 passed
3. cargo check --locked -p aegis-shell: ok
Also: cargo fmt --all, scripts/ci.sh crates/aegis-client: all passed.
Notes: 258 non-test lines changed (cap 300). view_commands normalizes the view first (not stated in the prompt). Tests rely on UUIDv7 IDs sorting in creation order.
NEXT: reviewer
