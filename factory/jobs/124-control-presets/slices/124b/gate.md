GATE: PASS
Slice: 124b  Base: b0d395330def3553f92c8d032f9649dfaaeb25bc


Diff: 97 counted lines, 5 files
ok   scripts/ci.sh crates/aegis-model
ok   cargo test --locked -p aegis-model --test control_kinds --test pack --test pack_limits --test pack_properties
