VERDICT: PASS

Reviewed `factory/worktrees/008-workspace-ci` at cb8d4ce against base 7fc0912. Spec `## Checks` re-run from that worktree.

## Check results

| Command | Result |
|---|---|
| `actionlint .github/workflows/ci.yml` | pass (no output, exit 0) |
| `scripts/ci.sh` | pass (ok fmt, clippy, test for aegis-model; "all steps passed in 1 crate(s)") |
| `scripts/deny.sh` | pass (exit 0, advisories included) |
| `tools/deny-fixtures/run.sh` | pass ("all 8 fixture cases match") |
| `crates/aegis-model`: `cargo fmt --check` | pass |
| `crates/aegis-model`: `cargo clippy --all-targets -- -D warnings` | pass |
| `crates/aegis-model`: `cargo test` | pass (all suites ok) |
| `~/.cargo/bin/cargo +1.85 check --locked` | pass |
| extra: fmt, clippy `--locked`, test `--locked` under the pinned rustc 1.99.1 (installed to a scratch RUSTUP_HOME) | pass. The pin is real (channel-rust-1.99.1.toml returns 200, and it is the current stable) and does not turn CI red. |
| extra: `shellcheck -s sh scripts/ci.sh` | pass |
| extra: `gh api repos/<action>/commits/<sha>` for all 4 pinned SHAs | all 4 exist |

## Acceptance criteria

- AC1: pass.
- AC2: pass. `push:` is at line 15 and `pull_request:` at line 16. Jobs `check`, `deny`, `msrv` exist. `pull_request_target` does not appear.
- AC3: pass. One `contents: read`, one `concurrency:`, three `timeout-minutes:` (30, 15, 20).
- AC4: pass. No `uses:` line lacks a 40-hex SHA, and each has a `# v` comment.
- AC5: pass.
- AC6: not re-run. I read the keep-going logic. `run_step` never exits on failure, `check_crate` always runs all three steps, a missing `Cargo.toml` is counted and then the loop continues, and zero checked crates gives exit 1. build.md records the exact commands and output, and they are consistent with the code.
- AC7: pass. `grep -c -- '--locked' scripts/ci.sh` prints 5, and clippy and test both use `--locked`.
- AC8: pass. `cargo-deny@0.20.2` is at ci.yml line 86.
- AC9: pass.
- AC10: pass. README has a "Continuous integration" section naming `scripts/ci.sh`, and ADR 0007 has context, options, decisions, numbers, and a reproduce section.
- AC11: deferred to the approver after merge, as agreed (rework Q6). ADR "Numbers" still needs the GitHub run time added then.
- AC12: pass. `git diff --stat 7fc0912 -- crates deny.toml scripts/deny.sh spikes` is empty. The diff has only 4 files, all in scope.

## Findings

No blocking findings. Standards: there is no Rust code in this diff, so D1, D2, D7, P4, A8, A9 and R1 do not apply. `ci.sh` documents its cost (O(crates x build time)), and the timeouts satisfy R6.

Low severity, optional:

1. `docs/adr/0007-workspace-ci.md` "Numbers" line 145 — says the timings were taken with "Rust 1.99.1", but the machine's default rustc is 1.97.1 and no 1.99.1 toolchain was installed. The clippy and test results do hold under 1.99.1 (checked above), but the timings were not measured with it. Fix: change the label to the compiler actually used, or re-time under 1.99.1.
2. `README.md:40-41` — repeats the pin ("CI pins Rust 1.99.1"), so it will go stale at the next bump. Fix: say "CI pins a Rust version (see `RUST_TOOLCHAIN` in ci.yml)" or drop the number.
3. `.github/workflows/ci.yml:108` onward — the msrv shell loop duplicates the discovery rule from `scripts/ci.sh`. This is what the spec asked for. If a third copy appears, move it to a script.

## Extras beyond the spec (judged)

All six builder additions are small, safe, and consistent with the spec's goals, so I accept them. `persist-credentials: false`, `fallback: none` and `save-if` are hardening only. The generated rust-cache `workspaces` list (an echo loop writing to `$GITHUB_OUTPUT`) matches the "no workflow edit" goal. `github.ref` appears only in `with:` and `concurrency:`, never in a `run:` script, so there is no injection path.
