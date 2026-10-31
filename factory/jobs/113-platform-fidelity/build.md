# Build — 113-platform-fidelity

## What was built (this round)

### Renderer selection (UI-07.1, ADR 0063)
- New `crates/aegis-shell/src/renderer.rs`: `Renderer::{Skia, Software}`,
  `parse(AEGIS_RENDERER)` (absent/empty -> Skia, `software`/`sw` ->
  Software, unknown value -> Skia with one warning marker), and `select()`
  which installs the winit backend and retries once on software when Skia
  cannot be selected.
- `crates/aegis-shell/src/main.rs` now calls `renderer::select()`, documents
  `AEGIS_RENDERER` in the environment table, and, when the Skia window fails
  at `window.run()` (a GPU surface can fail only once the first window is
  shown), re-executes the process once with `AEGIS_RENDERER=software` instead
  of silently downgrading.
- Workspace `Cargo.toml`: Slint now enables `renderer-skia` in addition to
  `renderer-software` (the fallback, the escape hatch, and the headless
  renderer). `Cargo.lock` updated.

### Per-OS widget style (UI-07.4)
- `crates/aegis-shell/build.rs` reads `CARGO_CFG_TARGET_OS`, selects
  `cupertino` on macOS and `fluent` elsewhere via
  `CompilerConfiguration::with_style`, and exports `AEGIS_SLINT_STYLE` for a
  test. Cross-compiling still picks the target style, not the host style.

### Shortcuts from one table (UI-07.6)
- `crates/aegis-shell/src/shortcuts.rs` is now a single table of key parts
  (`["Control", "Shift", "Z"]`), rendered as `⇧⌘Z` on macOS and
  `Ctrl+Shift+Z` elsewhere by `format_label`; `slint_keys` turns a row into
  the `keys` binding the menus use. `install(&window)` pushes every menu
  accelerator and the platform-spelled description hint.
- `ui/app.slint` menu items bind `shortcut: root.<id>-keys;` from that table
  instead of spelling `@keys(...)`; the description editor's hint comes from
  `root.save-description-hint`. A test
  (`no_slint_file_or_rust_string_spells_a_shortcut`) fails if any
  `.slint`/`.rs` file except `shortcuts.rs` spells a shortcut.
- The command palette is unchanged in behaviour and still reads the same table.

### Fonts (UI-07.7, partial)
- New `crates/aegis-shell/src/platform.rs`: the target widget style, the
  native UI family (`.SF NS` / `Segoe UI Variable`), the monospace family
  (`.SF NS Mono` / `Cascadia Mono`), and the Japanese fallback names, all by
  family name (no bundled font, no new licence). `apply_fonts` is called from
  `Controller::attach_with`.
- `ui/app.slint` binds `default-font-family: root.ui-font-family;` so UI text
  uses the native family; the Japanese fallback is left to Slint's OS font
  database per glyph.

### Headless tests stay on software (UI-07.9)
- `src/headless.rs` is unchanged: it installs `MinimalSoftwareWindow`
  directly and never consults `AEGIS_RENDERER`, so screenshot/monkey tests
  need no GPU. `renderer::tests` assert the backend names (`skia`,
  `software`).

## Verified
- `cargo fmt --all -- --check`.
- `cargo clippy -p aegis-shell --all-targets -- -D warnings`.
- `cargo test -p aegis-shell` and `cargo test --workspace --locked`.
- `scripts/deny.sh` (advisories, bans, licenses, sources) - the new
  `i-slint-renderer-skia` / `skia-safe` / `skia-bindings` graph passes D5.

## Not built (remaining)
- **Renderer spike (UI-07.2, acceptance criterion 1).** The Retina Mac +
  Windows laptop measurements have not been run; ADR 0063 stays `proposed`
  and its evidence table stays `TBD`. Merge is gated on this (owner-run).
- **System/Light/Dark appearance (UI-07.5, acceptance criterion 6).** Only
  the existing Dark toggle remains; there is no System default, no live OS
  following, and no persisted appearance setting yet.
- **Offline build (UI-07.3).** Skia's first build downloaded its prebuilt
  archive; `cargo build --offline --release` from a warmed cache has not been
  proven yet. Vendoring with a checksum/licence notice (or FemtoVG) is the
  fallback named in the ADR.
