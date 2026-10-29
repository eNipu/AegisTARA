# Build — 058-project-access

## What was built
- `crates/aegis-server/src/access.rs`: role table, permission resolution, member-key validation, and unit tests.
- `Principal.groups` populated from OIDC `groups`/`roles` claims (ACC-02).
- `ProjectStore` membership methods on Memory, Folder (manifest `memberships`, backward compatible), and Postgres (migration `006_project_access.sql`).
- `AegisService`: `load_access_cache`, `require_access` checks on every project-scoped RPC, and `ListMemberships`/`SetMembership`/`RemoveMembership`.
- `CreateProject` grants the creator `project_owner`; last-owner protection.
- Audit kinds `membership_changed` and `membership_removed`.

## Verified
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.
- `cargo test --workspace` (ran unsandboxed; loopback bind and OS trust store are blocked by the sandbox).
- `./scripts/deny.sh`.

## Not built (remaining)
- The Access tab in Administration (`aegis-client` + `aegis-shell`) and its connection/view wiring.
