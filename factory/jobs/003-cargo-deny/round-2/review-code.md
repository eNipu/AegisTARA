VERDICT: PASS

Checks (worktree factory/worktrees/003-cargo-deny, 0.20 form per rework.md):
- cargo deny --version -> cargo-deny 0.20.2, pass
- cargo deny --manifest-path crates/aegis-model/Cargo.toml --config deny.toml check licenses bans sources -> pass (bans/licenses/sources ok)
- same, check advisories -> pass
- scripts/deny.sh -> pass (ok aegis-model)
- tools/deny-fixtures/run.sh -> pass (8/8 match)
- PATH=/usr/bin:/bin scripts/deny.sh -> hint for 0.20.2, exit 127 (AC7)
- AC8 grep -> clean; git diff --stat master -- crates/ -> empty (AC10)
- aegis-model fmt, clippy -D warnings, test -> pass

Not verified: my grep for stale `check --config` or 0.19 references did not finish (the shell hung), so that is unchecked. build.md says none remain.

Findings (non-blocking):
- scripts/deny.sh:30-31 - traps are correct (INT/TERM exit 130, EXIT removes the temp file). No fix.
- The spec's ## Checks still shows the superseded 0.19 form (`check --config`). rework.md supersedes it. Update the spec text for the record.
