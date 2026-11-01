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

## Slice 2 (threat scenario, goal, claim, attack tree) — built test-first

### Red first
The tests were written before the code and failed for the right reasons: the client tests
did not compile (no `DraftKind::Threat`, `Goal`, `Claim`, `Tree`, no `SetDraftCategory`, no
`draft_incomplete`), and the window tests lacked the callbacks and properties. A later
assertion that the threat draft row exists failed on its own (the controller skipped the
threat page when only the draft changed) and went green with the fix.

### Built
- **Client core:** `DraftKind` gains Threat, Goal, Claim, Tree. A threat draft has two
  required fields, the asset and the category (`UiState.draft_asset`, `draft_category`,
  `draft_incomplete()`; `Command::SetDraftAsset`, `SetDraftCategory`). `BeginDraft` fills the
  asset from the context: the selected asset if there is one, else the only asset. Commit
  with a missing required field creates nothing and keeps the draft; an empty name still
  discards. `next` keeps the chosen asset and category for the following draft. Goal and
  claim are created with `AddGoal`/`AddClaim`, a tree with `NewTree` (free tree, name is the
  root), all through the one edit path, so each is one undo step.
- **Window:** the inline "Add a threat scenario" form (F6) is gone; the page header has "New
  threat scenario". The threat draft shows Asset and Category choices; Enter or Tab with a
  field missing moves focus to the first missing one, and when the last one is set focus
  returns to the name so Enter creates. A Create button (disabled with "Choose the required
  fields" while incomplete) is on every draft form. A threat draft does not create on
  focus loss (the other fields would trigger it). Goal and claim drafts are in the goals
  inspector column, the tree draft under the tree list. The `ask()` dialogs for goal, claim
  and tree are gone. The controller adds the draft row to the threat, tree, goal and claim
  tables.
- Strings: ja and pseudo catalogs; no new dependency.

### Verified
- `crates/aegis-client/tests/session.rs`: a threat draft with no asset is incomplete and
  creates nothing; with one asset the asset is the context; a missing category blocks
  commit; setting it completes the draft; commit creates the threat; one undo removes it.
  Goal, claim and tree drafts are created from a name alone.
- `crates/aegis-shell/tests/drafts.rs` with real keys: the threat draft row appears and is
  selected, Enter with no category creates nothing, choosing the category then Enter creates
  the threat; goal, claim and tree are created with the keyboard. Screenshot
  `draft-threat-incomplete.png` viewed.
- fmt, clippy `-D warnings`, `cargo test --workspace --locked` (83 binaries, 0 failed),
  `scripts/deny.sh`.

### Not built yet
Item-definition drafts (S3), `NameEditor` save semantics and Rename removal (S4), read-only
reasons on the header buttons, the shortcut, the start-screen card and the key-press count
(S5). Still open from slice 1: draft row styling, an inline error in the inspector. A tree
draft is a free tree; attaching to the selected threat stays on the threat page's "New tree".
