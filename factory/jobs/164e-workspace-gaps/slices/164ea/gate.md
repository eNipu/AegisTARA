GATE: PASS
Slice: 164ea  Base: 441912550ffdc8e41a32a06c2c893a3389a9f7dc


Diff: 52 counted lines, 8 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-workbench
ok   cargo test --locked -p aegis-model --test workspace
ok   cargo test --locked -p aegis-client --test folder
ok   cargo test --locked -p aegis-client --test folder_recovery
ok   cargo test --locked -p aegis-workbench --test folder_view
