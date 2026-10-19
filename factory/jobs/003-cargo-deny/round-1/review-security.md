VERDICT: PASS

Scope: configuration and shell tooling only (deny.toml, scripts/deny.sh, tools/deny-fixtures/**, README, ADR). No Rust source, no new Rust dependencies, no product code path that takes untrusted input. Q2/Q3/D7 do not apply.

Checked:
- deny.toml: allow list holds exactly the 11 D5 IDs; no deny list, no LicenseRef-/MPL/GPL/Unicode-DFS entries; `exceptions = []`, `[advisories] ignore = []`, `yanked = "deny"`. Sources: unknown registry and git denied, crates.io only. Wildcards denied (path deps exempt). Good supply-chain posture (Q7).
- scripts/deny.sh, tools/deny-fixtures/run.sh: `set -eu`, all expansions quoted, temp files from `mktemp` and cleaned up by trap, no eval, no user input interpolated into commands. Arguments pass straight to cargo-deny as check names. run.sh counts "fail" only on `error[rejected]`, so a broken config cannot look like a correct rejection.
- Fixtures: offline path deps only; Cargo.lock/target gitignored.
- Tool pin: cargo-deny 0.19.9 installed with `--locked` (MIT/Apache-2.0). It is a dev tool, not a crate dependency. D5 is fine.
- No secrets, tokens, or customer data in the diff.

Non-blocking notes (no change needed this round):
- deny.toml:37-40 — `[licenses.private] ignore = true` skips every `publish = false` crate in the graph, not only our own. A vendored path dependency marked `publish = false` would skip the license check. Today the only path crates are ours, and `[sources]` blocks unknown git or registries, so the risk is low. When a workspace or vendored code arrives, consider `ignore-sources` / limiting by name, or a review rule that vendored crates must not set `publish = false`. This could go in ADR 0003.
- scripts/deny.sh:29, run.sh:30 — the trap on INT/TERM removes the log but does not exit, so Ctrl-C moves on to the next crate. This is about robustness, not security. A possible fix: `trap 'rm -f "$log"; exit 130' INT TERM`, with a separate EXIT trap.
