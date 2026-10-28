# Security Review — 110-deny-dev-policy (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## Security Evaluation
- Shipped binaries remain strictly gated by the permissive SPDX allow-list (`MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unicode-3.0`, `Zlib`), verified by `bad-gpl-also-dev` failing whenever a copyleft crate enters `[dependencies]`.
- Advisories, bans (wildcard denies), and git/registry source restrictions still apply to all crates including `[dev-dependencies]` and `crates/aegis-model/fuzz`.
