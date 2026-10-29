# 0061 — Project access: built-in roles and per-project membership

- **Date:** 2026-10-30
- **Status:** accepted
- **Decision:** enforce access with a built-in role table and per-project
  membership stored in every backend, checked on the server before every
  project RPC. This is decision D19 implemented (`ACC-01` to `ACC-03`, job
  058, closing `SEC-02`).

## Context

Before this change every authenticated user could open and edit any project
(`SEC-02`). The requirement names six roles (Admin, Methodology Owner,
Project Owner, Analyst, Reviewer, Viewer), per-project membership, SSO-group
role mapping, an Access tab, and audit events.

## Decision

- A `crates/aegis-server/src/access.rs` table maps a project to member keys
  (`user:<subject>` or `group:<SSO group name>`) and role keys. The six role
  keys are fixed and ordered least-to-most capable:
  `viewer`, `reviewer`, `analyst`, `project_owner`, `methodology_owner`,
  `admin`.
- Permissions are `Read`, `Write`, `Manage`, and `Admin`. The access table
  grants `Read`/`Write`/`Manage` by role; it never grants `Admin`. A
  server-global admin (`Principal.admin`, from the OIDC admin claim or the
  password admin flag) has every permission without a membership row.
- Membership persists in `MemoryStore`, `FolderStore` (a `memberships` array
  in the manifest, `#[serde(default)]` for old folders), and `PostgresStore`
  (`aegis_project_member`, migration 6). `ProjectStore` grows
  `all_memberships`, `project_memberships`, `memberships_for`,
  `set_membership`, and `remove_membership`.
- `CreateProject` grants the creator the `project_owner` membership. The last
  owner (`admin` or `project_owner`) row cannot be removed or demoted, so a
  project can never be locked out by accident.
- `AegisService` holds an in-memory cache (`D10`) rebuilt at startup by
  `load_access_cache()` and kept current by the membership RPCs. Every
  project-scoped handler resolves the permission it needs through
  `require_access`. A caller matches by `user:<subject>` first, then by each
  `group:<name>` claim in order.
- SSO groups map from the `groups` and `roles` claims into
  `Principal.groups` (`ACC-02`).
- New RPCs `ListMemberships`, `SetMembership`, `RemoveMembership`, plus audit
  kinds `membership_changed` and `membership_removed` (`ACC-03`).

## Consequences

- Existing projects created before this change have no membership rows, so a
  non-admin, non-development user cannot see them until an admin assigns a
  membership. The development login (`AEGIS_DEV_AUTH`) remains trusted and is
  never enabled in a customer build, so development servers and tests do not
  need a membership table.
- Development login bypasses the access table (it already trusts any caller),
  which keeps the existing dev/test servers working unchanged.
- The Access tab in Administration (client UI) is the remaining UI work for
  this job and is tracked in the backlog.
