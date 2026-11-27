# 142a build

Commit: 7dd201c Add immutable tree roles with TT fragment IDs (142a)
Base: a5877d6 Plan factory job 142-shared-fragments

Done when:
- `cargo test --locked -p aegis-model --test shared_fragments` passed (9 tests): TT/AT prefixes
  from one sequence, undo restores role and ID, role in `Project::eq` and diff, snapshot and op
  log round trips with the default omitted, legacy files load as threat trees, a contradicting
  visual ID (`StoreError::InvalidId`) or tree file name (`WorkspaceTreeName` naming the file) is
  refused, an unknown role is refused.
- The random op generator now creates fragments, so `ops_properties` covers the role.
- `scripts/ci.sh` (workspace fmt, clippy, test across 9 crates) passed; factory gate PASS,
  237 counted lines / 400.

Notes:
- Threat scenarios can still link to a fragment tree; the guard lands with the fragment ops in
  142b, together with the protocol version bump for the new op variants.

NEXT: reviewer
