# Spec — 114-modeless-create

## Title
Modeless creation: one draft-in-the-inspector flow for every element (UI-08)

## Design Reference
- `docs/ui-ux-requirements.md` UI-08 (08.1 to 08.7), findings F5 and F6, UI-10 (undo is the safety net).
- `factory/backlog.md` 114-modeless-create. Follows job 116 (undo and toasts are in).
- Affected paths: `crates/aegis-client/src/{commands.rs,view.rs,state.rs,session.rs}` (draft state and commands),
  `crates/aegis-shell/ui/{app.slint,widgets.slint}` (draft row, draft inspector, `NameEditor`),
  `crates/aegis-shell/src/controller.rs`, `crates/aegis-shell/tests/*`, `crates/aegis-client/tests/*`.

## Problem & Goal
Creating an element opens a modal `ask()` name dialog (13 call sites) or an inline
"Add a ..." form, and renaming needs a separate Rename button. Workshop entry of many
elements is slow and breaks keyboard flow. Replace both with one pattern: a draft row
and a draft inspector, created by Enter, Tab or Ctrl/⌘+Enter, discarded by Escape,
with nothing sent to the server until the element is complete.

## Build versus reuse (owner question, 2026-11-01)
The job is interaction logic, not a capability a library provides. Decisions:

| Need | Decision | Why |
|---|---|---|
| Text input, caret, selection, field undo | **Reuse** Slint `LineEdit`/`TextInput` (already shipped, D7). | Done there. |
| Japanese IME (UI-08.4) | **Reuse** Slint's winit IME path (preedit and commit). Do not write IME code. | The OS consumes the Enter that confirms a composition; the app only needs a manual check on macOS (Romaji, Kana) and Windows (Microsoft IME). |
| Draft state machine, required fields, one-op commit | **Build** in the client core (small, methodology-neutral, testable headless). | Domain logic; no library fits. |
| Undo of a created element | **Reuse** the existing `Workspace` undo (job 116). | One op, one step. |
| Names, ids, vocabulary | **Reuse** existing `Edit::Add*` commands and the pack vocabulary (D24). | No new data path. |
| Rich text, Markdown, Pandoc | **Not in this job.** Descriptions are plain text (ADR 0051); a rich-text description is a separate decision. If it comes: `pulldown-cmark` (MIT) or `comrak` (BSD-2-Clause) for Markdown. **Pandoc is GPL-2.0-or-later: not allowed to ship (D5)**; as a developer-only tool it is allowed (D5, 2026-10-26) if nothing of it ships. | No benefit to creation. |
| Crypto | **Not in this job.** Nothing here touches crypto. The product already uses audited crates (`ring`, `rustls`, `argon2`, `sha2`); never hand-written primitives. | Policy for any future job. |
| New dependencies | **None.** Any new crate needs cargo-deny to pass (D5 allow list). | Smaller supply chain. |

## Design
- **The client core owns the draft** (`UiState.draft`), as it owns selection. The window only shows it.
  `Draft { kind, name, fields (required fields, each with a value or none), context }`.
- **Commands** (`commands.rs`): `BeginDraft { kind, context }`, `SetDraftName`, `SetDraftField`,
  `CommitDraft { next: bool }`, `CancelDraft`. `CommitDraft` builds the existing `Edit::Add*` with a
  fresh id and applies it as one edit (so undo is one step); it fails with an inline error and keeps
  the text when the model or server refuses. `Cancel`, or leaving with an empty name, drops the draft
  and sends no op.
- **View** (`ProjectView.draft`): kind label, placeholder, name, required fields with options, an
  "incomplete" flag, an inline error. Read-only users get no draft; the page button is disabled with
  the reason (reuse the band text, as the Undo tooltip does).
- **Window**: the controller appends an italic draft row to the table (selected); the inspector shows
  the draft form with the name field focused. Enter or Tab commits; Ctrl/⌘+Enter commits and begins the
  next draft; Escape cancels; focus loss commits when the name is not empty.
- **Names** (UI-08.3): `NameEditor` saves on Enter, Tab and focus loss; Escape restores; an empty name
  is not saved and shows "A name is needed"; the Rename button goes away.
- **Slices** (each ends green and committed): S1 plumbing + asset, damage scenario, security control;
  S2 threat scenario (asset and STRIDE category from context), goal, claim, attack tree (for a threat);
  S3 component, interface, data flow, trust boundary; S4 `NameEditor` save semantics and Rename removal;
  S5 read-only reasons, start-screen inline card for new project (UI-08.7), removal of the `ask()` creation
  sites and the inline "Add a ..." forms, keyboard-count measurement.

## Acceptance Criteria
1. **UI-08.1** Each of asset, damage scenario, threat scenario, attack tree, security control, goal,
   claim, component, interface, data flow, trust boundary is created from a draft; the `ask()` dialogs
   and inline "Add a ..." forms for these are gone.
2. **UI-08.2** Required fields come from context where possible, otherwise appear in the draft inspector
   with the first one focused after the name; nothing is created until all have values; an incomplete
   draft row shows a small mark.
3. **UI-08.3** Names save on Enter, Tab and focus loss; no Rename button; Escape restores; an empty name
   is never saved and shows "A name is needed".
4. **UI-08.4** An Enter that confirms an IME composition does not create the element. Reuses Slint's IME;
   verified by hand on macOS and Windows (recorded in the review, not a code gate).
5. **UI-08.5** Read-only users see no + buttons; the page button is disabled with the reason.
6. **UI-08.6** The draft row and inspector appear in one frame; the created row appears at once; a
   refusal shows inline in the inspector with the user's text kept.
7. **UI-08.7** New project stays on the start screen as an inline card, not a dialog.
8. A cancelled draft sends no op and leaves no history entry (tested).
9. Headless tests create, cancel (Escape) and create-and-next for assets, threats and controls (real
   key events, as in `typing_undo.rs`), and create then undo restores the project exactly.
10. The UX review records the key presses and time to add five assets before and after.
11. No new dependency; `cargo deny` unchanged; all new strings use `@tr` with ja and pseudo entries.
12. `crates/aegis-shell/tests/dialogs.rs` stays green (no new modal overlay).

## Out of scope
Row actions (+ and ⋯ on rows) are job 115; this job adds only the page-header button, the shortcut and
the + that 115 will reuse. Rich-text descriptions, Pandoc, new crypto: see the table above.

## Open questions (defaults taken)
1. The new-item shortcut: ⌘/Ctrl+Shift+N as proposed in UI-08, but it may clash with a window-manager
   binding on Linux; default to the proposal and list it in the shortcut table.
2. A draft kept while the user changes page: discard it (nothing was sent), no prompt.
