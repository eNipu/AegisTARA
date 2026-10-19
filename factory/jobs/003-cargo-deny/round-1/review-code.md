VERDICT: PASS

Checks (run in factory/worktrees/003-cargo-deny):
- cargo deny --version -> cargo-deny 0.19.9, pass
- check licenses bans sources -> pass (only expected duplicate warnings)
- check advisories -> pass
- scripts/deny.sh -> pass (ok aegis-model)
- tools/deny-fixtures/run.sh -> pass (8/8)
- aegis-model fmt, clippy -D warnings, test -> pass
- git diff --stat master -- crates/ -> empty (AC10)
- AC8 grep -> clean; AC7 missing cargo-deny -> hint, exit 127
- AC1: allow list is exactly the 11 D5 IDs.

Findings (non-blocking):
- deny.toml:28 and :55 - two additions beyond the sketch (unused-allowed-license = "allow", multiple-versions-include-dev = true) - both are justified in build.md and in comments; approver should confirm. No fix needed.
- build.md - pinned 0.19.9 instead of 0.20.x because 0.20 moved --config to a top-level option and would break the spec's commands. Reasonable, documented in the ADR; approver should confirm.
- run.sh classifies a failure only when error[rejected] appears, so a broken config cannot pass as a correct rejection. Good.
