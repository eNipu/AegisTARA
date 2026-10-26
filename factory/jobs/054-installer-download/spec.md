# Spec — 054-installer-download

## Title
Streaming size-capped, SHA-256-verified update installer download (`UpdateOffer::download`)

## Design Reference
- Reference: `docs/adr/0048-verified-update-download.md`
- Affected crates/paths: `crates/aegis-auth/src/http.rs`, `crates/aegis-client/src/updates.rs`, `crates/aegis-client/src/session.rs`, `crates/aegis-shell/ui/app.slint`

## Problem & Goal
Streams the update installer over HTTPS (`HttpDownload::download`, `UpdateOffer::download`) into an exclusive `<name>.<unique>.part` file (`create_new(true)`, `0700` private dir in temp), enforcing the signed size cap and SHA-256 checksum on the fly before `sync_all()` and atomic rename.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0048-verified-update-download.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Prevents symlink/TOCTOU attacks in shared folders, aborts immediately if bytes exceed the signed manifest size, deletes `.part` on any error, and never auto-executes the downloaded binary.
3. UX invariants hold: One-click verified installer download after an update check.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
