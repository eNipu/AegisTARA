## Round 1
Commits: 54fdbf1 Add cargo-deny license policy with fixtures and run script
Files:
- deny.toml (new)
- scripts/deny.sh (new, executable)
- tools/deny-fixtures/run.sh (new, executable)
- tools/deny-fixtures/{ok-mit,ok-or,bad-gpl,bad-agpl,bad-lgpl,bad-mpl,bad-slint,bad-and}/{Cargo.toml,src/lib.rs} (new)
- tools/deny-fixtures/deps/<same 8 cases>/{Cargo.toml,src/lib.rs} (new)
- docs/adr/0003-cargo-deny-policy.md (new)
- README.md (new "Dependency licenses" section)
- .gitignore (fixture Cargo.lock and target/ ignored)

Acceptance criteria:
- [x] AC1 — `grep -A13 '^allow' deny.toml` shows exactly the 11 D5 IDs (MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, BSL-1.0, CC0-1.0, MIT-0, OFL-1.1). The list is one ID per line, so `-A3` from the spec shows only the first three. No `deny` list.
- [x] AC2 — `cargo deny --manifest-path crates/aegis-model/Cargo.toml check --config deny.toml licenses bans sources` → exit 0, "bans ok, licenses ok, sources ok". No errors, no deprecation warnings (`grep -i deprecat` on full output is empty). The only warnings are `warning[duplicate]` for getrandom and r-efi (expected, multiple-versions = "warn").
- [x] AC3 — `cargo deny ... check --config deny.toml advisories` → exit 0, "advisories ok" (2026-10-19). No advisory found. `[advisories] ignore = []`.
- [x] AC4 — `tools/deny-fixtures/run.sh` → exit 0, 8 result lines plus "all 8 fixture cases match". I checked by hand that every bad-* case fails with `error[rejected]: failed to satisfy license requirements`. The runner now requires that text before it counts a "fail" (otherwise it reports `error`), so a broken config cannot pass as a correct rejection.
- [x] AC5 — I deleted the `"MIT",` line from deny.toml as a temporary edit. `tools/deny-fixtures/run.sh` then printed `MISMATCH ok-mit expected pass, got fail` and `MISMATCH ok-or expected pass, got fail` ("2 of 8 fixture case(s) did not match") and exited 1. I restored deny.toml from a backup and checked it with `cmp`.
- [x] AC6 — `scripts/deny.sh licenses bans sources` → exit 0, prints `ok    aegis-model`. As a temporary test I copied bad-gpl to `crates/zz-bad-gpl` (path dep rewritten to `../../tools/deny-fixtures/deps/bad-gpl`). The script then printed `ok aegis-model`, `FAIL zz-bad-gpl` with the cargo-deny report and "1 of 2 crate(s) failed", and exited 1. I removed the copy afterwards and `git status crates` was clean.
- [x] AC7 — `PATH=/usr/bin:/bin scripts/deny.sh` → prints "error: cargo-deny is not installed." and "install it with: cargo install cargo-deny --version 0.19.9 --locked", exit 127.
- [x] AC8 — `! grep -nE 'LicenseRef-|MPL|GPL|Unicode-DFS' deny.toml` → exit 0. `exceptions = []`, `ignore = []`. The first draft had a comment that said "LicenseRef-Proprietary". I reworded it so the grep passes.
- [x] AC9 — `grep -n 'cargo-deny' README.md docs/adr/0003-cargo-deny-policy.md` → both files name cargo-deny 0.19.9. The README describes the exception process (record the decision, then add a `{ crate = "name@version", allow = [...] }` entry with a D-number comment). The ADR explains why.
- [x] AC10 — `git diff --stat master -- crates/` is empty. In crates/aegis-model, fmt, clippy and test all pass.

Checks:
- `cargo deny --version` → `cargo-deny 0.19.9` → pass
- `cargo deny --manifest-path crates/aegis-model/Cargo.toml check --config deny.toml licenses bans sources` → pass (exit 0)
- `cargo deny --manifest-path crates/aegis-model/Cargo.toml check --config deny.toml advisories` → pass (exit 0)
- `scripts/deny.sh` → pass (exit 0, all four checks, network used)
- `tools/deny-fixtures/run.sh` → pass (exit 0, 8/8)
- crates/aegis-model `cargo fmt --check` → pass
- crates/aegis-model `cargo clippy --all-targets -- -D warnings` → pass
- crates/aegis-model `cargo test` → pass (51 unit + 3 + 2 integration + 32 doc + 1, 0 failed)

Notes:
- **Pinned version is 0.19.9, not the latest 0.20.2. This is a deliberate deviation.** I installed 0.20.2 first. In 0.20.x, `--config` is a top-level option (`cargo deny --config deny.toml check ...`), and `check --config` is rejected with "unexpected argument '--config'". That breaks every command in the spec's Checks and Design notes as written. I tested 0.19.0 and 0.19.9. Both accept `check --config`, and 0.19.9 is the newest such release (published 2026-06-15). That meets "0.18.x or newer". The ADR "Tool version" section records the choice and what an upgrade to 0.20 would require: change scripts/deny.sh, tools/deny-fixtures/run.sh, the README, and the spec commands together. If the approver would rather have 0.20.2, that is a small mechanical change in a later round.
- The machine's global cargo-deny is now 0.19.9 (`cargo install --version 0.19.9 --locked --force` replaced the 0.20.2 I installed first).
- Additions beyond the sketch:
  - `unused-allowed-license = "allow"`: without it, every run prints 8 `license-not-encountered` warnings for allowed IDs that no current dependency uses. The allow list is policy, so those warnings are noise.
  - `multiple-versions-include-dev = true`: cargo-deny skips dev-dependencies in the duplicate check by default. The getrandom and r-efi duplicates come in only through proptest (dev), so with `warn` alone nothing would ever be reported. The spec says dev-deps are checked too, so I enabled it.
  - Duplicate `syn` versions: syn 2 and 3 are both in Cargo.lock, but cargo-deny 0.19.9 does not report that pair. syn 2 is reached only through the zerocopy-derive proc-macro under proptest. This is noted in the ADR.
