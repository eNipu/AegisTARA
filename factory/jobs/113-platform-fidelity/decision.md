# Decision — 113-platform-fidelity

## Decision
ESCALATE

Round 3 reviewed the fixed code (the answer to the round-2 code review) and all
four reviews now PASS: Skia default with an automatic software fallback and
`AEGIS_RENDERER=software`, per-target `cupertino`/`fluent`, persisted
System/Light/Dark appearance that follows the OS, one shortcut table driving
the menus (Redo is ⇧⌘Z on macOS, Ctrl+Y elsewhere), native UI and monospace
fonts, and headless tests pinned to software. `fmt`, `clippy -D warnings`, the
workspace tests and `scripts/deny.sh` pass.

It is not merged because the remaining gates are owner/hardware work that the
spec requires before ADR 0063 can be accepted:

1. **Renderer spike (UI-07.2, AC1).** Retina Mac + Windows laptop screenshots,
   sizes, start times and memory; fill the evidence table in ADR 0063 and move
   it from `proposed` to `accepted`.
2. **Offline bundle (UI-07.3, AC4).** Proven on macOS aarch64 with the
   vendored `SKIA_BINARIES_URL` archive (ADR 0063); still to do: the Windows
   build and the archive licence notice.
3. **Manual checks (AC6, AC7, AC8, AC10).** Live OS-appearance following with
   Appearance = System, macOS Japanese/pseudo screenshots (record the rendered
   family), and the UI-07.8 menu-bar result in the ADR.

## What to do next
- Owner: run the spike and the Windows offline build, fill ADR 0063, then
  `/factory approve 113-platform-fidelity` to record and merge.
- If the spike contradicts Skia, reopen the build for FemtoVG per UI-07.3; the
  fallback and the rest of the job are renderer-agnostic.
- Two advisory code items stay open and can ride along: `JAPANESE_FALLBACK` is
  only read by a test (confirm the drawn family in the macOS screenshot), and
  `reexec_on_software` spawns the child while the parent still holds the
  runtime/launcher (document the standalone-lock caveat).

## Checks
- `cargo fmt --all -- --check`, `cargo clippy -p aegis-shell -p aegis-client
  --all-targets -- -D warnings`, `cargo test --workspace --locked`,
  `scripts/deny.sh`: all pass.
