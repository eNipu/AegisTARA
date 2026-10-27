# ADR 0051: Description text on every element and notes on the project

- Status: accepted (job 064-element-descriptions)
- Date: 2026-10-27
- Scope: `aegis-model` (`description.rs`, `op.rs`, `project.rs`, `diff.rs`, `store/`), `aegis-client` (`commands.rs`, `view.rs`, `report.rs`, `pdf.rs`), `aegis-shell` (`ui/`)
- Rules: R3, D10, P4 in `docs/engineering-standards.md`
- Requirement: FND-01
- Builds on: `0009-save-load-format.md`, `0013-element-provenance.md`

## Context

An analyst needs to say why an asset matters, what a threat means, what the
scope of the project is. Names are capped at 256 chars and say nothing more.
FND-01 asks for plain long text on every element and notes on the project,
capped, versioned, diffed and exported, with v1 files still loading.

## Decisions

### 1. One op and one map, not a field on every entity

`Op::SetDescription { target: ElementRef, text, provenance }` sets the text of
an asset, damage scenario, threat scenario, attack tree, security control,
cybersecurity goal or claim, or of the project (`ElementRef::Project`, the
notes). Empty text clears it. `Project` keeps `BTreeMap<ElementRef,
Described>`: ordered by ID, so every output is deterministic (P4), free for a
project without descriptions, and no entity struct changes. The inverse is the
same op with the old text and provenance, so undo restores both.

A field on each `Add*` op was rejected: about 640 construction sites across the
workspace would change, and every v1 op line and fixture with it.

### 2. An element with a description cannot be removed

`Remove*` is refused with `DescriptionNotCleared` while a description exists,
so the inverse of a removal (an `Add*` op) never has to carry text. A client
deletes by clearing first, in the same batch (`Edit::to_ops` does this for
every removal in an edit); undo applies the inverses in reverse, so one undo
step brings back the element and then its text. This also keeps the map free
of entries for elements that no longer exist.

### 3. Plain text with caps (R3)

At most 8,000 chars for an element and 15,000 for the notes, and 16 MiB of text
in all per project. Tabs and line breaks are text; NUL and other control
characters (including `\r`, which the client turns into `\n`) are refused with
a message that carries no text. Decoding caps the string at 60,000 bytes before
validation, and the op writer refuses a record the reader would refuse. Four
bytes a char at the notes cap stays under the 64 KiB op record cap.

### 4. Formats stay compatible

The op log gains a `set_description` record; old records are unchanged. The
snapshot gains an optional `descriptions` list written last and applied last
on load, only when there is one: a project without descriptions is saved byte
for byte as before (a test checks it), and every v1 fixture still loads.
Description provenance is the provenance of the op that wrote the text.

### 5. Diff, reports, window

- `Project::diff` lists an element whose description changed as *modified* and
  sets `ProjectDiff::notes_changed()`. The merge runs after the risk deltas, so
  a text change produces no risk change.
- The HTML and PDF reports print the notes under the title and a description
  under each threat and tree; for assets, damage scenarios, controls, goals and
  claims a "Descriptions" list follows the table. Nothing is added when there is
  no text, so existing reports are unchanged. Text is escaped in HTML. The CSV
  risk register is a register and is not changed.
- Every page that shows an element has a description box next to its details
  (assets, damage, threats, risks, trees, controls, goals, claims), and the
  Overview has the project notes. The page shows the text read-only; **Edit**
  opens a window with the text box. A text box on the page itself would keep
  the Tab key and trap a keyboard user (the Tab-reaches-the-canvas test caught
  this), so the window is where the text is typed, with Ctrl+Enter to save and
  Escape to cancel. A box saves one change, not one per key press.

### 6. Left for later

Attack tree *nodes* (steps and gates) have no description yet: they have no
ops of their own until job 065. A language tag and translations are job 069.
The outline editor's text box has the same Tab behaviour as the new window.

## Consequences

- One more op (38), one more snapshot list, one more error group.
- Text is kept in memory only for elements that have some (D10: derived byte
  count, rebuilt on load).
