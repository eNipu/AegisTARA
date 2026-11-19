GATE: PASS
Slice: 208d2  Base: bb6d926761d50f9167c234ca799ad8b504be8eb3


Diff: 235 counted lines, 11 files
ok   scripts/ci.sh crates/aegis-client crates/aegis-workbench
ok   cargo test --locked -p aegis-workbench --test start_local
ok   cargo test --locked -p aegis-workbench --test folder_view
ok   cargo test --locked -p aegis-workbench --test command_wiring
ok   cargo test --locked -p aegis-client --test command_table
