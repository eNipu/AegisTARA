# 008-workspace-ci: GitHub Actions CI for fmt, clippy, tests, and cargo-deny

## Open questions

1. **Which crates.** CI covers production crates only (`crates/*/`), found by glob the same way `scripts/deny.sh` finds them, so a new crate is picked up with no workflow edit. `spikes/` is left out (standard L3; the spikes also pull in heavy GUI dependencies). `tools/deny-fixtures/run.sh` is added to the deny job because it is offline, takes seconds, and catches a `deny.toml` edit that weakens the D5 policy. Confirm both choices.
2. **Advisories can break unrelated pushes.** `scripts/deny.sh` with no arguments includes `advisories`, which fetches the RustSec database. A new advisory can turn CI red without any code change. This spec runs all four checks, and a red result blocks the push (the point of Q7). The other option is to run `advisories` as a separate non-blocking step plus a weekly scheduled run. Confirm.
3. **Toolchain pin.** The workflow installs one fixed stable Rust version (the builder picks the current stable release, writes it in one `env` variable, and names it in the ADR). The alternative is floating `stable`, but new clippy lints would then turn CI red on unrelated pushes. No `rust-toolchain.toml` is added, so local toolchains do not change. An extra job checks the MSRV (`rust-version = "1.85"`) with `cargo check --locked` (library only, because dev-dependencies such as `divan` may need a newer compiler). Confirm the pin and the MSRV job.
4. **Runner OS.** Only `ubuntu-24.04`. The product ships Windows first (D2), but no crate has OS-specific code yet. Adding `windows-latest` to the `check` job is one matrix line and roughly doubles CI time. Confirm Linux only for now.
5. **Third-party actions.** Allowed: `actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, and `taiki-e/install-action` (installs prebuilt cargo-deny 0.20.2 and checks its checksum, instead of a 3 to 5 minute `cargo install`). Each one is pinned to a full commit SHA with the version in a comment. Confirm this list, or ask for `actions/checkout` only plus `cargo install`.
6. **Proof on GitHub.** The builder can prove the workflow locally (actionlint, and the same scripts CI calls), but a green run on GitHub only exists after a push. AC11 is checked by the approver after merge. Confirm, or allow the builder to push the job branch.

## Goal

Every push and every pull request to `github.com/eNipu/AegisTARA` runs the quality gates that standards Q1 and Q7 and decision D5 ("`cargo-deny` in CI") require: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `scripts/deny.sh` for each production crate. A broken gate shows as a red check on the commit or the pull request, with a log that names the crate and the failing step. Developers run the same gates locally with one command, `scripts/ci.sh`, so a green local run means a green CI run.

## Scope

In:
- `.github/workflows/ci.yml` with triggers `push` (all branches) and `pull_request`, and three jobs: `check` (fmt, clippy, test through `scripts/ci.sh`), `deny` (`scripts/deny.sh` and `tools/deny-fixtures/run.sh`), and `msrv` (`cargo check --locked` at Rust 1.85 for each crate).
- `scripts/ci.sh`: a POSIX sh script, written like `scripts/deny.sh`, that runs fmt, clippy, and test for each crate.
- A short "Continuous integration" section in `README.md`: what CI runs, and `scripts/ci.sh` for running it locally.
- `docs/adr/0007-workspace-ci.md` (standard L1): per-crate loop instead of a Cargo workspace, toolchain pin, action pinning, and cache choice, with the measured CI run time.

Out:
- A Cargo workspace or a shared `target/` (later job; the loop in `scripts/ci.sh` is the bridge until then).
- `spikes/` crates, benchmarks and the 10% regression gate (section 8), coverage, mutation testing, fuzzing, Miri.
- Windows or macOS runners, release builds, code signing, `cargo-about` notices.
- Branch protection rules (a GitHub setting, not repo content).
- Changes to `crates/`, `deny.toml`, or `scripts/deny.sh`.

## Design notes

**`scripts/ci.sh [CRATE_DIR...]`.** Defaults to every `crates/*/` directory that has a `Cargo.toml`, in sorted glob order (as in `deny.sh`). For each crate, run in its directory:
1. `cargo fmt --check`
2. `cargo clippy --locked --all-targets -- -D warnings`
3. `cargo test --locked`

`--locked` fails if `Cargo.lock` is out of date, so CI tests exactly the committed dependency set (P4, Q7). The script keeps going after a failure (one crate failing does not hide another crate's failure), prints one `ok    <crate> <step>` or `FAIL  <crate> <step>` line per step, and prints the full output only for failing steps. It exits 1 if any step failed, and exits 1 with an error if it found no crates. Taking crate directories as arguments makes the script testable on a throwaway crate (AC5, AC6). `set -eu`, a `trap` for Ctrl-C like `deny.sh`. Inside GitHub Actions (`GITHUB_ACTIONS=true`) it wraps each crate in `::group::`/`::endgroup::` so the log folds per crate. Cost: one fmt, clippy, and test run per crate, O(crates x build time).

**`.github/workflows/ci.yml`.**
- `permissions: contents: read` at the top level (least privilege; no job needs to write).
- `concurrency: { group: ci-${{ github.ref }}, cancel-in-progress: true }`, so a new push cancels the old run on the same branch.
- Each job has `timeout-minutes` (R6): `check` 30, `deny` 15, `msrv` 20.
- `env`: `CARGO_TERM_COLOR: always`, `RUST_BACKTRACE: 1`, and the pinned toolchain version (Open question 3).
- `check`: checkout, `dtolnay/rust-toolchain` with the pinned version and `components: rustfmt, clippy`, `Swatinem/rust-cache` with `workspaces:` listing each crate directory (`crates/aegis-model -> target`), then `scripts/ci.sh`.
- `deny`: checkout, `taiki-e/install-action` with `tool: cargo-deny@0.20.2` (the version pinned in ADR 0003), then `scripts/deny.sh` and `tools/deny-fixtures/run.sh`. No Rust cache is needed: cargo-deny reads `Cargo.lock` and metadata and does not compile.
- `msrv`: toolchain `1.85`, then `cargo check --locked` in each `crates/*/`, using a shell loop in the step, with the same discovery rule as `ci.sh`.
- Every `uses:` is pinned to a 40-character commit SHA, with `# vX.Y.Z` after it (supply chain, Q7).
- No secrets, no `pull_request_target`, and no `${{ }}` expressions from event data inside `run:` scripts (script injection).

