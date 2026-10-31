# Build — 116-undo-toasts

## What was built (round 1)

### Undo/Redo first-class buttons (UI-10.1, part of UI-10.2/UI-10.3)
- `app.slint`: Undo and Redo are now icon-only buttons at the left end of the
  project top bar, before the project title, on every open-project page. The
  old status-bar text buttons are removed.
- Each button carries a hover tooltip "Undo (⌘Z)"/"Redo (⇧⌘Z)" from the one
  shortcut table (`undo-shortcut`/`redo-shortcut` string properties set by
  `shortcuts::install`), and an accessible name for screen readers.
- The Edit menu and the existing shortcut table already provided the ⌘Z /
  Ctrl+Z / ⇧⌘Z / Ctrl+Y / Ctrl+Shift+Z bindings and the menu accelerators; the
  buttons reuse the same `undo()`/`redo()` callbacks.

## Verified
- `cargo fmt --all -- --check`.
- `cargo clippy -p aegis-shell --all-targets -- -D warnings`.
- `cargo test -p aegis-shell` (ui, pages, monkey, small_windows, i18n,
  appearance, standalone, login) — all pass.

## Not built (remaining)
- **Tooltip/Edit-menu naming the exact action** ("Undo remove asset 'Brake
  ECU'") — the client does not yet expose a description of the pending
  undo/redo, so the tooltip names the action generically.
- **UI-10.4** text-field typing undo, **UI-10.5** the "Undone: … · Redo"
  toast + selection/scroll, **UI-10.6** the per-op undo test, **UI-10.7** the
  history-cleared toast.
- **Part B** (UI-10.8 to UI-10.12) removal-without-confirmation and the undo
  toast, inline field errors + sticky action-error toast, the confirmation
  allow list, and **UI-12.1/UI-12.2** (dialog rules + modal-overlay test).

## Round 2

### Built
- **UI-10.6 per-op undo test** (`crates/aegis-client/tests/undo_every_op.rs`).
  Every one of the 64 `Op` kinds is applied through `Workspace` as a user edit,
  undone (`Project::eq` with the project before) and redone (equal to the
  project after). Two walks cover them: the model's `second_every_op` walk-through
  (37 kinds, recorded by a small `record_every_op` hook added to
  `aegis-model/tests/common/mod.rs`) and a second walk for the other 27 (item
  definition, tree nodes, descriptions, review marks). The test compares the kinds
  seen with the variants declared in `op.rs`, so a new `Op` fails it until a walk
  covers it. A second test undoes a whole recorded session of 64 ops as one batch.
  `crates/aegis-client/tests/undo_labels.rs` adds removal-and-undo of component,
  interface, data flow and trust boundary with their links. Placed in `tests/`
  (spec path) rather than `workspace.rs` `mod tests`, because it reuses the model's
  test kit through `#[path]`. No variant is skipped. `uuid` is a new dev-dependency
  of `aegis-client` (already in the lockfile).
- **Undo/redo description** (UI-10.2): `aegis-client/src/undo_label.rs` builds a
  `Label` ("remove asset 'Brake ECU'", past form "Asset 'Brake ECU' removed",
  "… and unlinked from 3 threat scenarios" / "… and N other changes") from the ops
  and the project before them. `Workspace` stores a label with every undo/redo
  step (`undo_label()`, `redo_label()`); names are shortened to 40 chars and
  appear only in user-facing strings, never in logs. `ProjectView` carries
  `undo_label`/`redo_label`; the top-bar tooltips, accessible names and Edit menu
  titles use them ("Undo remove asset 'Brake ECU' (⌘Z)"), with "Nothing to undo"
  and "Read only: <band text>" when off.
- **Toast** (UI-10.5, UI-10.9): the existing notice list is now a bottom-centre
  toast (left of the inspector, so it never covers the inspector buttons). Notices
  carry an optional Undo/Redo button (`NoticeAction`). Info toasts go after 8 s and
  stay while the pointer is over them; warnings and errors stay until dismissed.
  A toast with a button replaces the previous one. Each toast is a polite
  accessible live region (`accessible-live-region: polite`) with the text as its
  accessible label. After Undo/Redo: "Undone: … · Redo" / "Redone: … · Undo", and the
  page of the affected element opens with it selected.
- **History cleared** (UI-10.7): `Workspace::take_cleared()`; the session shows
  "Undo history cleared: …" on a server refusal, a lost lock, a stale base and a
  reload.
- **UI-10.4**: a `Typing` global in `app.slint`; `NameEditor` (the rename field of
  every details panel) sets it while it has focus and differs from the saved name.
  The Undo/Redo menu items are disabled then, so the platform shortcut reaches the
  field and undoes its own typing; saving the field is one project step.
- **Part B**: the 12 removal confirmations (asset, damage, threat, tree, tree
  node, control, goal, claim, component, interface, data flow, trust boundary) are
  gone; the buttons call the removal at once and a toast "Asset 'X' removed · Undo"
  shows (also for unlinks; the toast says what else changed). Refusals and failed
  edits are sticky error toasts (`Severity::Error`). A shared `FieldError` widget
  (error colour + icon) replaced the red texts for the login error, the membership
  error and the password mismatch. The confirmation that stays is now UI-12
  conformant: the title names the thing ("Delete project 'ECU Gen3'?"), the main
  button names the action, a red `DangerButton` that is not focused first, Escape
  cancels, focus starts on Cancel.
- **UI-12.1/12.2**: section 11 of `docs/engineering-standards.md`; a checklist item
  in `docs/review-requirements.md` section 4; `crates/aegis-shell/tests/dialogs.rs`
  lists the 7 modal overlays and the 7 confirmation kinds with reasons.
- Shell tests `tests/toast.rs` drive a removal and an undo through the window,
  assert the toast and its button, and save screenshots
  (`target/shell-screenshots/toast-*.png`, viewed). A session test covers the same
  at the client level.

### Decisions
- The allow list is the five UI-12 cases plus four existing confirmations kept with
  a written reason: delete user, remove member, password login off, log out. Log
  out is the weakest; the owner may drop it.
- The model's "every op" kit covers 37 kinds, not 64, so the second walk was added
  instead of trusting the kit.
- "Import that replaces data" has no confirmation in the app today (no such flow);
  nothing to keep or remove.

### Verified
`cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`;
`cargo test --workspace --locked` (81 test binaries, 0 failed); `scripts/deny.sh`.
Screenshots of the toast, the Undone toast and the delete-project dialog looked at.

### Remaining
- **UI-10.5 scroll into view**: the page opens and the element is selected, but the
  list is not scrolled to it (no scroll API wired per page).
- **UI-10.4 coverage**: only `NameEditor` stands aside; no automated test of the
  real ⌘Z key (needs the native menu on a real macOS window). The other inline
  fields have no saved value, and the description editor is a modal with its own
  undo.
- **Hardware**: screen-reader announcement of the live region not checked with
  VoiceOver or NVDA.

## Round 3

Closes the two UX points of round 2. No change to the allow list: log out keeps
its confirmation (owner decision, 2026-11-01).

### Built
- **UI-10.4 proof** (`crates/aegis-shell/tests/typing_undo.rs`): real key events
  (`Control`, which Slint maps to Command on macOS) go through the window. With no
  field focused the shortcut undoes the last project change; with the rename field
  focused and typed into, the same key leaves the project unchanged. This works
  because Slint matches menu-bar shortcuts at window level before any widget, and
  only for enabled entries, so a disabled Undo item lets the key reach the text
  field.
- **UI-10.5 scroll into view**: the same test file builds a project with 40 threats,
  leaves the Threats page, undoes a rename of the last threat, and checks the page
  opened, T39 is selected and its row is on screen
  (`target/shell-screenshots/undo-scroll-long-list.png`, viewed). The test first
  failed: the list stayed at the top, because a table made when its page opens has
  no size yet and `set-current-row` could not scroll. `Table` in `widgets.slint` now
  says it again after a 60 ms timer once the layout has settled; this fixes every
  list in the app.

### Verified
`cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`;
`cargo test --workspace --locked` (82 test binaries, 0 failed); `scripts/deny.sh`.

### Remaining (not code gates)
- Check by hand on a real macOS window that ⌘Z in a dirty rename field undoes the
  typing (the native menu bar is not in the headless test).
- Check the toast live region with VoiceOver and NVDA.
