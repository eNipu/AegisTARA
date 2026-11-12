# ADR 0068: Generic element store

- Status: proposed (job 205-generic-element-store, slices 205a to 205f)
- Date: 2026-11-12
- Crate: `crates/aegis-model` (`src/element.rs` for the kind table, `Element`
  and the six ops; `src/store/` for the snapshot, op log and workspace
  files; `src/diff.rs` and `Project::diff` for the diff; the caps in
  `src/element.rs` and `src/store/mod.rs`)
- Rules: P3 (one place per rule), P4 (deterministic output), P7 (business
  value per line of code), R3 (decode caps) in `docs/engineering-standards.md`
- Decisions: D34 in `docs/decisions.md` (packs are data; extra attributes on
  element kinds), D43 (every change goes through the business logic)
- Builds on: `0065-workspace-files.md` (one file per group, the envelope),
  `0009-save-load-format.md` (strict, capped JSON decoding)

## Context

Each element kind that the engine does not compute on costs about a thousand
lines today: an arena, a typed ID and index, three to six ops with inverses,
snapshot and op log wire types, a workspace file entry, and diff and equality
code. The backlog has seven more such kinds (misuse cases, parties,
functions, requirements, verifications, glossary terms, documents). D34 also
needs a place for attributes that a methodology pack declares, such as an
assurance level, without kernel code for each.

This ADR records the store that holds such kinds. Misuse cases (`MU`, linked
N:M to threat scenarios) are the first row and the proof. Existing typed kinds
are not moved into the store: they change risk numbers, and their diffs and
files stay as they are.

## Decision

### Kind table

`KINDS` in `src/element.rs` is the one kernel table, in `ElementKind` order.
Each row is a `KindDef`:

| Column | Meaning | Misuse case value |
|---|---|---|
| `kind` | the `ElementKind` variant | `MisuseCase` |
| `key` | file and op log key, pack key syntax | `misuse-case` |
| `prefix` | visual ID prefix | `MU` |
| `file_stem` | workspace file `systems/main/<stem>.json` | `misuse` |
| `label` | English singular for messages | `misuse case` |
| `targets` | what an element of this kind may link to | `[Threat]` |
| `max_elements` | most live elements of this kind | 10,000 |
| `max_links` | most links per element | 256 |

A new kind is one row plus one `ElementKind` variant. Tests check that keys
and prefixes are unique and well formed, that a prefix is unlike every
`VisualPrefix`, and that each row's caps do not exceed the global caps.

### Element

An `Element` holds its kind, a name, its `Provenance`, its attributes sorted
by key (keys unique) and its links sorted and unique. The name is 1 to 256
characters and not blank. Attributes are `AttrValue`: `Level(key)` (a key of
an ordered scale), `Choice(key)` (a key of an unordered list) or `Text`
(free text). A key is `[a-z][a-z0-9_-]{0,63}`, the pack key syntax. A text
value is 1 to 256 characters, not blank, with no control characters.

Links are `LinkTarget`s. The only target kind now is a threat scenario, so
nothing links to a generic element yet and an element is never "in use".

Descriptions on elements are not part of this ADR. `ElementRef` is not
extended; job 148 adds them with the misuse case page.

### Ops

Six ops change elements. Each one is checked by `validate` before it is
applied, and each returns an inverse that restores the project exactly (`==`).

| Op | Inverse | Guards (each has a test that only that guard refuses) |
|---|---|---|
| `AddElement` | `RemoveElement` | duplicate ID; name empty, blank or over 256 characters; kind full (`max_elements`); blank actor |
| `RenameElement` | `RenameElement` with the old name and provenance | unknown ID; name rules; blank actor |
| `RemoveElement` | `AddElement` with the old kind, name and provenance | unknown ID; refused while the element has any attribute (`ElementHasAttributes`) or any link (`ElementHasLinks`) |
| `SetElementAttribute` | the same op with the old value (`None` if unset) | unknown ID; bad key (`InvalidAttributeKey`); bad value syntax (`InvalidAttributeValue`); 33rd attribute (`TooManyAttributes`, cap 32) |
| `LinkElement` | `UnlinkElement` | unknown ID; target kind not allowed by the row (`LinkNotAllowed`); unknown threat; duplicate link; over `max_links` (`TooManyElementLinks`) |
| `UnlinkElement` | `LinkElement` | unknown ID; link not there (`ElementLinkMissing`) |

Removing a threat scenario is refused while any element links it
(`ThreatLinkedFromElements`). Its inverse is unchanged.

