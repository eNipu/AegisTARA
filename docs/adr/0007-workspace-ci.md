# ADR 0007: Continuous integration for the production crates

- Status: proposed (job 008-workspace-ci)
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
toolchain plus an MSRV 1.85 job; `ubuntu-24.04` only; the four third-party
actions listed below, each pinned by SHA; the builder does not push, and the
approver checks the first green run after merge.

## Options

| Question | Options | Chosen |
|---|---|---|
| How to cover several crates | Cargo workspace; **per-crate loop in a script** | Loop (`scripts/ci.sh`) |
| Toolchain | Floating `stable`; **one pinned version**; `rust-toolchain.toml` | Pinned in the workflow `env` |
| Action references | Tags (`@v4`); **full commit SHAs** | SHAs, tag in a comment |
| Build cache | None; `actions/cache` by hand; **`Swatinem/rust-cache`** | `Swatinem/rust-cache` |
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

The `msrv` job installs `RUST_MSRV: "1.85"` (the `rust-version` in each
crate's `Cargo.toml`) and runs `cargo check --locked` in each crate. It checks
the library only: dev-dependencies such as `divan` and `proptest` may need a
newer compiler, and they never ship. The loop uses the same glob as
`scripts/ci.sh`. If this job fails after a dependency update, the fix is to
hold the dependency back or to raise `rust-version` on purpose, never to drop
the job.

## Decision 3: four actions, each pinned to a commit SHA

| Action | Tag | Commit |
|---|---|---|
| `actions/checkout` | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1` |
| `dtolnay/rust-toolchain` | v1 | `683b588c487a69d01f6c6ddf40f036ff682cc2c8` |
| `Swatinem/rust-cache` | v2.9.3 | `d1527c8f67ef9af357290475d4af4b0fab8f12e0` |
| `taiki-e/install-action` | v2.87.29 | `8cca0918f8471c33515202b73f56ab27f2bed283` |

A tag can be moved to other code at any time; a commit SHA cannot. Each
`uses:` line carries the tag as a comment so an update is readable in review.
`v2.9.3` of `rust-cache` is an annotated tag; the SHA above is the commit it
points to, not the tag object.

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

## Decision 4: `Swatinem/rust-cache`, written only on master

`rust-cache` caches `~/.cargo` (registry, git) and the `target/` of each
crate, keyed by the toolchain and the lock files. Its `workspaces` input takes
one `<crate dir> -> target` line per crate. A small step builds that list with
the same glob as `scripts/ci.sh`, so a new crate is cached with no workflow
edit.

`save-if` is true only on `refs/heads/master`. Branches and pull requests
restore the master cache but do not save their own, so short-lived branches do
not evict it from the 10 GB repository limit. The `deny` job needs no cache:
cargo-deny reads `Cargo.lock` and metadata and does not compile. The `msrv`
job compiles only the library and its two runtime dependencies (`thiserror`,
`uuid`), about 3 s locally, so it is not cached either.

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

Local, Apple M2 Max (12 cores), macOS, Rust 1.99.1, one crate
(`aegis-model`), dependencies already downloaded, 2026-10-20:

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
`check` job to take roughly 1 to 3 minutes. The measured time of the first GitHub run is recorded
here by the approver after merge (spec AC11).

## Consequences

- A broken gate shows as a red check on the commit or pull request, with an
  annotation naming the crate and the step.
- `scripts/ci.sh` is the one local command before a push.
- Bumping `RUST_TOOLCHAIN` or an action SHA is a deliberate, reviewed commit.
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
rustup toolchain install 1.85 --profile minimal
(cd crates/aegis-model && cargo +1.85 check --locked)
```
