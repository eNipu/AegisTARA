# AegisTARA

A learning project: a Threat Analysis and Risk Assessment (TARA) tool, written fully in Rust.

- A native desktop client talks to a server that each customer company hosts in its own cloud.
- Users log in with their company credentials.
- Automotive first (ISO/SAE 21434, UN R155). Other methodologies come later as data packs.
- Visual attack trees with feasibility scoring, explain-why for every number, versions and baselines, and executive reports.
- AI features are planned in the backlog.

Status: direction set. No code yet.

## Documents

1. [docs/decisions.md](docs/decisions.md): decisions made so far. Start here.
2. [docs/direction-v1.md](docs/direction-v1.md): desktop client, customer-hosted server, login, v1 scope.
3. [docs/ai-backlog.md](docs/ai-backlog.md): how AI features fit, and in which order.
4. [docs/engineering-standards.md](docs/engineering-standards.md): coding requirements for performance, data structures, algorithms, and learning.
5. [spikes/RESULTS.md](spikes/RESULTS.md): UI toolkit spike results (Slint chosen).
6. [docs/brainstorm.md](docs/brainstorm.md): the full first brainstorm.

## Dependency licenses

Production crates (`crates/*`) must pass the `cargo-deny` policy in
[`deny.toml`](deny.toml) (decision D5, standard Q7): licenses, advisories,
bans, and sources. Rationale: [docs/adr/0003-cargo-deny-policy.md](docs/adr/0003-cargo-deny-policy.md).

Install the pinned version (cargo-deny 0.20.2):

```sh
cargo install cargo-deny --version 0.20.2 --locked
```

Run it from the repo root:

```sh
scripts/deny.sh                          # all checks, every crate (fetches the advisory database)
scripts/deny.sh licenses bans sources    # offline
tools/deny-fixtures/run.sh               # proves the policy rejects what D5 forbids

# one crate by hand (cargo-deny 0.20: --config goes before `check`)
cargo deny --manifest-path crates/aegis-model/Cargo.toml --config deny.toml check
```

Only the 11 licenses allowed by D5 pass. To accept a "review first" license
(for example MPL-2.0), first record the approval as a decision in
`docs/decisions.md`, then add one exception per crate and version to
`[licenses] exceptions` in `deny.toml`, with a comment naming that decision:

```toml
exceptions = [
    # Approved in D<N>: <reason>.
    { crate = "name@1.2.3", allow = ["MPL-2.0"] },
]
```

Never widen `allow` for a review-first license, and never add a `deny` list.