Why the removal inverse is a plain add: `RemoveElement` is refused while the
element has attributes or links. The editor clears them first in the same
batch, as it does for descriptions today (lesson L04). So the element that
`RemoveElement` removes is always bare, and `AddElement` restores it exactly.

Every op records provenance, and `validate` checks it for a blank actor on
each op that carries it, including the ops that return early (lesson L15).

Error text never contains a name or a value; a test checks this.

### Visual IDs

Visual numbers are per kind (`MU00`, `MU01`, ...). A number is never reused
after a removal. The store remembers the number of a removed element so that
an undone removal gets it back. After a load, the next number is the largest
plus one, as for typed kinds. The snapshot always writes `visual_id` for
elements.

### Attribute values are checked by syntax only

A pack cannot declare attributes yet (job 176, MTH-02). So a value is checked
for syntax only. A value whose key the pack no longer declares still loads;
it becomes a lint finding when job 176 adds the check. A pack update therefore
never makes a saved project unreadable.

### Formats

- Snapshot: an `elements` object, mapping each kind key to a list of elements
  in ID order. A project with no elements saves exactly the bytes it saves
  today, and `tests/fixtures/project-v1.json` loads unchanged.
- Op log: one JSON line per op with `"op"` set to `add_element`,
  `rename_element`, `remove_element`, `set_element_attribute`,
  `link_element` or `unlink_element`. The reader refuses an unknown kind key
  as `StoreError::UnknownKey` with `VocabKind::ElementKind`, and it refuses
  over-long names, keys and texts.
- Workspace: misuse cases go to `systems/main/misuse.json` (envelope `kind`
  `misuse-case`, key `elements`). The file is written only when there is at
  least one element. Reading and writing it round-trip to equal `save()`
  bytes, including a project with more than 100 misuse cases (lesson L06).

### Diff

`Project::diff` compares the live elements in one merge walk over the two
ID-ordered lists. An ID only in the baseline is removed, only in the current
project is added, and in both with an unequal element (kind, name, provenance,
attributes, links) is modified. Visual numbers, removed numbers and inbound
counts are ignored, so the diff is empty exactly when `Project`'s equality
says the projects are equal. Element changes add no risk delta.

### Caps

| Cap | Value | Where | Reason |
|---|---|---|---|
| `MAX_ELEMENTS_PER_KIND` | 10,000 | `src/element.rs` | Same order as the typed arenas; a choice, not an existing cap |
| `MAX_ELEMENT_LINKS` | 256 | `src/element.rs` | One element links a few threats; the bound stops one element from growing without limit |
| `MAX_ELEMENT_ATTRIBUTES` | 32 | `src/element.rs` | Pack attributes are few; the cap bounds the per-element decode |
| `MAX_ATTRIBUTE_TEXT_CHARS` | 256 | `src/element.rs` | Same as the description of a short field; longer text belongs in a description |
| `MAX_WORKSPACE_FILES` | 10,011 | `src/store/mod.rs` | Ten fixed files, one misuse file per kind (one today), and up to 10,000 attack trees |

Each cap has a test just under and just over its value. The decode caps of
ADR 0009 apply to the snapshot, and the workspace reader applies them to the
joined snapshot as before.

## Fuzz result

Run on 2026-11-12 with `crates/aegis-model/fuzz/run.sh 60 <target>` (stable
toolchain, libFuzzer coverage, no sanitizer):

| Target | Runs | Seconds | Result |
|---|---|---|---|
| `snapshot` | 1,468,292 | 61 | no crash, no property failure |
| `op_log` | 995,857 | 61 | no crash, no property failure |
| `workspace` | 1,371,114 | 61 | no crash, no property failure |

The runs used the binaries that `run.sh` builds (same RUSTFLAGS, no
sanitizer), started from the shared target folder because `CARGO_TARGET_DIR`
is set in the factory worktree; `run.sh` itself looks in the fuzz crate's
own `target` folder and failed to find them. The fuzz targets and the script
are unchanged. Corpus and artifacts are git-ignored; no artifact was written.

## Left for later

- Descriptions on elements (`ElementRef`) and the misuse case page (job 148).
- Pack-declared attributes and checking values against them (job 176).
- Links to generic elements, and links from typed elements.
- The next kind rows: parties, functions, requirements, verifications,
  glossary terms, documents.

## Consequences

- Adding a kind is a table row, one `ElementKind` variant and its tests. The
  next kind costs its row, not a new arena and set of ops.
- The typed kinds stay as they are. Their diffs, files and numbers do not
  change in this job.
- The misuse case's ops and files exist before any UI, so job 148 can build
  on them without changing the format.
