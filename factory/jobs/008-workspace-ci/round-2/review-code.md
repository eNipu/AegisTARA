VERDICT: PASS

Reviewed `factory/worktrees/008-workspace-ci` at d0a85cd (round 1 was cb8d4ce) against base 7fc0912. Spec `## Checks` re-run from that worktree. Rework 1 (replace `Swatinem/rust-cache` with `actions/cache`) read against `rework.md`.

## Check results

| Command | Result |
|---|---|
| `actionlint .github/workflows/ci.yml` | pass (no output, exit 0) |
| `scripts/ci.sh` | pass (ok fmt, clippy, test for aegis-model; "all steps passed in 1 crate(s)"; 21 s) |
| `scripts/deny.sh` | pass (exit 0, advisories included) |
| `tools/deny-fixtures/run.sh` | pass ("all 8 fixture cases match") |
| `crates/aegis-model`: `cargo fmt --check` | pass |
| `crates/aegis-model`: `cargo clippy --all-targets -- -D warnings` | pass |
| `crates/aegis-model`: `cargo test` | pass (all suites ok) |
| `~/.cargo/bin/cargo +1.85 check --locked` | pass (Homebrew cargo is first on PATH and does not take `+toolchain`; same note as round 1) |
| extra: `gh api repos/<repo>/license` for checkout, rust-toolchain, actions/cache, install-action | MIT, MIT, MIT, Apache-2.0 (all on the D5 allow list) |
| extra: `actions/cache` SHA `55cc834...` exists and is the commit that tag `v6.1.0` points to | pass |

## Rework 1 requirements

- `Swatinem/rust-cache` gone from the workflow: yes. Remaining mentions are only in ADR 0007, as history and as the rejected option.
- `actions/cache` (MIT), pinned by full SHA with `# v6.1.0`: yes, `restore` at ci.yml:68 and `save` at ci.yml:85.
- Paths: `registry/index/`, `registry/cache/`, `git/db/` under `${CARGO_HOME:-$HOME/.cargo}`, plus `crates/*/target/` from the same glob as `scripts/ci.sh`: yes (ci.yml:49-62). A new crate is picked up with no edit.
- Key is OS + `RUST_TOOLCHAIN` + `hashFiles('crates/*/Cargo.lock')`, with a restore-key of OS + toolchain: yes (ci.yml:70-73).
- Only master saves, and only after `scripts/ci.sh` passed: yes. The save step uses the default `success()` condition plus `github.ref == 'refs/heads/master'`. `cache-hit != 'true'` also covers a partial restore-key hit (`cache-hit` is `false` then) and a total miss (empty), so a new lock file does get saved. The key is taken from `steps.cache.outputs.cache-primary-key`, which is the restore step's computed key.
- ADR 0007 names `actions/cache` and MIT, states that every action is checked against D5, and has a license-check loop in "Reproduce": yes.
- `CARGO_INCREMENTAL: 0` is an addition. It is small, explained, and matches what `rust-cache` did implicitly. Accepted.

## Acceptance criteria

- AC1: pass.
- AC2: pass. `push:` l.15, `pull_request:` l.16, jobs `check`, `deny`, `msrv`. No `pull_request_target`.
- AC3: pass. One `contents: read` (l.19), one `concurrency:` (l.21), three `timeout-minutes:` (30, 15, 20).
- AC4: pass. No `uses:` line lacks a 40-hex SHA, and none lacks a `# v` comment (6 lines).
- AC5: pass.
- AC6: pass by inspection. `scripts/ci.sh` is unchanged since round 1 (not in the round 2 diff), and I accepted its keep-going logic in round 1.
- AC7: pass (5 matches; clippy and test both use `--locked`).
- AC8: pass. `cargo-deny@0.20.2` is in ci.yml.
- AC9: pass.
- AC10: pass. README names `scripts/ci.sh`. ADR has context, options, decisions, numbers, and consequences.
- AC11: deferred to the approver after merge, as agreed. The ADR "Numbers" section still needs the GitHub run time added then.
- AC12: pass. `git diff --stat 7fc0912 -- crates deny.toml scripts/deny.sh spikes` is empty. Diff has 4 files, all in scope.

## Findings

No blocking findings. No Rust code changed, so D1, D2, D7, P4, A8, A9 and R1 do not apply.

Low severity, optional:

1. `.github/workflows/ci.yml:85` — the master-only save step and the new cache logic cannot be exercised before a push. The first master run (AC11) is the first real save, and the next run is the first real restore. Fix: none now. The approver should confirm the "Restore cargo cache" log shows a hit on the second run, and record that in ADR 0007 "Numbers" with the run time.
2. `docs/adr/0007-workspace-ci.md` Decision 4, "Known limit" — a restore-key hit copies the old `target/` forward, and nothing prunes it, so the entry can grow with each lock-file change. The ADR states this and the bound, so it is acceptable. Fix: add a prune step if the entry nears the size limit (later job).
3. `.github/workflows/ci.yml` msrv step — still duplicates the crate-discovery loop from `scripts/ci.sh`. Same as round 1; the spec asked for it. Move it to a script if a third copy appears.
4. Build log: the round 2 section in `build.md` is appended under the job-level file, not under `round-2/`. No effect on the result.
