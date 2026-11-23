# ADR 0073 — Qualified Authorization Identities and Legacy Membership Migration

- **Status:** Accepted
- **Date:** 2026-11-23
- **Backlog:** `220-qualified-identities`

## Problem

Previously, `OidcAuthn` dropped the OIDC `iss` claim after validation and `Access`
looked up every caller under `user:<subject>`. In a server running both local
password accounts and OIDC (or migrating between identity providers), a local
username and an OIDC `sub` with equal strings—or two issuers sharing a `sub`—would
implicitly collide on the same membership and audit identity.

## Decision

1. **Carry issuer on OIDC claims and principals:** `aegis_auth::validate::AccessClaims`
   carries `issuer`, and `aegis_server::auth::Principal` carries `issuer: Option<Arc<str>>`
   and `user_id: Option<UserId>`.
2. **No implicit string linking:** `IdentityDirectory` maps qualified
   `ExternalIdentity::{Local, Oidc { issuer, subject }, Development}` keys to stable
   internal `UserId`s (`uid:<uuid>`). Two external identities with equal bare strings
   receive distinct `UserId`s unless an administrator explicitly calls
   `IdentityDirectory::link` or `IdentityDirectory::replace_issuer`.
3. **Qualified access lookup:** `Access::roles_for` matches `Principal::qualified_member_key()`:
   - `uid:<uuid>` when an internal `UserId` is attached,
   - `local:<username>` for `AuthMethod::Password`,
   - `oidc:<issuer>:<sub` for `AuthMethod::Oidc`,
   - `user:<name>` only for `AuthMethod::Development`.
   Password and OIDC principals never match bare legacy `user:<bare>` rows.
4. **Historical audit bytes unchanged:** Existing rows in `aegis_project_op` and
   `aegis_event` are never rewritten, so hash chains remain valid. New audit records
   use `Principal::audit_actor()` (`uid:<uuid>` when a `UserId` is assigned).

## Backup, Migration, and Rollback Procedure

1. **Backup:** Before running `migrate_legacy_memberships`, export the full
   `aegis_project_membership` table (`MigrationPlan::backup` captures the exact
   in-memory snapshot).
2. **Migrate (idempotent):** Run `migrate_legacy_memberships(&existing, &directory)`:
   - `user:<bare>` rows matching **exactly one** `UserId` rewrite to `uid:<uuid>`.
   - `user:<bare>` rows matching **0 or >1** `UserId`s are quarantined in
     `MigrationPlan::ambiguous` and remain as `user:<bare>`, which grants **no access**
     to Password or OIDC callers.
   - Existing `uid:<uuid>` and `group:<name>` rows pass through untouched; re-running
     the migration is idempotent.
3. **Explicit resolution:** Administrators inspect `MigrationPlan::ambiguous` and call
   `resolve_legacy_membership(rows, project, bare_subject, chosen_user_id)` to assign
   each quarantined row to the intended `UserId`.
4. **Rollback:** Restore `MigrationPlan::rollback_rows()` (or the pre-migration SQL
   backup of `aegis_project_membership`). Because historical audit tables are untouched,
   rollback requires no audit chain changes.
