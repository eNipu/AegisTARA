GATE: PASS
Slice: 102f  Base: b138e57debd0da2264b6916033058cbc9d6c7ec9


Diff: 5 counted lines, 10 files
ok   scripts/ci.sh
ok   sh scripts/check-security-pack.sh --complete
ok   sh scripts/check-security-pack-test.sh
ok   sh -c "test $(grep -c '^| Q-' docs/security-pack/ja/06-security-questionnaire.md) -ge 40"
ok   sh -c "test $(grep -c 'run: scripts/check-security-pack' .github/workflows/ci.yml) -eq 2"