## Acceptance criteria

Commands run from the repo root unless noted.

- [ ] AC1 The workflow is valid: `actionlint .github/workflows/ci.yml` (actionlint 1.7 or later; install with `brew install actionlint` or `go install github.com/rhysd/actionlint/cmd/actionlint@latest`) prints nothing and exits 0.
- [ ] AC2 Triggers and jobs: `grep -nE '^\s*(push|pull_request):' .github/workflows/ci.yml` shows both triggers, and the file defines the jobs `check`, `deny`, and `msrv`. `grep -n 'pull_request_target' .github/workflows/ci.yml` prints nothing.
- [ ] AC3 Least privilege and limits: the file has a top-level `permissions:` block with `contents: read`, a `concurrency:` block, and `timeout-minutes:` on every job. Check: `grep -nE 'contents: read|concurrency:|timeout-minutes:' .github/workflows/ci.yml` shows one `contents: read`, one `concurrency:`, and three `timeout-minutes:`.
- [ ] AC4 Every action is pinned by SHA: `grep -nE 'uses:' .github/workflows/ci.yml | grep -vE '@[0-9a-f]{40}( |$)'` prints nothing, and every `uses:` line has a `# v` version comment.
- [ ] AC5 `scripts/ci.sh` passes on the current tree: `scripts/ci.sh` exits 0 and prints an `ok` line for fmt, clippy, and test of `aegis-model`.
- [ ] AC6 `scripts/ci.sh` catches failures and keeps going: with two throwaway crates `a` and `b` made with `cargo new --lib` in a temp directory `$tmp` (run `cargo generate-lockfile` in each, because `--locked` fails without a `Cargo.lock`), one with a formatting error and one with a failing `#[test]`, `scripts/ci.sh "$tmp/a" "$tmp/b"` exits 1, prints `FAIL  a fmt` and `FAIL  b test`, and still runs every step of `b` after `a` failed. Given only a directory with no `Cargo.toml` (`scripts/ci.sh "$tmp/empty"`), it exits 1 with an error. The builder records the exact commands and output in `build.md`.
- [ ] AC7 `scripts/ci.sh` uses `--locked`: `grep -c -- '--locked' scripts/ci.sh` is at least 2 (clippy and test).
- [ ] AC8 Deny job matches the local run: `scripts/deny.sh` exits 0 (needs network for advisories) and `tools/deny-fixtures/run.sh` prints `all 8 fixture cases match`. The workflow installs cargo-deny `0.20.2`: `grep -n 'cargo-deny@0.20.2' .github/workflows/ci.yml` matches.
- [ ] AC9 MSRV: `cd crates/aegis-model && cargo +1.85 check --locked` exits 0 (install with `rustup toolchain install 1.85 --profile minimal`). If it fails, the builder stops and reports which dependency needs a newer compiler, instead of raising `rust-version`.
- [ ] AC10 Docs: `README.md` has a "Continuous integration" section naming `scripts/ci.sh`, and `docs/adr/0007-workspace-ci.md` exists with context, options, decision, and the measured CI run time (from AC11, or a local `time scripts/ci.sh` until then). Check: `grep -n 'scripts/ci.sh' README.md` and `test -f docs/adr/0007-workspace-ci.md`.
- [ ] AC11 (after merge, checked by the approver, see Open question 6) The first run on GitHub is green for all three jobs: `gh run list --workflow ci.yml --limit 1` shows `completed success`.
- [ ] AC12 No change outside the scope: `git diff --stat <base> -- crates deny.toml scripts/deny.sh spikes` is empty, where `<base>` is the job's base commit.

## Checks

From the repo root:

```sh
actionlint .github/workflows/ci.yml
scripts/ci.sh
scripts/deny.sh
tools/deny-fixtures/run.sh
```

From `crates/aegis-model/` (the only crate today; `scripts/ci.sh` runs the same three):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo +1.85 check --locked
```
