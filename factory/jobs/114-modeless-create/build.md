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

## Slice 3 (component, interface, data flow, trust boundary) — built test-first

- **Red first:** the client test (`item_definition_drafts_wait_for_their_required_fields`) and the window test
  (`item_definition_drafts_are_created_with_the_keyboard`) were written and failed to compile before any code.
- **Client core:** `DraftKind` gains Component, Interface, DataFlow, TrustBoundary (page: Item). Required
  fields: an interface needs a component and a kind; a data flow needs two different components. The component
  (interface) or "from" end (flow) comes from the context: the selected component, else the only one. New
  commands `SetDraftComponent`, `SetDraftTo`, `SetDraftInterfaceKind`; `draft_incomplete` also covers "from and
  to are the same component".
- **Window:** one contextual "New component / interface / data flow / trust boundary" button on the Item page
  header; a draft opens its tab. The inline interface and data flow forms and the two `ask()` dialogs are gone.
  Draft forms for each kind (choices for component, kind, from, to) with the same focus rules as the threat
  draft; draft rows in the four tables.
- **Verified:** client and window tests above (real keys; Enter with a missing field creates nothing, choosing
  the field then Enter creates; same component for both ends stays incomplete); fmt, clippy `-D warnings`,
  `cargo test -p aegis-shell -p aegis-client --locked`.

## Slice 4 (names save without a Rename button, UI-08.3) — built test-first

- **Red first:** `crates/aegis-shell/tests/name_editor.rs` (real keys and clicks) failed at "saved by Tab"
  before the change; a pixel check for the "A name is needed" note failed next and exposed a real bug (an
  empty text equalled the empty "already sent" marker, so the note never showed).
- **Built:** `NameEditor` (used by every details panel) saves on Enter, on Tab and when focus leaves
  (including a click on another table row; the name still goes to the element that was selected when it was
  typed); Escape puts the saved name back and sends nothing; an empty name is never saved: the saved name
  returns and a `FieldError` "A name is needed" shows until the next keystroke. The Rename button is gone. A
  guard stops Enter followed by focus loss from sending the same name twice. ja and pseudo strings added.
- **Verified:** the test above (Enter, Tab, focus loss, Escape sends no op, empty name not saved and the note
  drawn); fmt, clippy `-D warnings`, `cargo test -p aegis-shell --locked`.

## Slice 5 (read-only reasons, shortcut, inline create error, measurement) — built test-first

- **Red first:** window tests (`the_new_element_shortcut_starts_a_draft_of_this_page`,
  `a_read_only_user_gets_no_draft_and_a_reason`, `five_assets_take_the_keyboard_alone`) and the client test
  (`a_refused_draft_stays_and_says_why_in_the_draft`) did not compile before the change.
- **Built:**
  - ⌘/Ctrl+Shift+N (the table's existing "new-element" shortcut, until now a stub) starts the draft of the page
    on screen, from a new Edit-menu item and from the command palette; on the Item page it follows the tab.
  - UI-08.5: every header "New ..." button is disabled while the user cannot edit and its tooltip says why
    ("Read only: <who is editing>", or "Read only: take the edit lock first."). A read-only user gets no draft.
  - UI-08.6: a refused create keeps the draft and the typed text and shows the reason in the draft form
    (`FieldError`), not as a toast; the next draft or a good commit clears it.
  - UI-08.7: the start screen already has the inline new-project card (name, methodology, Create); no dialog
    there. The File menu's "New project…" keeps its name prompt for use inside a project.
- **Measured (UI-08 done-when):** five assets with the keyboard alone take 37 key presses (31 letters + 1 chord
  to start + 4 Ctrl+Enter + 1 Enter) and no mouse. The old flow needs, per asset, a click on "Add asset", the
  letters and an Enter in the dialog: 41 inputs of which 5 are mouse clicks, and a dialog opens five times.
  The headless run takes 0.4 s; a person's time was not measured.
- **Verified:** fmt, clippy `-D warnings`, `cargo test --workspace --locked` (84 binaries, 0 failed),
  `scripts/deny.sh`.

## Summary of job 114

All eleven element types are created from a draft in the inspector (asset, damage scenario, threat scenario,
attack tree, security control, goal, claim, component, interface, data flow, trust boundary). Nothing reaches the
server until the element is complete; Escape or an empty name sends nothing; create is one undo step; names save
on Enter, Tab and focus loss with no Rename button. Eleven `ask()` dialogs and the inline add forms are gone. No
new dependency; Slint's text input and IME are reused.

### Not done, for a person
- UI-08.4: confirm by hand that the Enter which confirms an IME composition does not create the element, on macOS
  (Japanese Romaji and Kana) and Windows (Microsoft IME). The code uses Slint's own IME path; there is no test.
- Advisory: the draft row is plain text, not italic grey (a table cell cannot style one row); the draft row and
  inspector need two frames in the headless tests (Slint runs `changed` handlers on the next turn), which is not
  a measured one-frame guarantee.
