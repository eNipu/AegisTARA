# ADR 0007: Continuous integration for the production crates

- Status: proposed (job 008-workspace-ci, rework 1: `Swatinem/rust-cache`
  replaced by `actions/cache` under D5)
- Date: 2026-10-20
- Scope: every crate under `crates/` (production code). `spikes/` is not checked.
- Rules: Q1, Q7, R6, L1, L3 in `docs/engineering-standards.md`; D5 in
  `docs/decisions.md`
- Builds on: `0003-cargo-deny-policy.md` (`deny.toml`, `scripts/deny.sh`)
- Files: `.github/workflows/ci.yml`, `scripts/ci.sh`

## Context

Standards Q1 and Q7 say the code MUST pass `cargo fmt --check`, `cargo clippy
-- -D warnings`, and `cargo-deny` "in CI", and Q7 asks for a pinned minimum
supported Rust version. Decision D5 says `cargo-deny` runs in CI. Until this
job, all of these were commands a developer had to remember. ADR 0003 left the
workflow for later and made `scripts/deny.sh` the single entry point CI would
call.

The repo has one production crate (`crates/aegis-model`), with its own
`Cargo.lock` and `target/`. There is no Cargo workspace yet.

The owner confirmed the spec's open questions on 2026-10-20 (`rework.md`):
`crates/*/` only, found by glob, with `tools/deny-fixtures/run.sh` in the deny
job; all four cargo-deny checks block, advisories included; one pinned stable
toolchain plus an MSRV 1.85 job; `ubuntu-24.04` only; third-party actions
pinned by SHA; the builder does not push, and the approver checks the first
green run after merge.

Rework 1 (owner, 2026-10-20): D5 is strict and has no exception for CI-only
tools. Round 1 used `Swatinem/rust-cache`, which is LGPL-3.0 and so "Not
allowed" under D5. It is replaced by `actions/cache` (MIT), and every action
in the workflow is now checked against D5 (Decision 3).

## Options

| Question | Options | Chosen |
|---|---|---|
| How to cover several crates | Cargo workspace; **per-crate loop in a script** | Loop (`scripts/ci.sh`) |
| Toolchain | Floating `stable`; **one pinned version**; `rust-toolchain.toml` | Pinned in the workflow `env` |
| Action references | Tags (`@v4`); **full commit SHAs** | SHAs, tag in a comment |
| Build cache | None; **`actions/cache` with explicit paths** (MIT); `Swatinem/rust-cache` (LGPL-3.0, not allowed by D5) | `actions/cache` restore and save |
| cargo-deny install | `cargo install` (3 to 5 min); **prebuilt binary with checksum** | `taiki-e/install-action` |
| Runners | Linux, Windows, macOS matrix; **Linux only** | `ubuntu-24.04` |

## Decision 1: a per-crate loop, not a workspace (yet)

`scripts/ci.sh` finds each `crates/*/Cargo.toml` in sorted glob order (the
same rule as `scripts/deny.sh`) and runs, inside each crate:

1. `cargo fmt --check`
2. `cargo clippy --locked --all-targets -- -D warnings`
3. `cargo test --locked`

`--locked` fails when `Cargo.lock` is out of date, so CI builds exactly the
committed dependency set. The script keeps going after a failure, prints one
`ok` or `FAIL` line per step, prints the output of failing steps only, and
exits 1 if any step failed or no crate was found. Under GitHub Actions it
folds each crate into a log group and turns each failing step into an error
annotation, so the run summary names the crate and the step.

A workspace with a shared `target/` would build shared dependencies once, but
it changes every crate's layout and lock file. That is a separate job. The
loop is the bridge: a new crate under `crates/` is checked with no workflow
edit, and the workspace job only has to replace the loop body.

The developer runs the same script locally, so a green `scripts/ci.sh` means a
green `check` job (same commands, same flags).

Cost: three cargo runs per crate, O(crates x build time).

## Decision 2: one pinned stable toolchain, plus an MSRV job

The workflow sets `RUST_TOOLCHAIN: "1.99.1"` (the current stable release on
2026-10-20) in one `env` variable, used by the `check` and `deny` jobs. A
floating `stable` would pull in new clippy lints every six weeks and turn CI
red on unrelated pushes. A bump is a deliberate one-line commit that also
fixes any new lints.

