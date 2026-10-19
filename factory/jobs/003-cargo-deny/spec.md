# 003-cargo-deny: cargo-deny license policy (D5)

## Open questions

1. **CI workflow now or later?** D5 says "`cargo-deny` in CI", but the repo has no CI yet (`.github/` does not exist; direction-v1 section 13 step 4 puts CI in the workspace skeleton). This spec adds the policy file and one script that CI can call. It does **not** add a workflow file. If the approver wants `.github/workflows/deny.yml` in this job, add it and an AC that it calls `scripts/deny.sh`.
2. **OFL-1.1 scope.** D5 lists OFL-1.1 as allowed "(fonts)". This spec allows it globally, because cargo-deny cannot limit a license to font crates. The other option is to leave it out of `allow` and add a per-crate exception for each font crate. Confirm.
3. **Spikes.** `spikes/` (including Slint under its royalty-free license, still open in D7) is not checked. Spike code is not production (L3). Confirm that spikes stay out of the policy.
4. **Advisories need the network.** `cargo deny check advisories` fetches the RustSec database. A failure there can come from a new advisory, not from this job. The builder records the result; a new advisory in a dependency is reported, not fixed, in this job.

## Goal

Every production crate is checked against the license policy of decision D5 with one command. A dependency under an allowed license (MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, BSL-1.0, CC0-1.0, MIT-0, OFL-1.1) passes. A dependency under any other license fails, including the forbidden ones (GPL, LGPL, AGPL, SSPL, BUSL, Commons Clause, non-commercial) and the "review first" ones (MPL-2.0, the Slint royalty-free license, AI model and data catalog licenses). A review-first license can pass only through a named per-crate exception in the config, with a comment that points to the approving decision. The same run also checks advisories, banned or duplicate crates, and dependency sources (standard Q7). Users of the desktop app see no change.

## Scope

In:
- `deny.toml` at the repo root: `[licenses]`, `[advisories]`, `[bans]`, `[sources]`, `[graph]`.
- `scripts/deny.sh`: runs `cargo deny check` for every `crates/*/Cargo.toml` with the root `deny.toml`. Exits non-zero if any crate fails. This is the entry point for CI later.
- Negative and positive test fixtures under `tools/deny-fixtures/` and a runner `tools/deny-fixtures/run.sh` that proves the policy rejects what D5 forbids.
- A short "Dependency licenses" section in `README.md` (or `docs/engineering-standards.md` next to Q7): how to install cargo-deny (`cargo install cargo-deny --locked`, the pinned version), how to run the script, and how to add a reviewed exception.
- A decision record `docs/adr/0003-cargo-deny-policy.md` (L1): why allow-list only, why `multiple-versions = "warn"`, why private crates are ignored, why an exception must name the crate and version.

Out:
- CI workflow files (Open question 1).
- `cargo-about` and the third-party notices in the About dialog (separate job).
- Checking `spikes/` (Open question 3).
- A Cargo workspace. Changing any dependency of `aegis-model`. MSRV changes (already pinned at 1.85).
- Any exception for Slint, MPL-2.0, or other review-first licenses. None are approved yet.

## Design notes

Files to add: `deny.toml`, `scripts/deny.sh`, `tools/deny-fixtures/**`, `docs/adr/0003-cargo-deny-policy.md`. Files to edit: `README.md` (or `docs/engineering-standards.md`). No Rust source changes in `crates/aegis-model`.

**cargo-deny version.** Use the current cargo-deny release (0.18.x or newer), installed with `--locked`. Record the exact version in the ADR and the README section. Use the current config format (no deprecated keys; `cargo deny check` must print no config deprecation warnings).

**`deny.toml` sketch** (field names must match the pinned version):
```toml
[graph]
all-features = true            # check optional deps too

[licenses]
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib",
         "Unicode-3.0", "BSL-1.0", "CC0-1.0", "MIT-0", "OFL-1.1"]
confidence-threshold = 0.93
exceptions = []                # review-first licenses go here, one crate each, with a D-number comment
[licenses.private]
ignore = true                  # our own publish = false crates (LicenseRef-Proprietary)

[advisories]
yanked = "deny"
ignore = []

[bans]
multiple-versions = "warn"     # aegis-model already locks two syn, getrandom, r-efi
wildcards = "deny"
allow-wildcard-paths = true

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```
- Allow list only. Do not add a `deny` list or a GPL entry: anything not allowed fails, so new license kinds fail by default.
- Do not allow `Unicode-DFS-2016`, `MPL-2.0`, `LGPL-*`, or any `LicenseRef-*`. If a current dependency needs one, stop and report it; do not add it.
- OR expressions pass when one branch is allowed (for example `rustix`: `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`; `r-efi`: `MIT OR Apache-2.0 OR LGPL-2.1-or-later`). AND expressions need every part allowed (for example `unicode-ident`: `(MIT OR Apache-2.0) AND Unicode-3.0`).
- Dev-dependencies are checked too (default). Test tools still enter our build machines.

