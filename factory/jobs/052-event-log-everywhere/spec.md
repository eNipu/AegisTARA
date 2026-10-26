# Spec — 052-event-log-everywhere

## Title
Unified security event log across OIDC, password, and dev servers with age and row-count retention

## Design Reference
- Reference: `docs/adr/0040-audit-event-log.md`
- Affected crates/paths: `crates/aegis-server/src/accounts/events.rs`, `crates/aegis-server/src/accounts/postgres.rs`, `crates/aegis-server/src/service.rs`

## Problem & Goal
Ensures the persistent security event log (`EventLog`, `aegis_event`) runs on every server mode (company-login-only, password, hybrid, and dev), enforcing retention by both row count (`max_events = 10,000`) and age (`retention = 90 days`), verified by a high-volume stress test.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0040-audit-event-log.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Bounds disk consumption under event floods while recording project creation/deletion, lock breaks, baselines, and exports across all auth modes. (See `docs/security-and-architecture-review.md` SEC-06 for governance-event partitioning recommendation).
3. UX invariants hold: OIDC-only enterprise deployments get the same administrative audit trail as password/hybrid deployments.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
