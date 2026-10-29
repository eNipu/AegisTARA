# Decision — 058-project-access

## Decision
Built-in role table plus per-project membership, checked on the server before every project RPC, stored in MemoryStore, FolderStore, and PostgresStore. See ADR 0061.

## Roles and permissions

| Role | Read | Write | Manage membership | Admin |
|---|---|---|---|---|
| viewer | yes | no | no | no |
| reviewer | yes | no | no | no |
| analyst | yes | yes | no | no |
| project_owner | yes | yes | yes | no |
| methodology_owner | yes | no | no | no |
| admin | yes | yes | yes | yes |

Global admin (`Principal.admin`) has every permission without a membership row. The access table never grants `Admin`.

## Key choices
- Member keys are `user:<subject>` or `group:<SSO group>`; a caller matches user first, then groups in claim order.
- `CreateProject` grants the creator `project_owner`. The last owner row cannot be removed or demoted.
- Development login bypasses the table (it already trusts any caller and never ships in a customer build).
- An in-memory cache rebuilt at startup keeps every check O(1) (D10).
