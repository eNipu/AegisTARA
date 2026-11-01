# Build — 114-modeless-create

## Slice 1 (asset, damage scenario, security control)

### Built
- **Client core owns the draft.** `view::DraftKind` (Asset, Damage, Control) in
  `UiState.draft` plus `draft_seq`; `ProjectView.draft` / `draft_seq`. Commands
  `BeginDraft(kind)`, `CommitDraft { name, next }`, `CancelDraft`. The typed text stays
  in the window and travels with `CommitDraft` (one command, no per-key round trip).
  Commit trims the name; an empty name drops the draft and sends no op; otherwise it
  builds the existing `Edit::Add*` (asset gets every property, as the old dialog did) and
  applies it through the one edit path, so create is one undo step and the created element
  is selected. A refused edit keeps the draft (the sticky error toast says why). `next`
  starts a new draft with a new `draft_seq`. Leaving the page or selecting another row
  drops the draft.
- **Window.** `DraftForm` (widgets.slint): the inspector in draft mode with a focused name
  field. Enter or Tab creates, Ctrl/⌘+Enter creates and starts the next, Escape discards,
  leaving the field with a name creates it, an empty field creates nothing. The page
  buttons are now "New asset", "New <damage scenario>", "New control" and start a draft;
  the three `ask()` name dialogs are gone. The table gets a selected draft row at the end
  (text translated in Slint, read by the controller).
- Strings: ja and pseudo catalogs updated. No new dependency.

### Verified
- `crates/aegis-client/tests/session.rs`: begin then cancel sends nothing and leaves no
  history; an empty name discards; create-and-next creates and opens a new draft with a new
  sequence number; one undo removes a created asset.
- `crates/aegis-shell/tests/drafts.rs` with real key events: an asset created with the
  keyboard alone; Escape on a control draft leaves `confirmed_seq` unchanged and no undo
  entry; three damage scenarios created with Ctrl+Enter and a fourth draft left open and
  then discarded. Screenshots `draft-asset.png`, `draft-damage-next.png` viewed.
- fmt, clippy `-D warnings`, `cargo test --workspace --locked` (83 binaries, 0 failed),
  `scripts/deny.sh`.

### Not built yet (later slices)
- Threat, goal, claim, tree, component, interface, data flow, trust boundary drafts; the
  required-field handling (UI-08.2) and the "incomplete" mark (S2, S3).
- The draft row is plain text, not italic grey (the table cell cannot style one row); an
  inline error in the inspector (UI-08.6); `NameEditor` save on Tab/focus loss and removal
  of the Rename button (S4); the + and ⌘/Ctrl+Shift+N shortcut, read-only reasons on the
  button, the start-screen inline card, removing the remaining `ask()` sites, the
  key-press measurement and the Japanese IME check by hand (S5).
