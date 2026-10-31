VERDICT: PASS

# Security Review — 113-platform-fidelity (Round 3)

## Blocking findings
None.

## Advisory findings
- The GPU renderer adds native dependencies (Skia, gbm/drm on Linux). They
  pass D5 and `scripts/deny.sh`; keep that gate in CI.
- For the offline bundle, keep the vendored Skia archive's checksum and
  licence notice with the bundle (release checklist already says so).
- `reexec_on_software` re-execs the current executable with
  `AEGIS_RENDERER=software` and the same arguments. It inherits the
  environment and adds no privileged operation; the child starts in software
  mode so there is no loop. Note the standalone-folder lock caveat in the ADR.

## Security Evaluation
- `AEGIS_RENDERER` only selects a renderer; unknown values warn and use Skia.
  It never loads a library from a user-controlled path.
- Appearance and font choices are UI state only: no file, network or
  credential handling. Fonts are resolved by family name (no download, no
  bundled binary).
- The headless test platform never uses the GPU path, so CI needs no GPU.
- `scripts/deny.sh` confirms the new dependency graph.
