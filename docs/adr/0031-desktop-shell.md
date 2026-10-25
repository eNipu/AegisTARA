# ADR 0031: Desktop shell on Slint

- Status: proposed (job 033-client-core-and-slint-shell, part 2)
- Date: 2026-10-25
- Scope: `crates/aegis-shell`, `deny.toml`, `docs/release-checklist.md`
- Rules: A5, C2, P4, D10 in `docs/engineering-standards.md`; decisions D7 and D9
- Builds on: `0029-canvas-engine.md`, `0030-client-core.md`

## Context

The client core (ADR 0030) holds all behaviour and has no UI dependency. The
canvas engine (ADR 0029) turns a tree into a display list in screen
coordinates. The shell has one job: draw those two things in a window and
report what the user did. D7 chose Slint. D9 records the license assumption.

## Decisions

### 1. The shell is thin on purpose

- `ui/*.slint` lays out and reports events. It makes no decisions.
- `adapter.rs` turns a `DisplayList` into Slint `DrawItem` rows. It also cuts
  dashed curves into segments, because the Slint path element has no dash.
- `controller.rs` is the only code that touches both the `Session` and the
  window. It copies the latest `ViewModel` into window properties on the UI
  thread (`apply_latest`) and sends `Command`s back. Pan, zoom, hit test and
  keyboard walk call the canvas engine. The shell holds no project state.
- `provider.rs` picks the login provider: OIDC with the OS keychain, or the
  dev login for local work.

So replacing Slint rewrites `ui/`, `adapter.rs`, and the glue in
`controller.rs`. The client core, the canvas engine, and the server do not
change. That keeps D7 cheap to reverse.

### 2. Software renderer only

The renderer is CPU based (`renderer-software`, no GPU backend). It runs the
same on a laptop, a VM, and a remote desktop, and it needs no driver. The
canvas engine measured a frame at 100k nodes in 7.8 microseconds, so the CPU
path has large headroom. The status bar says "CPU renderer" so the choice is
visible.

### 3. Tests drive the real window

`headless.rs` installs a Slint platform whose window renders into memory.
`tests/ui.rs` starts a real server, creates the real `AppWindow`, and drives
it with pointer, wheel, and key events. Six tests cover: connect, project
list, create, lock, sample tree, node click and inspector, rating change and
undo, pan, zoom, fit, keyboard walk, residual toggle, five factors to an attack
potential, and a second user who sees the project read only and follows the
editor live. Each test saves a PNG to `target/shell-screenshots/` for review.
No test checks pixel equality, so a font change does not break them. They
check properties, the view model, and a few pixels.

A bug found this way: a `ComboBox` loses its binding when the user picks a
value, so an undo or a remote change did not show. The `Choice` component in
`app.slint` forwards both `index` and `options` changes into the box. Every
drop-down uses it.

### 4. License and supply chain

Slint is used under the Royalty-free License 2.0 (D7, D9). Its condition is
met by the About window (`AboutSlint`) and by the badge on the download page
(`docs/release-checklist.md`). In `deny.toml`:

- eleven `slint*` crates have a license exception that names D9;
- a `[bans]` rule lets only `aegis-shell` depend on `slint`, so the license
  cannot spread into the core or the server by accident.

`scripts/deny.sh` and `tools/deny-fixtures/run.sh` check both.

Slint also brings four RustSec "unmaintained" notices (`ttf-parser`, `zvariant`,
`zbus_names`, `zvariant_derive`). They come from the window frame font and the
Linux accessibility bridge, no safe upgrade exists, and none is a
vulnerability. `deny.toml` ignores them by id, each with a reason, and the
file says to re-check at every Slint bump.

### 5. Accessibility basics now, the rest later

Controls carry `accessible-label`. The canvas has a text summary
(`canvas-summary`) and full keyboard navigation (ADR 0029). A screen reader
path for the canvas items is not built. It is listed in the backlog for the
UX review, which the owner deferred.

## Consequences

- One more big dependency tree, confined to one crate by `[bans]`.
- If D7 is rejected, delete `crates/aegis-shell` and the exceptions. Nothing
  else changes.
- The shell is only built and tested with the software renderer. A GPU
  backend would be a new feature flag and a new ADR.
