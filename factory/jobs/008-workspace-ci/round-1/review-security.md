VERDICT: PASS

Scope reviewed: `git diff master...factory/008-workspace-ci` (`.github/workflows/ci.yml`, `scripts/ci.sh`, `README.md`, `docs/adr/0007-workspace-ci.md`), with spec.md and rework.md. This is round 1, so there are no earlier security reviews. No Rust code, no `unsafe`, no hash maps, and no new crate dependencies (Q2, Q3, D7 do not apply).

Checked and fine:
- Workflow privilege: top-level `permissions: contents: read`, `persist-credentials: false` on every checkout, no secrets, no `pull_request_target`. Fork PRs run with a read-only token.
- Script injection: no `${{ }}` expression appears inside any `run:` block. `github.ref` is used only in `concurrency.group` and in rust-cache `save-if` (`with:` input), which do not reach a shell.
- Cache poisoning: `save-if: github.ref == 'refs/heads/master'`, so PR refs (`refs/pull/N/merge`) and feature branches cannot write the cache that master restores.
- Supply chain: all four `uses:` are pinned to 40-character SHAs with version comments. I checked `dtolnay/rust-toolchain` 683b588c... against current `master` HEAD and it matches. `taiki-e/install-action` uses `fallback: none`, so cargo-deny is never built from an unverified source.
- Limits (R6): `timeout-minutes` on all three jobs (30/15/20), and `cancel-in-progress` concurrency. No unbounded queues (R4).
- `scripts/ci.sh`: `set -eu`, a `mktemp` log removed by an EXIT/INT/TERM trap, all variables quoted, and commands run as argv (`"$@"`) with no `eval`. `$GITHUB_OUTPUT` and `::error`/`::group::` lines contain only crate directory names from the repo, not untrusted input.
- No secrets, tokens, or customer data in the code, docs, or build log.

Non-blocking note (approver decision, not a code defect):
- .github/workflows/ci.yml:62 — `Swatinem/rust-cache` is licensed LGPL-3.0 (GitHub license API), and D5 lists LGPL as "Not allowed". The owner approved this action by name in rework.md answer 5, and it runs only on the CI runner (it is not linked or shipped), so no distribution obligation applies. ADR 0007 does not mention the license. Fix (choose one): add a line to ADR 0007 saying D5 covers shipped dependencies and CI-only tools are out of its scope, or replace it with `actions/cache` (MIT) and explicit `~/.cargo` and `crates/*/target` paths. The other three actions are MIT, MIT, and Apache-2.0.
