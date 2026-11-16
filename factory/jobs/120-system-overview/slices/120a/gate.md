GATE: PASS
Slice: 120a  Base: 7b2bf4adde7bd57d69f5476d171732bd2eadd606


Diff: 88 counted lines, 6 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test systems --test ops_properties --test project
ok   cargo test --locked -p aegis-client --test systems --test pages
