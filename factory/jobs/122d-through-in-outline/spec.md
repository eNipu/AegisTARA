# 122d-through-in-outline

## Title

Through and attack type in the outline text and the library

## Design Reference

- `factory/backlog.md` line 108 (this job), lines 105 to 107 (jobs 122, 122b, 122c it follows).
- `docs/requirements.md`: CFG-01 (a step names what it goes through), CFG-02 (a step carries at most one attack type), LIB-03 and LIB-04 (library insert).
- `docs/decisions.md`: D26 (via and attack types), D14 (the library inserts copies with an origin note).
- Code:
  - `crates/aegis-client/src/outline.rs` (`to_outline`, `parse_line`, `parse_outline`, `OutlineError`)
  - `crates/aegis-client/src/commands.rs` lines 1694 to 1701 (`Edit::ReplaceTree`)
  - `crates/aegis-client/src/tree_diff.rs` (`replace_tree_ops`, `clear_step_ops`, `transfer_step_ops`)
  - `crates/aegis-client/src/session.rs` lines 3327 to 3334 (the outline text the editor shows)
  - `crates/aegis-model/src/library.rs` (`ItemNode`, `check_tree`), `crates/aegis-model/packs/library-demo.json`
  - `crates/aegis-client/src/library.rs` (`Builder::add`, `insert_ops`, `LibraryView::preview`)

## Problem & Goal

Since job 122 a step can name the component or interface it goes through and an attack type, and
since 123 these two values decide which security settings cap the step. Today they can be set only
one step at a time in the inspector. The outline editor, the fastest way to write a tree, neither
shows nor accepts them, and a library item cannot bring an attack type along.

Business value (P7): a consultant who types or pastes a tree of 30 steps sets "through" and
"attack type" in the same pass instead of 60 inspector picks, and library items arrive with the
attack type filled in, so the settings of job 123 start working on a new tree without re-typing.
Nothing is added to the screen: the existing outline editor and library panel carry it.

## Decisions taken in this spec

1. **Syntax.** After the text of a step: ` | through: C00 | type: debug-access`. `through` takes
   the visual ID of a component (`C00`) or an interface (`IF00`), letters in either case. `type`
   takes the attack type key of the project's methodology. The value `-` means "none". Either
   order is accepted; the writer writes `through` first.
2. **The text decides.** A step line with no `through` has no through item after the outline is
   applied, and the same for `type`. The editor text is written with the values, so an unchanged
   text changes nothing. This is the only way to clear a value in text. It changes one existing
   behaviour: a plain outline sent to `Edit::ReplaceTree` now clears the two values (one test in
   `step_via.rs` is updated).
3. **A pipe in a name stays a name.** Only the last two ` | ` segments are looked at, and only if
   they have the shape `word: value` (word = lower-case ASCII letters). `Read A | B` is a name.
   A segment of that shape with another word (`| throgh: C00`) is an error that names the line,
   so a typo is never silently kept as text.
4. **Exact round trip for odd names.** A name whose own last segment has the shape `word: value`
   is written with both values, `-` for none (`A | note: x | through: - | type: -`). Once both are
   read, everything to the left is the name. `to_outline` does this too, so every text it writes
   parses back to the same names.
5. **Gates and shared steps.** A gate may not carry a value other than `-`. A shared step (same
   text on several lines) carries its values on its first line; a later line may repeat the same
   values or carry none; different values are an error.
6. **Where unknown IDs are caught.** `parse_outline` knows no project, so it returns the raw
   values with their line; `Edit::ReplaceTree` resolves them and returns
   `OutlineError::UnknownItem(line)` or `OutlineError::UnknownAttackType(line)`. Nothing is
   applied when an error is returned.
7. **Library.** `ItemNode` gets an optional `attack_type` key (steps only). The insert sets it on
   each copied step when the project's methodology has that key and skips it otherwise (packs are
   methodology-neutral). It is filled on every insert, with or without ratings, because an attack
   type alone changes no number (a ceiling needs a through item too). It is not silent: the
   preview shows `{type: key}` on the step, and the origin note in the tree's description says on
   how many steps a suggested attack type was filled in. No review mark: that mark means "the
   rating needs review" and blocks removal. The value is a plain `SetStepAttackType`, editable in
   the inspector and the outline like any other.
8. **Pack format.** `aegis-library` stays at version 1. The field is omitted when empty, so
   existing packs keep their hash. A pack that uses the field is refused by older builds (unknown
   field), which is the existing rule.
9. **Report.** The report keeps the plain outline; job 122c already prints a Through and Attack
   type table under each tree.

## Acceptance Criteria

1. `parse_outline_annotated("OR Goal\n  - Flash | through: C00 | type: debug-access\n", None)`
   returns a tree whose step is named `Flash` and one note with line 2, `through = Some("C00")`,
   `attack_type = Some("debug-access")`. Check: `cargo test --locked -p aegis-client --test outline_notes`.
2. `parse_outline` returns the same tree as `parse_outline_annotated` for the same text (the
   values are stripped from the name). Check: same test file.
3. A name that contains a pipe is kept: `- Read A | B`, `- Ratio | Then: go`. Check: same test file.
4. Unknown keys and bad forms are errors with the line: `| throgh: C00` gives `UnknownNote(2)`,
   `| type: a | type: b` gives `DuplicateNote(2)`, a value on a gate gives `NoteOnGate(n)`, two
   lines of one step with different values give `NoteConflict(n)`. Check: same test file.
5. For every name in a list that includes `A | note: x`, `A | type: x`, `A | through: - | type: -`,
   the text written by `to_outline` and by `to_outline_with` parses back to the same name and the
   same values. Check: same test file.
6. Round trip through a project: with a through item and an attack type set on a step,
   `to_outline_annotated` contains ` | through: C00 | type: <key>`, and applying that text with
   `Edit::ReplaceTree` yields no ops. Check: `cargo test --locked -p aegis-client --test outline_apply`.
7. Editing the text sets, changes and clears the two values (component and interface), on kept,
   new and moved steps, and one undo restores the project exactly. Check: same test file.
8. An unknown item gives `EditError::Outline(OutlineError::UnknownItem(line))`, an unknown type
   gives `OutlineError::UnknownAttackType(line)`, and the project is unchanged. Check: same test file.
9. The Trees page data (`d.outline`) carries the values. Check: same test file, through
   `to_outline_annotated`; and `grep -n "to_outline_annotated" crates/aegis-client/src/session.rs`.
10. A library pack with `"attack_type"` on a step loads; on a gate it is refused; packs without
    the field keep their hash. Check: `cargo test --locked -p aegis-model --test library`.
11. Inserting an item whose step suggests an attack type the methodology has sets it on the copy,
    in the same undo step, and the tree's description says how many were filled in; a key the
    methodology lacks is skipped and the insert succeeds; the preview shows `{type: key}`.
    Check: `cargo test --locked -p aegis-client --test library_insert --test library_view`.
12. The demo pack suggests an attack type on at least two steps and still loads.
    Check: `cargo test --locked -p aegis-model --test library`.
13. The whole gate passes. Check: `scripts/ci.sh`.

## Out of scope

- Any new widget, tick box or dialog (no Slint screen work). The existing outline editor and
  library panel show the new text as they are.
- Names of components next to the IDs in the outline text, and completion while typing.
- Ratings, factors and controls in the outline text.
- The through item in library items (it belongs to one project).
- The outline in the report and the exports (122c covers them).
- A lint for an attack type the pack suggests but the methodology lacks.
