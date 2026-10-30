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
- None. The Access tab in Administration is built in this round (round 2).

## What was built in round 2 (UI)
- `aegis-client`: `Connection::{memberships,set_membership,remove_membership}`, `MembershipRow`/`MembershipView` state, `Command::{LoadMemberships,SetMembership,RemoveMembership}`, and the actor events that load and change memberships.
- `aegis-shell`: an **Access** tab in Administration (the fourth tab) that lists the open project's member and role rows, adds a member through an inline `user:name or group:name` + role row (not a dialog), and changes a role in place with a role dropdown.
- Removing a member keeps a confirmation per UI-12: a named title (`Remove <member> from '<project>'?`) and an action-named button (`Remove`), never "Yes". The last-owner refusal is shown inline next to the row.

## Verified
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.
- `cargo test --workspace` (ran unsandboxed; loopback bind and OS trust store are blocked by the sandbox).
- `./scripts/deny.sh`.
