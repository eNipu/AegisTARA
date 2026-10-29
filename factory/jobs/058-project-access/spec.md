# Spec — 058-project-access

## Title
Project access: built-in roles and per-project membership (ACC-01 to ACC-03, D19)

## Design Reference
- Reference: docs/adr/0061-project-access-roles-and-membership.md, docs/decisions.md D19
- Affected crates/paths: crates/aegis-proto/proto/aegis/v1/aegis.proto, crates/aegis-server/src/{access,auth,auth_layer,service,error,main}.rs, crates/aegis-server/src/store/{mod,memory,folder,postgres}.rs, crates/aegis-server/migrations/006_project_access.sql, crates/aegis-server/src/accounts/mod.rs

## Problem & Goal
Every authenticated user could edit any project (SEC-02). Add the six built-in roles, per-project membership enforced on the server before every RPC, SSO-group role mapping, membership RPCs, and audit events.

## Acceptance Criteria
1. Built-in roles Admin, Methodology Owner, Project Owner, Analyst, Reviewer, Viewer with a total permission order; 2. membership persisted in MemoryStore, FolderStore, and PostgresStore and checked before every project RPC; 3. role mapping from SSO groups; 4. membership/role changes audit events; 5. the last owner row cannot be removed.
