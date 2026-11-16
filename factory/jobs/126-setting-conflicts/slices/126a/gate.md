GATE: PASS
Slice: 126a  Base: 0182a0577c5ed1523da439ed45118c0e48e7ef83


Diff: 220 counted lines, 13 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-model --test control_kinds --test setting_conflicts --test pack_limits
ok   cargo test --locked -p aegis-model --test control_presets --test control_settings --test builtin_pack --test stride_pack
ok   cargo test --locked -p aegis-client --test pages