**Config lookup.** Always pass the config explicitly: `cargo deny --manifest-path <crate>/Cargo.toml check --config <repo>/deny.toml`. Do not rely on cargo-deny searching parent directories.

**`scripts/deny.sh`.** POSIX `sh` or `bash` with `set -eu`. Resolves the repo root from the script location, loops over `crates/*/Cargo.toml` in sorted order, runs `cargo deny ... check` (all four checks) for each, keeps going after a failure, prints a one-line summary per crate, and exits 1 if any failed. Optional argument: the check names to run (default all), so `scripts/deny.sh licenses bans sources` runs offline. Fails with a clear message if `cargo-deny` is not installed.

**Fixtures** (`tools/deny-fixtures/`). Each case is a tiny `publish = false` root crate with one path dependency. The dependency crates live in a sibling `deps/` directory (not inside the root crate's directory, so Cargo does not make them workspace members and `private.ignore` does not skip them) and do **not** set `publish = false`. Each crate has an empty `src/lib.rs`. No registry dependencies, so the fixtures work offline. Cases:

| Case | Dependency license | Expected `check licenses` |
|---|---|---|
| `ok-mit` | `MIT` | pass |
| `ok-or` | `GPL-3.0-only OR MIT` | pass |
| `bad-gpl` | `GPL-3.0-only` | fail |
| `bad-agpl` | `AGPL-3.0-only` | fail |
| `bad-lgpl` | `LGPL-2.1-or-later` | fail |
| `bad-mpl` | `MPL-2.0` (review first) | fail |
| `bad-slint` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0` | fail |
| `bad-and` | `MIT AND GPL-3.0-only` | fail |

`run.sh` runs `cargo deny --manifest-path <case>/Cargo.toml check --config ../../deny.toml licenses` for each case and compares the exit status with the table. It exits 1 on any mismatch and prints which case broke. Fixture `Cargo.lock` and `target/` stay out of git (add to `.gitignore`), so the fixtures cannot be reached by `scripts/deny.sh` (it only reads `crates/`).

**Cost.** One `cargo deny` run per crate: O(D) in the number of locked dependencies, plus one advisory database fetch. Not on any product hot path.

## Acceptance criteria

- [ ] AC1: `deny.toml` exists at the repo root, and its `licenses.allow` is exactly the 11 SPDX IDs from D5 (no more, no less). Check: `grep -A3 '^allow' deny.toml` and reviewer reads it; `run.sh` cases `ok-mit` and `bad-*` cover behavior.
- [ ] AC2: The policy passes on the current production crate, offline checks. Command (repo root): `cargo deny --manifest-path crates/aegis-model/Cargo.toml check --config deny.toml licenses bans sources` exits 0 with no errors and no config deprecation warnings.
- [ ] AC3: Advisories run. Command: `cargo deny --manifest-path crates/aegis-model/Cargo.toml check --config deny.toml advisories`. Exits 0, or the build log names the advisory (Open question 4). `[advisories] ignore` stays empty.
- [ ] AC4: Every row of the fixture table behaves as listed. Command: `tools/deny-fixtures/run.sh` exits 0 and prints 8 results.
- [ ] AC5: Removing `"MIT"` from `allow` (temporary local edit, not committed) makes `run.sh` report `ok-mit` as a mismatch and exit 1. The builder records this in `build.md`. This shows the runner can fail.
- [ ] AC6: `scripts/deny.sh` checks every crate under `crates/`. Command: `scripts/deny.sh licenses bans sources` exits 0 and prints one line for `aegis-model`. With a temporary copy of a `bad-gpl` fixture under `crates/` (not committed), it exits 1 and names that crate.
- [ ] AC7: `scripts/deny.sh` prints an install hint and exits non-zero when `cargo-deny` is missing. Check: `PATH=/usr/bin:/bin scripts/deny.sh` (or equivalent with cargo-deny hidden).
- [ ] AC8: `[licenses] exceptions` and `[advisories] ignore` are empty, and `deny.toml` contains no `LicenseRef-`, `MPL`, `GPL`, or `Unicode-DFS` entry. Command: `! grep -nE 'LicenseRef-|MPL|GPL|Unicode-DFS' deny.toml`.
- [ ] AC9: `docs/adr/0003-cargo-deny-policy.md` exists and the README (or standards) section names the pinned cargo-deny version and the exception process. Check: `grep -n 'cargo-deny' README.md docs/adr/0003-cargo-deny-policy.md`.
- [ ] AC10: No change to `crates/aegis-model` sources or dependencies. Check: `git diff --stat master -- crates/` is empty, and its existing checks stay green.

## Checks

From the repo root:
```sh
cargo deny --version
cargo deny --manifest-path crates/aegis-model/Cargo.toml check --config deny.toml licenses bans sources
cargo deny --manifest-path crates/aegis-model/Cargo.toml check --config deny.toml advisories
scripts/deny.sh
tools/deny-fixtures/run.sh
```

From `crates/aegis-model/`:
```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
