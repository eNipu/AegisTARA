GATE: PASS
Slice: 141d  Base: abbb9686a900f27033ff53502e93f63141d907c3


Diff: 282 counted lines, 10 files
ok   scripts/ci.sh crates/aegis-model
ok   cargo test --locked -p aegis-model --test store --test workspace --test store_limits --test shared_steps
