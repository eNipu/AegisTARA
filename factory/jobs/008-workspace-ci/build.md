# Build: 008-workspace-ci

## Round 1
Commits: cb8d4ce Add GitHub Actions CI for fmt, clippy, tests, and cargo-deny
Base: 7fc0912 (branch `factory/008-workspace-ci`, not pushed)
Files:
- `.github/workflows/ci.yml` (new)
- `scripts/ci.sh` (new, mode 100755)
- `docs/adr/0007-workspace-ci.md` (new)
- `README.md` (new "Continuous integration" section)

Tools used: actionlint 1.7.12 (Homebrew), shellcheck (Homebrew), cargo-deny 0.20.2,
Rust 1.99.1 (current stable on 2026-10-20, per static.rust-lang.org channel-rust-stable.toml),
Rust 1.85.1 via `rustup toolchain install 1.85 --profile minimal`.

Acceptance criteria:
- [x] AC1 — `actionlint .github/workflows/ci.yml` printed nothing, exit 0 (actionlint 1.7.12 with shellcheck on PATH, so the `run:` scripts were shellchecked too).
- [x] AC2 — `grep -nE '^\s*(push|pull_request):'` → `14:  push:` and `15:  pull_request:`. Jobs `check` (l.34), `deny` (l.69), `msrv` (l.93). `grep -n 'pull_request_target'` prints nothing, exit 1. (The first draft had the word in a header comment; I reworded it so the grep stays empty.)
- [x] AC3 — `grep -nE 'contents: read|concurrency:|timeout-minutes:'` → one `contents: read` (l.18), one `concurrency:` (l.20), three `timeout-minutes:` (30, 15, 20).
- [x] AC4 — `grep -nE 'uses:' ... | grep -vE '@[0-9a-f]{40}( |$)'` prints nothing; `grep -nE 'uses:' | grep -vc '# v'` → 0. Each SHA checked with `gh api repos/<repo>/commits/<sha>`:
  - actions/checkout 3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1 (latest release)
  - dtolnay/rust-toolchain 683b588c487a69d01f6c6ddf40f036ff682cc2c8 # v1 (= master HEAD)
  - Swatinem/rust-cache d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2.9.3 (annotated tag, so this is the peeled commit, not the tag object 2201cc5...)
  - taiki-e/install-action 8cca0918f8471c33515202b73f56ab27f2bed283 # v2.87.29 (its manifest lists cargo-deny 0.20.2)
- [x] AC5 — `scripts/ci.sh` exit 0:
  ```
  ok    aegis-model fmt
  ok    aegis-model clippy
  ok    aegis-model test
  all steps passed in 1 crate(s)
  ```
- [x] AC6 — exact commands (`$tmp` = a scratch directory):
  ```sh
  mkdir -p "$tmp/empty"; cd "$tmp"
  cargo new --lib --vcs none -q a && cargo new --lib --vcs none -q b
  (cd a && cargo generate-lockfile -q) && (cd b && cargo generate-lockfile -q)
  # a: formatting error, placed before the test module so clippy stays clean
  { printf 'pub fn badly_formatted( )->u8{1}\n'; cat a/src/lib.rs; } > a/lib.tmp && mv a/lib.tmp a/src/lib.rs
  # b: failing #[test]
  sed -i '' 's/assert_eq!(result, 4);/assert_eq!(result, 5);/' b/src/lib.rs
  scripts/ci.sh "$tmp/a" "$tmp/b"; echo "exit=$?"
  scripts/ci.sh "$tmp/empty"; echo "exit=$?"
  ```
  Output (summary lines; the rustfmt diff and the test panic are printed indented under each FAIL line):
  ```
  FAIL  a fmt
  ok    a clippy
  ok    a test
  ok    b fmt
  ok    b clippy
  FAIL  b test
  2 step(s) failed in 2 crate(s): a/fmt b/test
  exit=1
  error: no Cargo.toml in <tmp>/empty
  error: no crates found in: <tmp>/empty
  exit=1
  ```
  Every step of `a` and of `b` ran after `a fmt` failed. With `GITHUB_ACTIONS=true` the same run wraps each crate in `::group::a` / `::endgroup::` and emits `::error title=ci.sh::FAIL a fmt` and `::error title=ci.sh::FAIL b test`.
  (A first try appended the bad item after the test module; that also failed `a clippy` via `clippy::items_after_test_module`, which showed the keep-going path for two failures in one crate.)
