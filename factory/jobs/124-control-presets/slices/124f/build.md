Commit: ee4a6ca Add preset settings, refuse a wrong fit, reset to the pack's values (124f)
Files: crates/aegis-client/src/commands.rs, crates/aegis-client/tests/control_presets.rs
Done when:
1. Preset created, named, ruled, applied in one undo step: `cargo test --locked -p aegis-client --test control_presets` (7 passed)
2. Wrong fits refused with exact sentences, project unchanged: same command, passed
3. UsePackValues restores the preset in one undo step: same command, passed
4. Shell builds: `scripts/ci.sh crates/aegis-client crates/aegis-shell` all steps passed
5. Changed lines in src: 169 added, 5 deleted (cap 250)
Notes: `cargo fmt --all` run. Clears come before sets in UsePackValues (checked by reading ops). No existing shell code needed changes.
NEXT: reviewer
