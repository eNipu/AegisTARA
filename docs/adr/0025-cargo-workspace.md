# ADR 0025: One Cargo workspace for all production crates

- Status: proposed (job 027-workspace-and-readme)
- Date: 2026-10-25
- Scope: the repository root, `crates/*`, `scripts/ci.sh`, `scripts/deny.sh`, `.github/workflows/ci.yml`
- Rules: Q1, Q7, R10, L1 in `docs/engineering-standards.md`; D5 in `docs/decisions.md`
- Supersedes: "Decision 1: a per-crate loop, not a workspace (yet)" in `0007-workspace-ci.md`

## Context

ADR 0007 kept `crates/aegis-model` standalone and promised a workspace "before
crate #2". Crate #2 arrives with the server (job 029). Several crates now
need the same dependency versions (`serde`, `tokio`, `tonic`, ...) and one
lock file, or the client and server could silently resolve different versions
of the engine's dependencies. Engine parity (the same `aegis-model` build in
client and server) is a product requirement.

## Options

| Question | Options | Chosen |
|---|---|---|
| Lock file | one per crate; **one at the root** | Root `Cargo.lock` |
| Shared versions | repeat per crate; **`[workspace.dependencies]`** | Workspace table |
| Spikes, deny fixtures, fuzz crate | members; **excluded, own roots** | Excluded |
| CI loop | **keep the per-crate loop**; one `--workspace` run | Keep the loop |

## Decision

1. The root `Cargo.toml` is a virtual workspace with `members = ["crates/*"]`,
   `resolver = "3"`, and `exclude = ["spikes", "tools", "crates/aegis-model/fuzz"]`.
   Spikes are throwaway (L3), the deny fixtures need their own roots to prove
   the policy, and the fuzz crate keeps its own lock file and nightly needs.
2. `[workspace.package]` holds `edition`, `rust-version`, `publish`, `license`.
   `[workspace.dependencies]` holds every third-party version used by more
   than one crate and the path to `aegis-model`. A crate writes
   `name.workspace = true`.
3. Amended (2026-11-01): with no arguments `scripts/ci.sh` checks the whole
   workspace in one cargo run per step. Each crate built on its own unifies
   features on its own, so shared dependencies were rebuilt per crate and CI
   ran past its time limit. The MSRV job still checks each crate alone.
   Originally: `scripts/ci.sh` and `scripts/deny.sh` keep their per-crate loop. Inside a
   crate directory, `cargo fmt`, `cargo clippy` and `cargo test` act on that
   package only, and `cargo deny --manifest-path` checks that crate's
   dependency graph, so one failing crate is still reported by name. Only the
   shared `target/` and `Cargo.lock` moved to the root.
4. The CI cache holds the root `target/` and is keyed by the root `Cargo.lock`.
5. `[profile.release]` sets `lto = "fat"`, `codegen-units = 1` and
   `strip = "symbols"` (R10). Tests and benches use the dev profile, so
   test time is unchanged.

## Consequences

- `cargo build` and `cargo test --workspace` work from the repo root.
- A dependency bump is one edit in one table and one lock file diff.
- The per-crate loop costs one cargo start per step per crate; the shared
  `target/` means no crate is compiled twice.
