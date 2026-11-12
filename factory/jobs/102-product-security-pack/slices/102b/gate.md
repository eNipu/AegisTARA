GATE: PASS
Slice: 102b  Base: 91a35b38013631f8359af1875a59d51185ca102f


Diff: 10 counted lines, 4 files
ok   scripts/ci.sh crates/aegis-server
ok   sh scripts/check-security-pack.sh
ok   sh -c "! grep -n 'does not check the database certificate' docs/deployment.md"
ok   sh -c "! grep -n 'without checking the certificate' crates/aegis-server/src/store/postgres.rs"
