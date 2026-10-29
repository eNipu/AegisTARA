# Spec — 060-login-hardening

## Title
Login hardening: single-entry throttle eviction (`SEC-01`), monotonic DPoP replay eviction (`SEC-03`), and per-address Argon2id admission (`SEC-04`) (`ADR 0054`)

## Design Reference
- Reference: `docs/adr/0054-security-hardening-sweep.md`
- Affected crates/paths: `crates/aegis-auth/src/dpop.rs`, `crates/aegis-server/src/accounts/{mod,service}.rs`

## Problem & Goal
Fix three security review findings in authentication:
1. `SEC-01`: Prevent an attacker who sprays 10,000 random usernames from clearing active lockouts on known accounts in `Throttle`.
2. `SEC-03`: Prevent a DPoP proof with a future-skewed `iat` at the head of the FIFO queue from blocking expiry eviction of later proofs in `ReplayCache`.
3. `SEC-04`: Place a per-address admission gate (`AddrThrottle`) in front of the `LogIn` Argon2id semaphore and isolate authenticated password-hashing permits from unauthenticated login floods.

## Acceptance Criteria
1. `Throttle::fail` evicts a single entry when full (preferring unknown and unblocked usernames) and never calls `self.names.clear()`.
2. `ReplayCache::check_and_insert` computes expiry monotonically in server arrival time (`now + window + skew`) and sweeps expired entries on capacity pressure.
3. `AddrThrottle` limits concurrent in-flight logins per peer IP (`max_login_in_flight_per_addr = 2`) and enforces per-address backoff before acquiring a `LogIn` Argon2id permit; authenticated operations use a separate `auth_hashing` pool.
4. Regression tests cover `SEC-01`, `SEC-03`, and `SEC-04`.
