# ADR 0076: Repository License (source-available, noncommercial)

- **Status:** Accepted
- **Date:** 2026-11-24
- **Related:** ADR 0003 (cargo-deny policy), `deny.toml`, root `Cargo.toml`, `LICENSE`

## Context

The GitHub repository `eNipu/AegisTARA` is public but had no `LICENSE` file,
so GitHub reports "no license" and the default is all-rights-reserved: anyone
may look at the code, but nobody is granted any explicit right to run, modify,
or share it. The owner wants the opposite signal: the repository stays public
for viewing and for showing to prospective customers, while no one else may
use it commercially. The product itself is commercial (customer-hosted
sovereign deployments with signed license files, `crates/aegis-license`), so
the owner must keep full commercial rights. Until now the workspace crates
were marked `license = "LicenseRef-Proprietary"` with `publish = false`.

## Options considered

| License | Blocks commercial use by others? | Notes |
|---|---|---|
| None (all rights reserved, status quo) | Yes, but grants nothing | GitHub shows "no license"; no explicit viewing/evaluation grant; ambiguous for customers |
| MIT / Apache-2.0 / BSD | No | Permit commercial use and sublicensing |
| AGPL-3.0 / SSPL-1.0 | No | Commercial use allowed under copyleft/service conditions; wrong direction |
| Elastic License 2.0 | No | Allows commercial use; only blocks offering it as a hosted/managed service and tampering with license keys |
| Commons Clause | Nominally | Non-standard rider bolted onto an OSS license; not a standalone SPDX license; murky enforceability |
| CC BY-NC / BY-NC-ND / BY-NC-SA 4.0 | Yes, but | Drafted for creative works, not code; `-ND` blocks modifications; OSI advises against CC licenses for software |
| BUSL-1.1 | Yes, temporarily | Blocks production/commercial use until the Change Date, then auto-converts to a Change License (e.g. Apache-2.0). Fits a *planned* future open-sourcing, not an indefinitely personal/commercial project |
| **PolyForm Noncommercial 1.0.0** | **Yes** | Source-available; every noncommercial purpose is permitted (incl. charities, education, government); anything commercial is outside the grant; SPDX-listed; patent grant, 32-day cure period, no-liability clause |

## Decision

License the whole repository under the **PolyForm Noncommercial License 1.0.0**
(SPDX ID `PolyForm-Noncommercial-1.0.0`) via a root `LICENSE` file. Every
in-repo crate (the workspace, `tools/aegis-ai`, the fuzz crate, the spikes)
declares `license = "PolyForm-Noncommercial-1.0.0"` in its `Cargo.toml`
instead of `LicenseRef-Proprietary`.

## Consequences

- Viewing, studying, modifying, and noncommercial redistribution are explicitly
  allowed; commercial use requires a separate grant from the author. As the
  copyright holder the author is not bound by this license, so commercial
  deals (customer-hosted deployments, signed license files from
  `crates/aegis-license`) continue unchanged — this is dual licensing.
- `deny.toml` is unaffected: its allow list governs third-party dependencies
  only, and `[licenses.private] ignore = true` already skips our own
  `publish = false` crates whatever their license string is. If one of our
  crates is ever published, the noncommercial SPDX ID fails the allow list
  until an exception names the approving decision — the intended behavior.
- If the project is ever meant to become fully open source, the owner can
  relicense (the owner is not bound by the current license) or switch to
  BUSL-1.1 with a Change Date; nothing in the dependency policy prevents that.
- GitHub detects the license from the root `LICENSE` file; the README links
  to it in a dedicated License section.
