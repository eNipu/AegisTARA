# UX Review — 114-modeless-create (Round 1)

VERDICT: PASS

## Blocking findings
None.

## What was checked
- Each of the eleven elements is created without a dialog. Five assets with the keyboard alone: 37 key presses, no
  mouse; the old flow was 41 inputs including 5 mouse clicks and five dialogs.
- Enter or Tab creates; Ctrl/⌘+Enter creates and starts the next draft (the asset, chosen category and component
  carry over); Escape or an empty name sends nothing and leaves no history; focus loss creates when there is a name
  (not on forms that have other fields).
- Required fields come from the context (the only asset or component, or the selected one); Enter with one missing
  moves focus to it; the Create button says what is missing; choosing the last one returns focus to the name.
- Names save on Enter, Tab and focus loss; Escape restores; an empty name returns the saved name and says "A name is
  needed"; no Rename button.
- Read-only: the header buttons are off with the reason; no draft starts.
- A refused create keeps the text and says why in the draft.

## Follow-ups for a person (not gates)
- Japanese IME (UI-08.4): confirm that the Enter which commits a composition does not create the element, on macOS
  (Romaji, Kana) and Windows (Microsoft IME). Slint's IME path is used; no code of ours handles composition.
- Measure a person's time to add five assets before and after (only key presses were counted).

## Advisory findings
- The ⌘/Ctrl+Shift+N shortcut may clash with a window-manager binding on some Linux desktops; it is in the shortcut
  table and the command palette.
- The File menu's "New project…" still asks for a name in a small dialog; the start screen has the inline card.
