# ADR 0032: Offline license files

- Status: proposed (job 034-license-tokens)
- Date: 2026-10-25
- Scope: `crates/aegis-license`, `crates/aegis-server` (gate), `crates/aegis-client` (notices), `aegis.proto` (`LicenseInfo`)
- Rules: A1, R3, R4, R6, C2, D5 in `docs/engineering-standards.md`; `direction-v1.md` section 12
- Builds on: `0026-server-skeleton.md`

## Context

Direction section 12 asks for a signed license file, checked offline, with a
seat limit and a grace period, so air gapped sites work and engineering work
never stops suddenly.

## Decisions

### 1. File format

One ASCII line: `AEGIS-LICENSE-1.<payload>.<signature>`. The payload is JSON of
`License` (customer, seats, issued_at, expires_at, grace_days, sorted features,
key ID, license ID). The signature is Ed25519 over `"aegis-license-v1\0" +
payload bytes`. Signing the exact bytes avoids any canonical JSON problem. The
prefix stops a signature made for another purpose from passing as a license.

Parsing is strict and bounded (R3): 16 KiB file, `deny_unknown_fields`, one
format version, at most 64 features, names limited to a safe alphabet, no
control characters. The verifier runs `check()` after the signature and the
issuer runs it before signing, so the two cannot disagree.

### 2. Crypto without a new dependency

Ed25519 comes from `ring`, already audited for login (ADR 0028). No new crate
needs a D5 review. Signing lives behind the `issuer` feature and a separate
binary, so server and client builds carry no signing code.

### 3. Trusted keys are part of the build

`keys/vendor.keys` lists up to 8 public keys (key ID and 32 bytes). A license
names the key that signed it, so a key can be rotated without breaking old
licenses. The file ships empty. The owner adds the production key before the
first release (checklist). With no key, every license is refused. That is the
safe failure.

### 4. Dates are one pure function

`License::state(now)` returns `NotYetValid`, `Active`, `Grace` or `Expired`.
A start up to one day in the future is allowed for clock skew. Grace lasts
`grace_days` (at most 90). The clock is an argument, so the edges are tested
exactly.

### 5. What the server does

- **Start:** refuses without a valid license (`AEGIS_LICENSE_FILE`). A license
  in grace starts, with a warning. Only a build with the `dev-license` feature
  can skip the check (`AEGIS_DEV_LICENSE=1`), and the checklist forbids that
  feature in a release.
- **Running:** the gate decides one thing, "may this user take the edit
  lease?" Reading, listing and watching are never gated, so a lapsed license
  never locks people out of their own data. Expired gives `license_expired`
  (FAILED_PRECONDITION). A full ledger gives `license_seats`
  (RESOURCE_EXHAUSTED).
- **Seats:** a seat is a named user who asked for the edit lease within 30
  days. The ledger holds at most `seats` names (R4) and frees a seat only when
  someone else needs it. Taking a seat on the request, before the lease is
  granted, means the limit can never be exceeded, at the cost that a user
  refused with `lock_held` still counts as having tried to edit.
- **Telling people:** `GetServerInfo` carries `LicenseInfo`. The client shows
  "Licensed to X, n of m seats" in the status bar, and raises a notice when
  the license ends within 30 days, is in grace, or is over.

### 6. Tests

Real Ed25519 keys throughout. Property tests: no single byte change to a
license file gives a different valid license, and arbitrary text never
panics. Server tests run the gate over real gRPC with a moving clock. Client
tests run end to end against a licensed server.

## Consequences

- The seat ledger is in memory. A restart gives everyone a fresh window. It
  can be rebuilt from the audit log, which records every actor and time
  (backlog 039).
- The license cannot stop a determined customer who patches the binary. It
  states the agreement and stops accidents. Stronger protection would need
  phone-home, which direction section 12 rules out.
- Revocation is not possible offline. Short terms and renewals are the
  control.