- Open questions: no CI workflow was added (OQ1). OFL-1.1 is allowed globally, per the spec and with the reason in the ADR (OQ2). spikes/ is not checked (OQ3). Advisories passed (OQ4).
- The scripts are POSIX `sh` with `set -eu`, and both pass `sh -n`. shellcheck is not installed here. run.sh keeps its loop in the current shell (here-document, not a pipe), so the mismatch counter survives the loop. AC5 confirms this.
- Fixture dependency crates do not set `publish = false`, so `licenses.private.ignore` does not skip them. Fixture Cargo.lock files are gitignored.

## Round 2
Commits: 49ddaf9 Pin cargo-deny 0.20.2 and record accepted open questions
Files: deny.toml (pin comment), scripts/deny.sh, tools/deny-fixtures/run.sh, README.md, docs/adr/0003-cargo-deny-policy.md

Rework applied (rework.md, Rework 1):
- I installed and pinned cargo-deny 0.20.2 with `cargo install cargo-deny --version 0.20.2 --locked --force`, which replaced 0.19.9 on this machine. All pin references now say 0.20.2: deny.toml header, both install hints, README, ADR.
- All commands now use the 0.20 form, with `--config` as a top-level option before `check`: `cargo deny --manifest-path <crate>/Cargo.toml --config <repo>/deny.toml check [CHECK...]`. This covers scripts/deny.sh, tools/deny-fixtures/run.sh, the README (new one-crate example) and the ADR's "Tool version" section. No `check --config` remains: `grep -rn -- 'check --config'` over those files is empty.
- ADR 0003 has a new section, "Accepted open questions", covering:
  - OQ1: CI comes later.
  - OQ2: OFL-1.1 is allowed globally.
  - OQ3: spikes/ is not checked.
  - OQ4: how advisory results are recorded.
  - The acceptance of the two extra deny.toml keys.
- The ADR Status line now says rework 1 was approved by the owner.

Round-1 review notes (all PASS, non-blocking). Addressed because each fix was small:
- review-security: both scripts now have `trap 'rm -f "$log"' EXIT` plus `trap 'rm -f "$log"; exit 130' INT TERM`, so an interrupt stops the run instead of moving on to the next crate. Verified by sending TERM to a running `scripts/deny.sh`: exit 130, no temp file left. A SIGINT test from a background job is not valid, because a non-interactive sh ignores INT for async jobs.
- review-security: ADR Decision 3 now records a known gap: `private.ignore` would also skip a vendored `publish = false` crate. It names the follow-ups (`ignore-sources`, or a review rule).
- review-ux: the scripts/deny.sh failure summary now goes to stdout with the FAIL lines, so they do not interleave. When `advisories` was one of the checks run, the summary adds a note on how to rerun offline (`scripts/deny.sh licenses bans sources`).

Acceptance criteria (rerun with cargo-deny 0.20.2):
- [x] AC1 — the allow list is unchanged: exactly the 11 D5 IDs.
- [x] AC2 — `cargo deny --manifest-path crates/aegis-model/Cargo.toml --config deny.toml check licenses bans sources` → exit 0, "bans ok, licenses ok, sources ok". The only warnings are duplicates (getrandom, r-efi). The full-check output has 0 lines that match `deprecat`.
- [x] AC3 — `... --config deny.toml check advisories` → exit 0, "advisories ok". `ignore = []`.
- [x] AC4 — `tools/deny-fixtures/run.sh` → exit 0, 8/8 match.
- [x] AC5 — with `"MIT"` temporarily removed, run.sh reports MISMATCH for ok-mit and ok-or and exits 1. deny.toml was restored and checked with `cmp`.
- [x] AC6 — `scripts/deny.sh licenses bans sources` → exit 0, `ok aegis-model`. With a temporary `crates/zz-bad-gpl`, it prints `FAIL zz-bad-gpl` and exits 1. Running all checks shows the advisories note as well. The temporary crate was removed and `git status crates` is clean.
- [x] AC7 — `PATH=/usr/bin:/bin scripts/deny.sh` → install hint for 0.20.2, exit 127.
- [x] AC8 — `! grep -nE 'LicenseRef-|MPL|GPL|Unicode-DFS' deny.toml` → exit 0.
- [x] AC9 — README and ADR both name cargo-deny 0.20.2 and the exception process.
- [x] AC10 — `git diff --stat master -- crates/` is empty. fmt, clippy and tests pass.

Checks (0.20 form, repo root):
- `cargo deny --version` → cargo-deny 0.20.2 → pass
- `cargo deny --manifest-path crates/aegis-model/Cargo.toml --config deny.toml check licenses bans sources` → pass
- `cargo deny --manifest-path crates/aegis-model/Cargo.toml --config deny.toml check advisories` → pass
- `scripts/deny.sh` → pass
- `tools/deny-fixtures/run.sh` → pass (8/8)
- crates/aegis-model: `cargo fmt --check` → pass; `cargo clippy --all-targets -- -D warnings` → pass; `cargo test` → pass (51+3+2+32+1, 0 failed)

Notes:
- The spec's `## Checks` still shows the 0.19 form (`check --config`). That form fails under 0.20.2. Reviewers should run the 0.20 form listed above. The spec file is outside the worktree, so I did not edit it.
- 0.20.2 behaves like 0.19.9 on duplicates: only getrandom and r-efi are reported, not the syn 2/3 pair. The ADR text still holds.
