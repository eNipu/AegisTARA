# Security review — 058-project-access (round 1)

## Findings
- None blocking. The access table never grants admin capabilities; SSO group
  names and member keys are length-capped and control-character-free; the
  last owner cannot be removed; the development-login bypass is gated behind
  `AEGIS_DEV_AUTH`.

## Open
- The Access tab UI (and any client-side role strings) is not built yet.
