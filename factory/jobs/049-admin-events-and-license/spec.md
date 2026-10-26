# Spec — 049-admin-events-and-license

## Title
Admin security event log viewer with filtering and license details page

## Design Reference
- Reference: `docs/adr/0040-audit-event-log.md`
- Affected crates/paths: `crates/aegis-server/src/accounts/events.rs`, `crates/aegis-client`, `crates/aegis-shell/ui/app.slint`

## Problem & Goal
Adds the persistent admin event log (`ListEvents` RPC, `EventLog`) with filtering and a dedicated License view displaying customer, edition, seat usage, expiry date, and enabled feature flags.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0040-audit-event-log.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Restricts `ListEvents` to authenticated admins and strips control characters from all event fields (`clean_field`).
3. UX invariants hold: Gives administrators visibility into logins, exports, user changes, and license capacity in one place.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
