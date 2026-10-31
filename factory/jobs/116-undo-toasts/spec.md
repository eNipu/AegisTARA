# Spec — 116-undo-toasts

## Title
Visible undo and redo, then undo instead of confirmation (UI-10, UI-12)

## Design Reference
- Reference: `docs/ui-ux-requirements.md` UI-10 (Part A and Part B) and UI-12;
  `docs/requirements.md` UI-10/UI-12; `factory/backlog.md` 116-undo-toasts.
- Affected crates/paths:
  - `crates/aegis-shell/ui/app.slint` (top bar, toast overlay, dialogs)
  - `crates/aegis-shell/ui/widgets.slint` (if a shared toast/field-error widget is added)
  - `crates/aegis-shell/src/{controller.rs,shortcuts.rs}`
  - `crates/aegis-client/src/{workspace.rs,commands.rs,state.rs,view.rs,session.rs}`
    (undo/redo description, per-op undo coverage)
  - `crates/aegis-client/tests/*` (per-op undo test)
  - `docs/engineering-standards.md` (UI-12 dialog rules), the UX review checklist

## Problem & Goal
The client core already has undo/redo (`Workspace::undo`, `redo`), but the app
hides them as two small text buttons next to "numbers checked" (F12). The owner
could not find them, so confirmation dialogs cannot be removed yet: undo is the
safety net, and a safety net the user cannot find is not a safety net. Make
undo/redo first-class, then (and only then) remove confirmation dialogs for
everything undo can bring back.

## Acceptance Criteria

### Part A — undo/redo the user can see and trust (UI-10.1 to UI-10.7)
1. **UI-10.1** Undo and Redo are icon buttons in one fixed place: the left end
   of the project top bar, before the page title, on every page of an open
   project. The status-bar text buttons are removed.
2. **UI-10.2** The tooltip and the Edit menu name the action and the shortcut
   ("Undo remove asset 'Brake ECU' (⌘Z)", "Redo rename threat scenario (⇧⌘Z)").
   Nothing to undo → disabled with "Nothing to undo"; read-only → disabled with
   the reason ("Read only: Hana Suzuki is editing").
3. **UI-10.3** Shortcuts follow the platform: ⌘Z and ⇧⌘Z on macOS; Ctrl+Z,
   Ctrl+Y and Ctrl+Shift+Z elsewhere, and work from any focus (table, canvas,
   inspector, panel).
4. **UI-10.4** Text fields keep their own typing undo while their text differs
   from the saved value; otherwise the shortcut undoes the last project change.
   A saved field is one project undo step.
5. **UI-10.5** After undo/redo, open the page of the affected element if
   needed, select it, scroll it into view, and show "Undone: … · Redo" toast.
6. **UI-10.6** Every user action is one undo step, including a batch (library
   insert, outline apply, import, draft). A test applies and undoes every `Op`
   kind and checks the project is exactly restored (links, ratings,
   descriptions, tree nodes); a removal's undo brings back every cut link.
7. **UI-10.7** The undo history is this session's edits; cleared on close,
   reload or lease loss, with a toast when that happens.

### Part B — no confirmation for what undo can bring back (UI-10.8 to UI-10.12)
8. **UI-10.8** Removing, unlinking and every other undoable change happen at
   once with no confirmation, per element type only after its UI-10.6 removal
   test passes.
9. **UI-10.9** A bottom-center toast says what happened and offers Undo
   ("Asset 'Brake ECU' removed · Undo (⌘Z)"), stays 8 s (longer on hover),
   never covers the inspector buttons, replaces the previous toast, and is
   announced to screen readers.
10. **UI-10.10** When a removal changes other elements, the toast says so in
    one line ("… and unlinked from 3 threat scenarios").
11. **UI-10.11** Field errors show inline next to the field (error color +
    icon); action errors show as a sticky error toast. No error opens a dialog.
12. **UI-10.12** Confirmation stays only for actions undo cannot reverse
    (UI-12): delete project, break another user's lock, reset/change password,
    end sessions, import that replaces data.

### Dialog policy (UI-12)
13. **UI-12.1** The UI-12 rules are added to `docs/engineering-standards.md`
    and to the UX review checklist used by `review-ux`.
14. **UI-12.2** A test lists the modal overlays in `app.slint` against an
    allow list, so a new dialog fails the build until it is added with a
    reason.
