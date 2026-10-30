# Rework — 113-platform-fidelity

## Answers

- **Renderer (spec Open question 1): Skia.** The owner picked Skia on
  2026-10-30. The app defaults to Slint's Skia renderer and keeps the automatic
  software fallback plus the `AEGIS_RENDERER=software` escape hatch. Prefer a
  vendored prebuilt Skia archive where that is what keeps the offline build and
  the D5/`cargo-deny` checks passing (`UI-07.3`). If Skia cannot be made
  offline-safe even with vendoring, `UI-07.3` allows FemtoVG instead; record that
  in ADR 0063 before implementing it.
- **Spike evidence (spec Open question 2): still required.** The pick does not
  waive `UI-07.2`: run the Retina Mac and Windows laptop spike, fill the evidence
  table in `docs/adr/0063-platform-fidelity-renderer.md`, and only then change
  its status from `proposed` to `accepted`. Screenshots and numbers must be
  measured, never estimated.
