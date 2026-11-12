# Spec: 205-generic-element-store

## Title
A generic element store in `aegis-model`, with misuse cases as the first kind (architecture review 2026-11-11; D34, D43; FND-13 model part)

## Design Reference
- `factory/backlog.md`, the line ending `(205-generic-element-store)`.
- `docs/decisions.md` D34 (packs are data; extra attributes on element kinds), D43 (every change goes through the business logic).
- `docs/design/workbench-architecture.md` section 3.2 (misuse case: system level, links threats), section 6.2 (extra attributes: level, choice or text), section 7 (`systems/<name>/misuse.json`).
- `docs/architecture.md` III.9.3 (the element recipe: ops with exact inverses, store, diff).
- `docs/requirements.md` FND-13 (misuse cases linked N:M to threat scenarios).
- Code: `crates/aegis-model/src/{id.rs,op.rs,project.rs,error.rs,diff.rs,lib.rs}`, `crates/aegis-model/src/store/{mod.rs,wire.rs,op_log.rs,snapshot.rs,workspace.rs}`, a new `crates/aegis-model/src/element.rs`; tests in `crates/aegis-model/tests/` and `crates/aegis-client/tests/undo_every_op.rs`; `docs/adr/0068-generic-element-store.md`, `docs/architecture.md`.
- Not touched: `aegis-server`, `aegis-shell`, `aegis-client/src`, the methodology pack format, the existing typed kinds.

## Problem & Goal
Each element kind today costs an arena, a typed ID and index, three to six ops with inverses, snapshot and op log wire types, a workspace file entry, diff and equality code: roughly a thousand lines per kind. The backlog has at least seven more kinds that the engine never computes on (misuse cases, parties, functions, requirements, verifications, glossary terms, documents). D34 also needs a place for pack-declared extra attributes.

After this job, `Project` holds one store for such kinds. A kind is one row in a kernel table (key, visual ID prefix, file name, allowed link targets, caps). Six generic ops change elements through the business logic (D43), each with an exact inverse. The store is saved in the snapshot, the op log and one workspace file per kind, and shows in the project diff. Misuse cases (`MU`, linked N:M to threat scenarios) are the first row and the proof.

Business value (P7): every later kind line costs a table row and its tests instead of a new arena, so parties, requirements and glossary reach consultants sooner and with less code to keep correct; pack attributes such as a cybersecurity assurance level (HEAVENS, IEC 62443) get a home without kernel code.

## Decisions taken in this spec
- **Which kinds:** only misuse cases now. Existing typed kinds are not migrated; conditions, assumptions and shared steps stay typed (they change numbers).
- **Attributes before pack declarations:** packs cannot declare attributes yet (that is job 176, MTH-02). This job stores values by attribute key and checks syntax only: key `[a-z][a-z0-9_-]{0,63}`; a `Level` or `Choice` value is a key of the same syntax; a `Text` value is 1 to 256 chars, not blank, no control characters. Job 176 adds the check against the pack's declarations. A value whose key the pack no longer declares still loads (it becomes a lint finding later), so a pack update never makes a saved project unreadable.
- **Links:** stored on the element, sorted. The only target kind now is a threat scenario. Nothing links to a generic element yet, so an element is never "in use".
- **Removal guard (lesson L04):** `RemoveElement` is refused while the element has any attribute or any link; the editor clears them first in the same batch (as for descriptions today). So the inverse of `RemoveElement` is a plain `AddElement` and stays exact. `RemoveThreatScenario` is refused while any element links the threat; its inverse is unchanged.
- **Visual IDs:** numbered per kind (`MU00`, `MU01`, ...), never reused after removal, restored by undo, the next number after load is the largest plus one, as for typed kinds. The snapshot always writes `visual_id` for elements.
- **No descriptions on elements in this job:** `ElementRef` is not extended; job 148 (misuse cases UI) adds that.

## Acceptance Criteria
1. **Kind table.** `aegis-model` exports `ElementKind` (one variant, `MisuseCase`), `KindDef`, `KINDS`, `TargetKind`, `LinkTarget`, `AttrValue`, `Element`, `ElementId` and the caps `MAX_ELEMENTS_PER_KIND` (10,000), `MAX_ELEMENT_LINKS` (256), `MAX_ELEMENT_ATTRIBUTES` (32), `MAX_ATTRIBUTE_TEXT_CHARS` (256). The misuse case row is key `misuse-case`, prefix `MU`, file stem `misuse`, targets `[Threat]`. A test checks: keys and prefixes unique, prefixes unlike every `VisualPrefix`, each row's caps no greater than the global caps. Check: `cargo test --locked -p aegis-model --test elements`.
2. **Ops.** `Op` gains `AddElement`, `RenameElement`, `RemoveElement`, `SetElementAttribute`, `LinkElement`, `UnlinkElement`. Each applied then its inverse applied gives a project equal to the one before (`==`), and the visual ID comes back after an undone removal. Check: `cargo test --locked -p aegis-model --test elements` and `cargo test --locked -p aegis-client --test undo_every_op`.
3. **Guards, each with a test that only that guard refuses and that checks the error variant:** unknown or duplicate ID; empty, blank or over-long name; kind full (`max_elements`); bad attribute key; bad attribute value; 33rd attribute; link target kind not allowed by the row; unknown threat; duplicate link; unlinking a link that is not there; link over `max_links`; removing an element with attributes; removing an element with links; removing a threat an element links. Error text never contains a name or a value.
4. **Op log.** All six ops write and read back equal (`OpLogWriter`, `OpLogReader`); an unknown kind key is refused as `StoreError::UnknownKey` with `VocabKind::ElementKind`; over-long names, keys and texts are refused by the decoder.
5. **Snapshot.** A project with elements saves an `elements` object (kind key to a list in ID order) and loads back with equal `save()` bytes; a project without elements saves exactly the bytes it saves today, and `tests/fixtures/project-v1.json` loads unchanged. Decode caps: elements per kind, attributes per element, links per element, each with a just-under and just-over test.
6. **Workspace files.** Misuse cases go to `systems/main/misuse.json` (envelope `kind` `misuse-case`, key `elements`), written only when there is at least one. `from_workspace_files(to_workspace_files(p))` gives equal `save()` bytes, including a project with more than 100 misuse cases (lesson L06). `MAX_WORKSPACE_FILES` grows by one per kind.
7. **Property tests.** The random op generator in `tests/common/mod.rs` produces all six ops; `ops_properties`, `store_properties` and `workspace_properties` pass with them.
8. **Diff.** `ProjectDiff::elements()` lists added, removed and modified element IDs in ID order, and `ProjectDiff::is_empty()` counts them. An element op changes no risk number: `risk_deltas()` stays empty.
9. **Written down.** `docs/adr/0068-generic-element-store.md` records the table, the ops, the guards, the formats and the caps; `docs/architecture.md` lists `element.rs`.
10. **Gate.** `scripts/ci.sh` passes on the whole workspace. No new dependency.

## Out of scope
- Pack declarations of attributes, and checking values against them (job 176).
- Any UI, client page, relation registry row, undo label, report or XLSX column (job 148 for misuse cases).
- Descriptions on elements (`ElementRef`), lint rules, links to generic elements, links from typed elements.
- Moving any existing typed kind into the store.
- Parties, functions, requirements, verifications, glossary and documents (each a later table row).