- [x] AC7 — `grep -c -- '--locked' scripts/ci.sh` → 5 (clippy, test, plus 3 in comments).
- [x] AC8 — `scripts/deny.sh` → `ok    aegis-model`, exit 0 (network, advisories included). `tools/deny-fixtures/run.sh` → `all 8 fixture cases match`. `grep -n 'cargo-deny@0.20.2' .github/workflows/ci.yml` → line 85.
- [x] AC9 — `cd crates/aegis-model && cargo +1.85 check --locked` → `Finished dev profile`, exit 0 (rustc 1.85.1). I also extracted the `msrv` job's `run:` script from the YAML and ran it with `RUSTUP_TOOLCHAIN=1.85` under `bash -e -o pipefail`: prints `ok    aegis-model msrv`.
- [x] AC10 — `grep -n 'scripts/ci.sh' README.md` → lines 33, 34, 39; `test -f docs/adr/0007-workspace-ci.md` → true. ADR has Context, Options, five Decisions, Numbers (local `time scripts/ci.sh`: 23.9 s cold target, 23.1 s warm, M2 Max), Consequences, Reproduce.
- [ ] AC11 — not checkable by the builder (open question 6: no push). The approver runs `gh run list --workflow ci.yml --limit 1` after merge and adds the GitHub run time to ADR 0007 "Numbers".
- [x] AC12 — `git diff --stat 7fc0912 -- crates deny.toml scripts/deny.sh spikes` → empty.

Checks:
- `actionlint .github/workflows/ci.yml` → pass
- `scripts/ci.sh` → pass
- `scripts/deny.sh` → pass
- `tools/deny-fixtures/run.sh` → pass (all 8 fixture cases match)
- `crates/aegis-model`: `cargo fmt --check` → pass
- `crates/aegis-model`: `cargo clippy --all-targets -- -D warnings` → pass
- `crates/aegis-model`: `cargo test` → pass (all suites ok)
- `crates/aegis-model`: `cargo +1.85 check --locked` → pass
- extra: `shellcheck -s sh scripts/ci.sh` → pass

Notes:
- **Toolchain pin:** `RUST_TOOLCHAIN: "1.99.1"` and `RUST_MSRV: "1.85"` in the top-level `env`, passed to `dtolnay/rust-toolchain` via `${{ env.* }}` in `with:`. No `rust-toolchain.toml`.
- **Local `cargo +1.85`:** on this machine Homebrew's `cargo` comes before `~/.cargo/bin` on PATH, and Homebrew cargo does not understand `+toolchain`. I ran the MSRV check as `~/.cargo/bin/cargo +1.85 check --locked`. That is a local PATH issue, not a repo issue; CI uses rustup only.
- **Small additions beyond the spec text, for the reviewer to judge:**
  1. The rust-cache `workspaces:` list is **generated** by a step using the same `crates/*/Cargo.toml` glob (it outputs `crates/aegis-model -> target` today), not hard-coded. The point is that a new crate gets cached with no workflow edit, which matches the spec's "no workflow edit" goal. Hard-coding it is a two-line change if the reviewer prefers that.
  2. `save-if: ${{ github.ref == 'refs/heads/master' }}` on rust-cache, so branches restore the master cache but do not write their own. `github.ref` is used in `with:`, not in a `run:` script, so it is not a script-injection path.
  3. `persist-credentials: false` on every `actions/checkout`.
  4. `fallback: none` on `taiki-e/install-action`, so it fails instead of falling back to cargo-binstall or building from source.
  5. The `deny` job also installs the pinned toolchain (minimal, no components), because cargo-deny calls `cargo metadata` and I did not want it to depend on whatever cargo the runner image ships. Still no Rust cache in `deny`.
  6. `scripts/ci.sh` prints a final summary line (`N step(s) failed in M crate(s): a/fmt b/test`), and under GitHub Actions emits a `::error` annotation per failing step so the run summary names the crate and the step without opening a folded group.
