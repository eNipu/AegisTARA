# 0063 — Platform fidelity: renderer choice for the Slint desktop shell

- **Date:** 2026-10-30
- **Status:** proposed
- **Decision:** Skia, chosen by the owner on 2026-10-30. The spike evidence and
  the D5/offline-build checks below still have to confirm it before this record
  becomes `accepted`. Job 113 (`UI-07`), decision D25.

## Context

D25 keeps Slint for the desktop app and replaces the forced software renderer
with a GPU renderer that has a software fallback. `UI-07.1` prefers Skia and
compares FemtoVG in a spike; `UI-07.2` requires the spike before any renderer
code is merged; `UI-07.3` requires D5/`cargo-deny` cleanliness and an offline
build, and names FemtoVG (or a vendored Skia archive) as the answer if Skia's
build-time download breaks the offline bundle.

Today `crates/aegis-shell/src/main.rs` selects `renderer_name("software")` on
every platform, and `crates/aegis-shell/src/headless.rs` renders screenshot
tests on the software renderer. CI has no GPU.

This ADR is a scaffold: it records the methodology and the decision slots, not
measurements. No numbers are entered until the spike has run on both machines.

## Spike methodology

Goal: give the owner comparable evidence for software, Skia and FemtoVG on one
Retina Mac and one Windows laptop, at 100 % and 150 % OS display scale, with no
change to the UI layout or project content.

### Builds

Three release builds of `aegis-shell` from the same commit, differing only in
the Slint renderer feature:

| Build | Slint feature | Notes |
|---|---|---|
| `software` | `renderer-software` | today's baseline |
| `skia` | `renderer-skia` | `UI-07.1` preference; check build download/vendoring |
| `femtovg` | `renderer-femtovg` | `UI-07.3` fallback |

Record the exact Slint version (`=1.18.1` at the time of writing), the commit,
the build command, and whether the build needed network access.

### Machines

| Machine | Must record |
|---|---|
| Retina Mac | model, macOS version, chip, display scale used for 100 %/150 % |
| Windows laptop | model, GPU, driver version, Windows build, display scale used for 100 %/150 % |

No machine is substituted for another; if one is unavailable the spike is not
complete and the ADR stays `proposed`.

### Measurements

For every renderer × machine × scale:

1. **Screenshots** of the assets page, the attack tree canvas, and a Japanese
   page (Japanese UI language, Japanese project content), captured at the same
   window size with the same project. Compare each side by side with a native
   app on the same machine at the same scale (for example Finder or Notes on
   macOS), as `UI-07`'s "Done when" asks.
2. **Binary size**: release, stripped, `aegis` executable, bytes.
3. **Cold start time**: process launch to first painted frame, median of at
   least 5 launches. Record the instrumentation used (log timestamp, external
   stopwatch, or OS tool) so the number can be reproduced.
4. **Peak memory**: after loading the `large` reference project and opening the
   attack tree canvas. macOS: `/usr/bin/time -l` maximum resident set size, or
   Instruments; Windows: `PeakWorkingSet64` from `Get-Process`, or Task Manager.
   Record the tool.
5. **Build observations**: build time, download/vendoring steps, warnings, and
   whether `cargo build --offline --release -p aegis-shell` succeeds from the
   prepared cache.
6. **Fallback and menu bar**: whether the GPU build starts on a remote/virtual
   desktop, and whether Slint's `MenuBar` gives a system menu bar on macOS on
   that backend (`UI-07.8`; record the gap, build no workaround).

### Artefacts

- Screenshots, raw timing/memory output and build logs go under
  `docs/spikes/platform-fidelity/` (create it with the spike; it is not part of
  this scaffold).
- The summary goes in the evidence table below. Unmeasured cells stay `TBD`.

## Evidence

Fill one row per renderer × machine × scale. Do not estimate or interpolate.

