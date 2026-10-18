# ADR 0003: cargo-deny license and supply chain policy

- Status: accepted (job 003-cargo-deny)
- Date: 2026-10-19
- Scope: every crate under `crates/` (production code). `spikes/` is not checked.
- Rules: D5 in `docs/decisions.md`, Q7 in `docs/engineering-standards.md`
- Tool: cargo-deny 0.19.9 (`cargo install cargo-deny --version 0.19.9 --locked`)

## Context

AegisTARA is closed source (D5). It may only ship dependencies under
permissive licenses. Some licenses are "review first" (MPL-2.0, the Slint
royalty-free license, AI model and data catalog licenses) and the rest are
forbidden (GPL, LGPL, AGPL, SSPL, BUSL, Commons Clause, non-commercial).
Standard Q7 also requires checks for advisories, banned or duplicate crates,
and dependency sources. A dependency update can change a license at any time,
so the check has to be a command, not a review habit.

Files: `deny.toml` (policy), `scripts/deny.sh` (runs every crate),
`tools/deny-fixtures/run.sh` (proves the policy rejects what D5 forbids).

## Decision 1: allow list only, no deny list

| Option | New, unknown license |
|---|---|
| Deny list (GPL, AGPL, ...) plus a default of "allow" | Passes silently. |
| **Allow list (the 11 D5 IDs), everything else fails** | Fails until someone decides. |

**Decision: allow list only.** `deny.toml` has `licenses.allow` with exactly
the 11 SPDX IDs of D5 and no `deny` entries. Every license we have not thought
of (a new `LicenseRef-*`, a non-commercial variant, a typo) fails by default.
Review-first licenses are simply not on the list, so they fail like forbidden
ones until an exception is added (Decision 4).

SPDX expressions follow the normal rules: `A OR B` passes if one branch is
allowed (we pick the allowed branch, for example `GPL-3.0-only OR MIT`); `A
AND B` needs every part allowed. The fixtures cover both.

`OFL-1.1` is allowed globally, not only for font crates, because cargo-deny
cannot scope an allow entry to a kind of crate. Fonts are the only expected
users; any other OFL crate should be questioned in review.

`unused-allowed-license = "allow"`: the list is the D5 policy, not a record
of what we use today, so unused entries are expected and not warned about.

## Decision 2: `multiple-versions = "warn"`

`aegis-model` already locks two versions of `syn`, `getrandom` and `r-efi`
through upstream crates (uuid, proptest) we do not control. `deny` would fail
the build with no fix available on our side; `allow` would hide new
duplicates. `warn` keeps them visible in every run. All three duplicates enter through
the `proptest` dev-dependency, and cargo-deny skips dev-dependencies in the
duplicate check by default, so `multiple-versions-include-dev = true` is set;
without it the warnings would not appear at all. With it, cargo-deny 0.19.9
warns about `getrandom` and `r-efi`. `syn` 2 and 3 are both in `Cargo.lock`
(syn 2 only through the `zerocopy-derive` proc-macro under proptest) but
cargo-deny does not report that pair; it is harmless either way. Revisit when the workspace
exists and a shared lock file makes duplicates cheaper to remove.

`wildcards = "deny"` with `allow-wildcard-paths = true`: registry
dependencies must carry a version requirement; path dependencies (fixtures,
future workspace members) may omit it.

## Decision 3: private crates are ignored

Our own crates are `publish = false` with `license = "LicenseRef-Proprietary"`.
Adding `LicenseRef-Proprietary` to the allow list would also let any third
party crate with that string through. `[licenses.private] ignore = true` skips
only unpublished crates in the graph, which are ours, and keeps the allow list
free of `LicenseRef-*`.

Consequence for the fixtures: the dependency crates in
`tools/deny-fixtures/deps/` do not set `publish = false`, otherwise they would
be skipped and every case would pass.

## Decision 4: an exception names the crate and version

A review-first license passes only through an entry like

```toml
exceptions = [
    # Approved in D<N> (docs/decisions.md): <one line why>.
    { crate = "name@1.2.3", allow = ["MPL-2.0"] },
]
```

The version is part of the name on purpose. A reviewed license is a fact about
one release; a later release can change its license or its bundled code. With
the version pinned, an upgrade fails the check and forces a new look, which is
the review D5 asks for. The comment points to the decision that approved it, so
the reason is traceable. No exceptions exist today (Slint and MPL-2.0 are not
approved yet).

`[advisories] ignore` stays empty for the same reason: an advisory is fixed or
reviewed, never silenced without a record.

## Tool version

Pinned to cargo-deny **0.19.9**, the newest release whose `check` subcommand
still takes `--config`. In 0.20.x, `--config` moved to the top level
(`cargo deny --config deny.toml check`) and `check --config` is rejected.
Moving to 0.20 means changing `scripts/deny.sh`, `tools/deny-fixtures/run.sh`
and the README together. The config uses the current format and prints no
deprecation warnings under 0.19.9.

## Numbers

- `aegis-model`: `licenses bans sources` and `advisories` pass (2026-10-19).
- Fixtures: 8 of 8 cases match (2 pass, 6 fail with `error[rejected]`).
- Removing `MIT` from `allow` makes `ok-mit` and `ok-or` mismatch; `run.sh`
  exits 1, so the runner can fail.
- Cost: one cargo-deny run per crate, O(D) in its locked dependencies, plus
  one advisory database fetch. Seconds, not on any product path.