- `scripts/ci.sh` with explicit args: a directory with no `Cargo.toml` is reported (`error: no Cargo.toml in DIR`), counted as a failure, and the script continues with the other directories; if no directory had a manifest, it exits 1 with `error: no crates found`.
- `pull_request` plus `push` on all branches means a PR from a branch in this repo runs twice (once per ref). The concurrency group is per ref, so the two runs do not cancel each other. That is what the spec asks for.
- Not pushed; master untouched.

## Round 2
Commits: d0a85cd Replace rust-cache with actions/cache to satisfy D5
Files:
- `.github/workflows/ci.yml`
- `docs/adr/0007-workspace-ci.md`
- `README.md`

Inputs: `rework.md` "## Rework 1" (owner: D5 is strict, no exception for CI-only tools), `decision.md` (ESCALATE on the `Swatinem/rust-cache` LGPL-3.0 license), `round-1/review-{code,security,ui,ux}.md` (all PASS).

Rework 1 items:
- [x] Replace `Swatinem/rust-cache` (LGPL-3.0) with `actions/cache` (MIT), pinned by SHA with a version comment. The workflow now uses `actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0` and `actions/cache/save@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0` (latest release; SHA checked with `gh api repos/actions/cache/commits/<sha>`; LICENSE at that SHA is "The MIT License"). `grep -n -i 'swatinem\|rust-cache' .github/workflows/ci.yml` prints nothing.
- [x] Cache paths: `$CARGO_HOME/registry/index/`, `$CARGO_HOME/registry/cache/`, `$CARGO_HOME/git/db/` (with `CARGO_HOME` defaulting to `$HOME/.cargo`), and `crates/<crate>/target/` for each crate. I used `$CARGO_HOME`/`$HOME` instead of a literal `~/.cargo`: actionlint's shellcheck flagged the quoted tilde (SC2088), and an absolute path also works if a runner sets `CARGO_HOME`. I ran the step script locally: it outputs the three cargo paths plus `crates/aegis-model/target/` (and `/opt/cargo/...` when `CARGO_HOME=/opt/cargo`).
- [x] Key: `cargo-${{ runner.os }}-${{ env.RUST_TOOLCHAIN }}-${{ hashFiles('crates/*/Cargo.lock') }}`. Restore key: `cargo-${{ runner.os }}-${{ env.RUST_TOOLCHAIN }}-`.
- [x] Only master saves: the save step has `if: github.ref == 'refs/heads/master' && steps.cache.outputs.cache-hit != 'true'` and runs after `scripts/ci.sh`, so only after a green run (default `success()`). It saves under `steps.cache.outputs.cache-primary-key`. It skips an exact hit because cache entries are immutable. A new crate needs no workflow edit: the path list comes from the `crates/*/Cargo.toml` glob, and the key from the `crates/*/Cargo.lock` glob.
- [x] ADR 0007 updated:
  - Status and Context record rework 1.
  - The Options row now names `actions/cache` (MIT) and says rust-cache is LGPL-3.0 and not allowed by D5.
  - Decision 3 has a License column (checkout MIT, rust-toolchain MIT, cache MIT, install-action Apache-2.0, from `gh api repos/<r>/license`) and states that every action in the workflow is checked against D5, with no CI-only exception. It also says this check is manual, because cargo-deny does not read workflows.
  - Decision 4 is rewritten for `actions/cache`: paths, key, restore key, master-only save, the known limit that nothing prunes `target/`.
  - Consequences now say an action change needs a D5 check.
  - Reproduce has the license loop.