No `rust-toolchain.toml` is added, so local toolchains do not change. Local
runs on a newer compiler may show lints that CI does not yet see; that is the
signal to bump the pin.

The `msrv` job installs `RUST_MSRV: "1.99.1"` (the `rust-version` in each
crate's `Cargo.toml`, updated from `1.85` to `1.99.1` on 2026-10-24) and runs
`cargo check --locked` in each crate. It checks
the library only: dev-dependencies such as `divan` and `proptest` may need a
newer compiler, and they never ship. The loop uses the same glob as
`scripts/ci.sh`. If this job fails after a dependency update, the fix is to
hold the dependency back or to raise `rust-version` on purpose, never to drop
the job.

## Decision 3: four actions, each pinned to a commit SHA and allowed by D5

| Action | Tag | Commit | License |
|---|---|---|---|
| `actions/checkout` | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1` | MIT |
| `dtolnay/rust-toolchain` | v1 | `683b588c487a69d01f6c6ddf40f036ff682cc2c8` | MIT |
| `actions/cache` (`restore` and `save`) | v6.1.0 | `55cc8345863c7cc4c66a329aec7e433d2d1c52a9` | MIT |
| `taiki-e/install-action` | v2.87.29 | `8cca0918f8471c33515202b73f56ab27f2bed283` | Apache-2.0 |

A tag can be moved to other code at any time; a commit SHA cannot. Each
`uses:` line carries the tag as a comment so an update is readable in review.

**Every action in the workflow is checked against D5**, the same as a crate
dependency. D5 has no exception for tools that only run in CI (owner, rework
1). The licenses above are from the GitHub license API
(`gh api repos/<owner>/<repo>/license`, 2026-10-20), and all four are on the
D5 "Allowed" list. Round 1 used `Swatinem/rust-cache`, which is LGPL-3.0 and
"Not allowed", so it was removed (Decision 4). cargo-deny does not see
workflow files, so this check is manual: a new or updated `uses:` line must
name an action whose license is on the D5 allow list, and the reviewer checks
it.

Other hardening in the workflow:

- `permissions: contents: read` at the top level. No job writes.
- `actions/checkout` with `persist-credentials: false`, so the token is not
  left in `.git/config` for later steps.
- Triggers are `push` and `pull_request` only. No secrets, no privileged pull
  request trigger, and no `${{ }}` event data inside `run:` scripts.
- `taiki-e/install-action` installs the prebuilt `cargo-deny@0.20.2` (the
  version ADR 0003 pins), checks it against the SHA-256 in the action's
  manifest, and has `fallback: none`, so it can never fall back to building
  an unpinned version from source.
- `concurrency: ci-${{ github.ref }}` with `cancel-in-progress`, so a new push
  cancels the older run on the same ref.
- `timeout-minutes` on every job (R6): `check` 30, `deny` 15, `msrv` 20.

## Decision 4: `actions/cache` with explicit paths, written only on master

The `check` job restores one cache entry with `actions/cache/restore` and
saves it with `actions/cache/save`. The entry holds:

- `$CARGO_HOME/registry/index/`, `$CARGO_HOME/registry/cache/`, and
  `$CARGO_HOME/git/db/` (downloaded sources, not the unpacked `registry/src/`,
  which cargo rebuilds from `cache/`);
- `crates/<crate>/target/` for every crate.

A small step builds that path list with the same `crates/*/Cargo.toml` glob as
`scripts/ci.sh`, so a new crate is cached with no workflow edit.

The key is `cargo-<runner OS>-<RUST_TOOLCHAIN>-<hashFiles('crates/*/Cargo.lock')>`.
The glob in `hashFiles` also covers a new crate's lock file. The restore key
`cargo-<runner OS>-<RUST_TOOLCHAIN>-` falls back to the newest entry for the
same OS and toolchain when a lock file changes; cargo rebuilds what changed. A
toolchain bump starts a fresh cache, because artifacts from another compiler
are useless.

The save step runs only when the ref is `refs/heads/master`, `scripts/ci.sh`
passed, and the restore was not an exact key hit (entries are immutable, so
saving the same key again would only fail). Branches and pull requests
restore the master entry but never save their own, so short-lived branches
cannot evict it from the 10 GB repository limit or poison what master
restores. This is the same behaviour round 1 had with `rust-cache`'s
`save-if`.

`CARGO_INCREMENTAL: 0` is set for the workflow. `rust-cache` set it
implicitly; with plain `actions/cache` it has to be explicit. Incremental
artifacts only help local edit loops and would make the cached `target/`
larger.

Known limit: unlike `rust-cache`, nothing prunes stale artifacts from
`target/` before saving. A new entry is written only when a lock file or the
toolchain changes, and a toolchain bump starts from empty, so growth is
bounded by the dependency changes between bumps. If the entry grows too
large, add a prune step (a later job).

The `deny` job needs no cache: cargo-deny reads `Cargo.lock` and metadata and
does not compile. The `msrv` job compiles only the library and its two runtime
dependencies (`thiserror`, `uuid`), about 3 s locally, so it is not cached
either.

## Decision 5: Linux only, all four cargo-deny checks blocking

Only `ubuntu-24.04`. The product ships Windows first (D2), but no crate has
OS-specific code yet. Adding `windows-latest` is one matrix line on the
`check` job once the first OS-specific code lands.

The `deny` job runs `scripts/deny.sh` with no arguments, so `advisories` is
included and fetches the RustSec database. A new advisory can turn CI red with
no code change; the owner chose that (rework 1, question 2): the red check is
the alert. `tools/deny-fixtures/run.sh` runs in the same job, offline, so an
edit to `deny.toml` that weakens the D5 license policy also fails CI.

## Numbers

Local, Apple M2 Max (12 cores), macOS, Homebrew Rust 1.99.1
(`/opt/homebrew/bin/cargo`, the first `cargo` on PATH, with clippy 0.1.99 and
rustfmt 1.10.0; the same version CI pins), one crate (`aegis-model`),
dependencies already downloaded, 2026-10-20:

| Command | Wall time |
|---|---|
| `scripts/ci.sh`, cold `target/` (`CARGO_TARGET_DIR` empty) | 23.9 s |
| `scripts/ci.sh`, warm `target/` | 23.1 s |
| `scripts/deny.sh` (all four checks, advisory DB already fetched) | 1.2 s |
| `tools/deny-fixtures/run.sh` | 1.9 s |
| `cargo +1.85 check --locked` in `crates/aegis-model` | 2.2 s to 3.3 s |

Warm and cold are close because `cargo test` dominates: the property tests
(job 007) and the timing tests run for about 10 s whatever the cache state.
A standard GitHub `ubuntu-24.04` runner has 2 to 4 vCPUs, so expect the
`check` job to take roughly 1 to 3 minutes.

GitHub, `ubuntu-24.04`, all three jobs green (spec AC11):

| Run | Commit | Cache | `check` | `deny` | `msrv` | Whole run |
|---|---|---|---|---|---|---|
| [39511008327](https://github.com/eNipu/AegisTARA/actions/runs/39511008327), 2026-10-20 | `657e775` (job 008) | miss; saved 138 MiB on master | 83 s | 21 s | 20 s | about 85 s |
| [39522231668](https://github.com/eNipu/AegisTARA/actions/runs/39522231668), 2026-10-21 | `3f41d56` (job 009) | exact key hit | 54 s | 21 s | 20 s | about 55 s |

The second run tests far more code (job 009 added about 360 tests), yet the
cache hit cut the `check` job by about a third. The cache key did not change
between the two runs because no `Cargo.lock` changed.

## Consequences

- A broken gate shows as a red check on the commit or pull request, with an
  annotation naming the crate and the step.
- `scripts/ci.sh` is the one local command before a push.
- Bumping `RUST_TOOLCHAIN` or an action SHA is a deliberate, reviewed commit.
  Adding or updating an action also means checking its license against D5.
- A RustSec advisory can block unrelated pushes until it is handled.
- The per-crate loop rebuilds shared dependencies once per crate. That cost
  grows with the number of crates and is the reason for the later workspace
  job.

## Reproduce

```sh
actionlint .github/workflows/ci.yml
time scripts/ci.sh
scripts/deny.sh
tools/deny-fixtures/run.sh
rustup toolchain install 1.99.1 --profile minimal
(cd crates/aegis-model && cargo +1.99.1 check --locked)
# D5 license of every action in the workflow
for r in actions/checkout dtolnay/rust-toolchain actions/cache taiki-e/install-action; do
    echo "$r $(gh api "repos/$r/license" --jq .license.spdx_id)"
done
```
