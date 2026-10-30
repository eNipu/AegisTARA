# UX Review — 113-platform-fidelity (Round 1)

## Verdict
CHANGES

## Blocking findings
- **System/Light/Dark appearance is not delivered (UI-07.5, acceptance
  criterion 6).** The app still only offers the old Dark toggle. There is no
  System default, no live follow of the OS appearance, and no persisted
  appearance setting. This is the main "not native on macOS" complaint behind
  D25 and must be finished before the job can ship.

## Advisory findings
- The native font family is applied, but the monospace family is not yet
  bound to identifier/detail surfaces, and Japanese output has not been
  re-screenshotted on macOS (UI-07.7).
- The renderer fallback is invisible to the user by design (one log line, no
  dialog), which is correct; confirm it reads clearly in a remote session.

## What is good
- Menus now take their accelerators from the one shortcut table, so ⌘ labels
  and Ctrl labels can no longer drift between the menu, the cheat sheet and
  the palette.
- The app starts on the GPU renderer and keeps working when the GPU is
  unavailable, which is the behaviour users notice most on a virtual desktop.
