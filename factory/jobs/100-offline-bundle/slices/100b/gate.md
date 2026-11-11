GATE: PASS
Slice: 100b  Base: 7251c9a9050f1c442e57ef95ab885959a711a05d


Diff: 362 counted lines, 6 files
ok   scripts/ci.sh crates/aegis-license
ok   cargo test --locked -p aegis-license --features issuer --test bundle_cli
ok   scripts/offline-bundle-test.sh
