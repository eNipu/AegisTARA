# Review — 058-project-access (round 1)

## Code
- `access.rs`: role order, permission resolution, member-key bounds, and the
  user-beats-group / most-capable-wins rules are unit tested.
- `service.rs`: every project-scoped handler now resolves a permission through
  `require_access`; `create_project` grants the owner and rolls back on
  membership failure; membership RPCs validate member/role and protect the
  last owner. Store errors map to `ServerError`.

## Security
- The access table never grants `Admin`; a server-global admin flag is the
  only admin path. SSO groups are read from the validated token claims.
- Member keys are length-bounded and reject control characters.
- Development login remains the only bypass and is gated behind
  `AEGIS_DEV_AUTH` (never a customer build).

## UX / UI
- The Access tab in Administration is not yet built; tracked as remaining
  work for this job.
