VERDICT: PASS

Scope: round 2 changes the cargo-deny pin (0.19.9 -> 0.20.2), moves `--config` to a top-level option, hardens the trap handling and updates the README and ADR text. No Rust source, no new crate dependencies, no product code path that takes untrusted input. Q2, Q3 and D7 do not apply.

Checked:
- deny.toml: the policy is unchanged apart from the pin comment. The allow list still has exactly the 11 D5 IDs. There is no deny list. `exceptions = []`, `[advisories] ignore = []` and `yanked = "deny"`. Only crates.io is allowed; unknown registries and unknown git sources are denied. Wildcards are denied, with path dependencies exempt (Q7).
- scripts/deny.sh and tools/deny-fixtures/run.sh: the round-1 note is fixed. There is now an EXIT trap plus `trap 'rm -f "$log"; exit 130' INT TERM`, so an interrupt stops the run and removes the temp file. Both scripts still use `set -eu`, quote every expansion, create temp files with `mktemp` and have no `eval`. Arguments go to cargo-deny as separate argv entries. run.sh still requires `error[rejected]` before it counts a case as fail.
- The round-1 note about `[licenses.private] ignore = true` is now recorded as a known gap in ADR 0003 Decision 3, with follow-ups named. Today nothing more is needed.
- Tool pin: cargo-deny 0.20.2 installed with `--locked`. Its license is MIT/Apache-2.0. It is a dev tool, not a crate dependency, so D5 is satisfied.
- No secrets, tokens or customer data in the diff.

No findings.