- [x] All other round 1 choices kept.

One addition from the switch: `CARGO_INCREMENTAL: 0` in the top-level `env`. `rust-cache` set it implicitly, so plain `actions/cache` needs it explicit to keep the cached `target/` small. The CI behaviour is the same as round 1.

Optional review items:
- code #1 (ADR "Numbers" says Rust 1.99.1 but "no 1.99.1 toolchain was installed"): I disagree with the premise. The reviewer looked only at rustup, whose stable is 1.97.1. But `/opt/homebrew/bin/cargo` comes first on PATH, and it is Homebrew `cargo 1.99.1`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, clippy 0.1.99, rustfmt 1.10.0, so the timings were taken with 1.99.1. I made the label exact ("Homebrew Rust 1.99.1 (`/opt/homebrew/bin/cargo`, the first `cargo` on PATH ...)") so it cannot be misread again.
- code #2 / ux #5 (README repeats the pin): fixed. The README now says "CI pins one Rust version (`RUST_TOOLCHAIN` in `ci.yml`)".
- code #3 (the msrv loop duplicates the discovery rule): left as is, as the reviewer suggests, until a third copy appears.
- ux #1, #2, #4, #6 (fix hints, printing failure output outside the group, an MSRV hint, a progress line): not done. The rework says "keep all other choices from round 1", and the reviewer marked these non-blocking. Each is a small follow-up.
- ux #3 (`cancel-in-progress` on master): not done. The spec asks for plain `true`. Note that with the master-only cache save, a cancelled master run also skips its save, and the next master run saves instead.

Acceptance criteria (re-verified after the change):
- [x] AC1 — `actionlint .github/workflows/ci.yml` → no output, exit 0.
- [x] AC2 — `push:` l.15 and `pull_request:` l.16; jobs `check`, `deny`, `msrv`; `pull_request_target` grep is empty.
- [x] AC3 — one `contents: read` (l.19), one `concurrency:` (l.21), three `timeout-minutes:` (l.41, 93, 117).
- [x] AC4 — the unpinned `uses:` grep is empty; `grep -vc '# v'` on `uses:` lines → 0 (6 `uses:` lines, 5 distinct actions/sub-actions, 4 repos).
- [x] AC5 — `scripts/ci.sh` → ok fmt, clippy, test for aegis-model, exit 0.
- [x] AC6 — `scripts/ci.sh` is unchanged since round 1 (the round 1 commands and output still apply).
- [x] AC7 — 5.
- [x] AC8 — `scripts/deny.sh` exit 0; fixtures `all 8 fixture cases match`; `cargo-deny@0.20.2` at l.106.
- [x] AC9 — `~/.cargo/bin/cargo +1.85 check --locked` exit 0.
- [x] AC10 — README l.33 names `scripts/ci.sh`; ADR exists.
- [ ] AC11 — deferred to the approver after merge (unchanged).
- [x] AC12 — `git diff --stat 7fc0912 -- crates deny.toml scripts/deny.sh spikes` → empty.

Checks:
- `actionlint .github/workflows/ci.yml` → pass
- `scripts/ci.sh` → pass
- `scripts/deny.sh` → pass
- `tools/deny-fixtures/run.sh` → pass
- `crates/aegis-model`: `cargo fmt --check` → pass
- `crates/aegis-model`: `cargo clippy --all-targets -- -D warnings` → pass
- `crates/aegis-model`: `cargo test` → pass
- `crates/aegis-model`: `cargo +1.85 check --locked` → pass
- extra: `shellcheck -s sh scripts/ci.sh` → pass

Notes:
- I cannot test the cache restore/save path itself without a run on GitHub. Since I do not push, AC11's first run on master will be the first real save. After that, the second run (a PR or branch) should log a restore from `cargo-Linux-1.99.1-...`.
- Not pushed; master untouched.