| Renderer | Machine | Scale | Screenshots | Binary size | Cold start | Peak memory |
|---|---|---|---|---|---|---|
| software | Retina Mac | 100 % | TBD | TBD | TBD | TBD |
| software | Retina Mac | 150 % | TBD | TBD | TBD | TBD |
| software | Windows laptop | 100 % | TBD | TBD | TBD | TBD |
| software | Windows laptop | 150 % | TBD | TBD | TBD | TBD |
| skia | Retina Mac | 100 % | TBD | TBD | TBD | TBD |
| skia | Retina Mac | 150 % | TBD | TBD | TBD | TBD |
| skia | Windows laptop | 100 % | TBD | TBD | TBD | TBD |
| skia | Windows laptop | 150 % | TBD | TBD | TBD | TBD |
| femtovg | Retina Mac | 100 % | TBD | TBD | TBD | TBD |
| femtovg | Retina Mac | 150 % | TBD | TBD | TBD | TBD |
| femtovg | Windows laptop | 100 % | TBD | TBD | TBD | TBD |
| femtovg | Windows laptop | 150 % | TBD | TBD | TBD | TBD |

## Decision criteria

The owner weighs, in this order:

1. Text and canvas crispness at 100 % and 150 % next to a native app.
2. Smoothness of canvas pan/zoom on the `large` reference project.
3. Binary size, cold start and memory deltas against the software baseline.
4. `UI-07.3`: D5/`cargo-deny` clean and an offline-capable build (vendored
   Skia archive allowed) without maintainer burden.
5. macOS system menu bar support on the backend (`UI-07.8`, could).

## Decision

**Skia**, picked by the owner on 2026-10-30. Still to be recorded here before
changing `Status: proposed` to `accepted`: the measured values from the evidence
table, whether a vendored Skia archive is used (with version, checksum and
licence notice), that D5 and `cargo-deny` pass, that the offline build works,
and any `UI-07.8` menu bar gap. If Skia cannot pass `UI-07.3` even with a
vendored archive, `UI-07.3` allows FemtoVG; record that change here first.

## Offline build observation (measured 2026-10-30)

`cargo build --offline --locked --release -p aegis-shell` in a fresh target
directory, on an Apple-silicon Mac with the network denied (`sandbox-exec`
`deny network*`), **fails**. `skia-bindings 0.153.3` first tries the prebuilt
`skia-binaries-b7f043e0b1e2a850e702-aarch64-apple-darwin-ganesh-gl-jpegd-jpege-metal-pdf.tar.gz`
from `github.com/rust-skia/skia-binaries`, then falls back to downloading the
Skia sources (`codeload.github.com/rust-skia/skia`), and stops with
`curl: (6) Could not resolve host`. `--offline` only covers the crates.io
registry. `UI-07.3` therefore needs the vendored-archive route: the crate's
`SKIA_BINARIES_URL` accepts a `file://` URL. Still open: build the shipped
targets (macOS, Windows) from a vendored archive with the network blocked, and
record the archive's checksum and licence notice here. If that proves too
heavy, `UI-07.3` allows FemtoVG.

## Consequences

- The default becomes the chosen GPU renderer; the software renderer stays as
  the automatic fallback, the `AEGIS_RENDERER=software` path, and the headless
  test renderer (`crates/aegis-shell/src/headless.rs`).
- The workspace `slint` features in the root `Cargo.toml` change, so build size,
  build time and the offline bundle (job 100) are affected; document the new
  features and any vendoring step in the release/deployment notes.
- `cupertino` (macOS) and `fluent` (elsewhere) styles are chosen in
  `crates/aegis-shell/build.rs`; `Theme` colours remain on top.
- If Skia is chosen without vendoring, its build-time download is a documented
  prerequisite and a risk to the offline bundle; if FemtoVG is chosen, record
  what the screenshots gave up, if anything.

## References

- `docs/decisions.md` D25; `docs/ui-ux-requirements.md` UI-07.1 to UI-07.8;
  `docs/requirements.md` UI-07.
- `docs/adr/0031-desktop-shell.md` (Slint shell), `docs/adr/0003-cargo-deny-policy.md`
  (D5), `docs/adr/0057-i18n-and-japanese-localization.md` (fonts, Japanese).
- `factory/jobs/113-platform-fidelity/spec.md`.
