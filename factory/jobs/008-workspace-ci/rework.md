## Answers

The owner answered the spec's open questions on 2026-10-20:

1. **Which crates:** confirmed. CI covers `crates/*/` only, found by glob. `spikes/` is excluded. `tools/deny-fixtures/run.sh` runs in the deny job.
2. **Advisories:** confirmed. All four `deny.sh` checks, including `advisories`, block the push. No non-blocking step and no weekly run.
3. **Toolchain:** confirmed. Pin one stable Rust version in a single `env` variable and name it in the ADR. Add no `rust-toolchain.toml`. Add the MSRV 1.85 `cargo check --locked` job (library only).
4. **Runner OS:** confirmed. `ubuntu-24.04` only for now.
5. **Third-party actions:** confirmed. `actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache` and `taiki-e/install-action`, each pinned to a full commit SHA with a version comment.
6. **Proof on GitHub:** confirmed. The builder proves the workflow locally and does not push. The approver checks AC11 (a green run on GitHub) after the merge is pushed.

## Rework 1

The owner's decision on the escalation in decision.md (2026-10-20): D5 stays strict and makes no exception for CI-only tools. Replace `Swatinem/rust-cache` (LGPL-3.0) with `actions/cache` (MIT), pinned to a full commit SHA with a version comment.

- Cache `~/.cargo/registry/index/`, `~/.cargo/registry/cache/`, `~/.cargo/git/db/` and each crate's `target/` directory.
- Key the cache on the runner OS, the pinned toolchain (`RUST_TOOLCHAIN`) and a hash of every crate's `Cargo.lock`, with a restore-key fallback.
- Keep the current behaviour: only master saves the cache (use `actions/cache/restore` and `actions/cache/save`, or an equivalent), and a new crate is picked up with no workflow edit.
- Update ADR 0007 to name `actions/cache` and its MIT license, and to state that every action in the workflow is checked against D5.
- Keep all other choices from round 1.
