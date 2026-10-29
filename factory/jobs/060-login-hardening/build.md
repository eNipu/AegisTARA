# Build Report — 060-login-hardening

## Summary
Implemented `SEC-01` (single-entry eviction in `Throttle`), `SEC-03` (monotonic server-arrival expiry in `ReplayCache`), and `SEC-04` (`AddrThrottle` in front of `LogIn` Argon2id permits and dedicated `auth_hashing` pool), documented in ADR 0054 (`docs/adr/0054-security-hardening-sweep.md`).

## Key Changes
- `crates/aegis-auth/src/dpop.rs`: Monotonic `expires = now.saturating_add(self.window).saturating_add(self.skew)` and expired-entry sweep in `ReplayCache::check_and_insert`.
- `crates/aegis-server/src/accounts/mod.rs` & `service.rs`: Single-entry eviction in `Throttle::fail` (preferring unknown/unblocked names), early rejection of syntactically invalid usernames, `AddrThrottle` (`max_login_in_flight_per_addr = 2`, `free_addr_failures = 20`), and separate `auth_hashing` semaphore.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
