# Security Review — 113-platform-fidelity (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
- The GPU renderer adds native dependencies (Skia, gbm/drm on Linux). They
  pass D5 and `scripts/deny.sh`; keep that gate in CI so a future bump cannot
  silently add a non-permissive licence.

## Security Evaluation
- `AEGIS_RENDERER` is read once at start; unknown values warn and use Skia.
  It selects a renderer, not a security boundary, and never loads a library
  from a user-controlled path.
- The fallback re-execs the current executable with `AEGIS_RENDERER=software`
  and the same arguments; it inherits the environment and adds no privileged
  operation. No loop is possible because the child starts in software mode.
- No new file, network, or credential handling. Fonts are referenced by
  family name (no download, no bundled binary). `scripts/deny.sh` confirms the
  new dependency graph.
- The headless test platform is unchanged and never uses the GPU path, so CI
  still needs no GPU and renders deterministically.
