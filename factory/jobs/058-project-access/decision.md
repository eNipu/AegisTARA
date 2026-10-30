# Decision — 058-project-access

## Decision
Built-in role table plus per-project membership, checked on the server before every project RPC, stored in MemoryStore, FolderStore, and PostgresStore. See ADR 0061.

## UI decision (round 2)
The Access tab lives inside Administration (which is reachable by server
admins). It is a table plus one inline add row, not a dialog: the member key
and role are entered in place, and the role changes in place through a
dropdown. Removing a member is the one confirmation that stays (UI-12): it is
not on the project undo history, so it gets a named title and an action-named
button.

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
