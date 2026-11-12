GATE: PASS
Slice: 102d  Base: 60b18b3ce2b211a0c2bd7b37bc96c2a7b949cb62


Diff: 0 counted lines, 1 files
ok   scripts/ci.sh crates/aegis-license
ok   sh scripts/check-security-pack.sh
ok   sh -c "test $(grep -c '^| Q-' docs/security-pack/en/06-security-questionnaire.md) -ge 40"