- **Monospace binding and Japanese fallback proof (UI-07.7).** The
  `mono-font-family` property is set but not yet bound to the identifier /
  detail surfaces, and Japanese output has not been re-screenshotted on macOS.
- **Explicit headless-renderer assertion (UI-07.9).** The behavioural
  guarantee holds (headless never uses the GPU path), but the criterion's
  "assert the headless renderer is `software` even when the env asks for GPU"
  test is not written yet.

## Round 2

### Built
- **System/Light/Dark appearance (UI-07.5).** `Settings.appearance` (`system`,
  `light`, `dark`; empty means system) with an `Appearance` enum in
  `aegis-client`. The window has `appearance` (0/1/2) and an
  `appearance-selected` callback; the launcher persists the choice through the
  existing settings file. System writes `ColorScheme.unknown` to `Palette`,
  which both Slint styles resolve to the OS value live. `Theme.dark` now reads
  the palette background instead of the raw scheme, so the brand tokens
  follow System mode too. The forced-dark `init` is gone; the View menu's
  "Dark theme" toggle became an Appearance submenu. Splash/About keep their
  obsidian tokens. ja and pseudo catalogs updated.
- **Monospace binding (UI-07.7).** The outline editor and the library
  preview now use `mono-font-family` instead of the literal `"monospace"`.
  Catalog details stay proportional (prose). The `ID` table column stays in the
  UI font because `StandardTableView` has no per-column font.
- **Headless renderer assertion (UI-07.9).** `headless::RENDERER` is
  `Software`; `tests/appearance.rs` checks it while `renderer::parse` asks for
  Skia, and renders a frame. Setting the variable itself is not possible
  (`deny(unsafe_code)`), so the test proves the headless path never reads it
  by construction.
- **Offline build (UI-07.3).** Measured, not assumed: see ADR 0063. The
  no-network release build fails at `skia-bindings`; the checklist now says to
  vendor the archive via `SKIA_BINARIES_URL`.

### Verified
- `cargo fmt`, `cargo clippy -p aegis-shell -p aegis-client --all-targets -D
  warnings`, `cargo test --workspace --locked` (1596 passed, 0 failed),
  `scripts/deny.sh`.
- `tests/appearance.rs`: Dark and Light repaint the same window live.

### Not done
- Renderer spike on a Retina Mac and a Windows laptop (owner-run); ADR stays
  `proposed`.
- Offline build from a vendored archive on macOS and Windows.
- Live OS-appearance following is not exercised by any test (the headless
  platform has no OS scheme); check it by hand: flip the OS appearance with
  Appearance = System and watch the window.
- Japanese and pseudo-locale screenshots on the real macOS window.

### Round 2 addendum: offline build proven on macOS
- With the cached Skia archive vendored and `SKIA_BINARIES_URL` pointing at it,
  `cargo build --offline --locked --release -p aegis-shell` succeeds with the
  network denied (46.3 MB binary). Checksum and details are in ADR 0063.
- Verified locally on `28d4aa5`: `scripts/ci.sh`, `scripts/deny.sh`,
  `tools/deny-fixtures/run.sh` (from a clean clone), per-crate `cargo check
  --locked`, and `deploy/verify-image.sh`. GitHub Actions did not run (billing).
- Still open: Windows vendored build, archive licence notice, the owner-run
  Retina Mac + Windows spike, and a hand check of live OS appearance.

### Round 2 review fixes (code review, CHANGES)
- **Hint translated again:** the description-editor hint is `@tr("Plain text. {0} saves, Escape cancels.", ...)` with the key label as an argument; ja and pseudo catalogs updated (the existing `tests/i18n.rs` key check covers it).
- **Redo keeps Ctrl+Y off macOS:** the table row has an `off_mac` spelling, so Windows/Linux keep Ctrl+Y (menu key and cheat sheet agree); macOS uses ⇧⌘Z. A test pins both labels.
- **Dead code removed:** the unused `mac-shortcuts` property and the unused `Meta`/`Super` modifier entries.
- **Left open (should-fix, not done):** `JAPANESE_FALLBACK` is only read by a test and the `.SF NS` names are unproven on a real macOS window (needs a screenshot); `reexec_on_software` spawns the child while the parent still holds the runtime and any standalone project, and hides the child's own error. Round 2 security, UX and UI reviews did not complete (stopped).
