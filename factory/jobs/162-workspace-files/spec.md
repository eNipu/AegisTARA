# Spec — 162-workspace-files

## Title
Write a project as a set of workspace files and read it back (WSP-01, WSP-02; D32)

## Design Reference
- Reference: `docs/decisions.md` D32; `docs/design/workbench-architecture.md` section 7; `docs/design/workbench-breakdown.md` group 1; `docs/adr/0009-save-load-format.md`; `docs/adr/0011-methodology-pack-files.md`.
- Affected crates/paths:
  - `crates/aegis-model/src/store/{mod.rs,snapshot.rs,wire.rs}` and a new `crates/aegis-model/src/store/workspace.rs`
  - `crates/aegis-model/src/{lib.rs,error.rs}`
  - `crates/aegis-model/tests/{workspace.rs,workspace_limits.rs,workspace_properties.rs}` (new), `tests/common/`
  - `crates/aegis-model/fuzz/fuzz_targets/workspace.rs` (new)
  - `crates/aegis-model/benches/` (one new benchmark)
  - `docs/adr/` (one new ADR, next free number), `docs/architecture.md` (store section)
- Not touched: `aegis-server`, `aegis-client`, `aegis-shell`, the snapshot format, the op log format.

## Problem & Goal
A project is saved today as one JSON snapshot. One file cannot be reviewed in git, two people cannot change it without a conflict, and a system cannot have its own folder. D32 decides that a project is saved as a folder of small JSON files.

This job adds the pure part: turning a `Project` into a list of `(path, bytes)` and back. It does not touch the disk. Later jobs write the files (163, 164), add one folder per system (170) and replace `item.json` with Markdown (193).

Business value: this is the base for file history, per-system TARAs, templates and the workbench. Nothing else in the rethink can start without it.

## Acceptance Criteria

1. **API (WSP-01).** `aegis-model` exports:
   - `WorkspaceFile { path: String, bytes: Vec<u8> }`
   - `Project::to_workspace_files(&self) -> Result<Vec<WorkspaceFile>, StoreError>`, sorted by `path`
   - `Project::from_workspace_files(files, methodology: Arc<Methodology>) -> Result<Project, StoreError>`, where `files` yields `(&str, &[u8])` in any order

   Neither function reads or writes the file system, the clock or the environment.

2. **Layout.** The file set for today's model (which has no systems yet) is:

   ```
   aegis.json                       workspace facts: methodology key and version, languages, visual ID counters
   descriptions.json                the descriptions side table
   review-marks.json                the review marks side table
   shared/damage.json
   shared/controls.json
   shared/goals.json
   shared/claims.json
   systems/main/item.json           components, interfaces, data flows, trust boundaries, item text
   systems/main/assets.json
   systems/main/threats.json
   systems/main/trees/<visual-id>.json    one file per attack tree, name from the tree's visual ID in lower case
   ```

   - `aegis.json` is always written. Any other file is written only when it has content, as the snapshot omits empty sections today.
   - **Every top-level field of the snapshot lands in exactly one file.** A test builds a project with one instance of everything (`common::record_every_op`), and fails if a snapshot field is in no file. A field added to the snapshot later without a home must fail that test.
   - Paths use `/`, lower-case ASCII letters, digits, `-` and `.` only.

3. **Envelope (WSP-02).** Every file is a JSON object that starts with `"format":"aegis-workspace"`, `"version":1`, `"kind":"<kind>"`, where kind is one of `workspace`, `descriptions`, `review-marks`, `damage`, `controls`, `goals`, `claims`, `item`, `assets`, `threats`, `tree`. The rest of the object is the same content, with the same stable keys, that the snapshot writes for that section. A file whose `kind` does not match its path is refused.

4. **Round trip.** For every fixture in `tests/fixtures/`, and for random projects from the existing store property generator: `from_workspace_files(to_workspace_files(p))` gives a project whose `save()` bytes equal `p.save()` bytes. Old fixtures stay untouched and still load.

5. **Deterministic and reviewable (P4).** The same project always gives the same paths and the same bytes. Files are pretty-printed with two spaces, one element per block, keys in a fixed order, elements in the snapshot's order, a final newline. A test changes one asset's name and checks that exactly one file differs, and by one line.

6. **Refusals, each with a typed error that names the path** (and line and column where the JSON reader gives one):
   - a path outside the layout, with `..`, absolute, with a backslash, or with upper-case or other characters
   - the same path twice
   - `aegis.json` missing
   - wrong `format`, a `version` above 1, an unknown `kind`, an unknown field
   - the same element ID in two files (both paths named)
   - a tree file whose name is not its tree's visual ID
   - a reference to an element that is in no file (the path of the referring file named)
   - the methodology key or version in `aegis.json` different from the one passed in

   A refusal never returns a partly built project.

7. **Caps (R3).** New public constants with a doc comment each: the largest number of files, the largest single file, and the largest total size, the last no greater than `MAX_PROJECT_FILE_BYTES`. All existing decode caps apply inside each file. Each cap has a test just under and just over. Error text never contains unbounded input.

8. **Fuzzing.** A fuzz target feeds arbitrary file sets to `from_workspace_files`. It must not panic, hang or allocate past the caps. Run it for the time the store fuzz targets are normally run and note the result in the ADR.

9. **Measured.** One benchmark writes and reads the `large` reference project as a file set. The ADR records the times and peak memory next to the snapshot's. Reading must stay inside the project-open budget of `docs/engineering-standards.md` section 2; if it does not, stop and report before optimising.

10. **Written down.** A new ADR records: the layout, why the side tables (descriptions, review marks) stay in their own files for version 1, how the loader reuses the snapshot reader, the caps and the measurements. `docs/architecture.md` gets a short entry for `store/workspace.rs`.

11. **Gate.** `scripts/ci.sh` passes: format, clippy with warnings denied, tests, `scripts/deny.sh`. No new dependency.

## Out of scope
- Reading or writing the disk, atomic writes, removing stale files (job 163 and 164).
- The `aegis` command-line tool (job 163).
- One folder per system; until job 170 the one system folder is named `main`.
- `item.md` and the structure block (job 193); the item elements go to `item.json` for now.
- Any change to the snapshot format, the op log, ops, or evaluation.
- Bundles, git, the server.

## Notes for the developer
- **The simplest correct loader:** check paths and envelopes, then put the sections back together and hand them to the existing snapshot decoder, so every validation and cap that exists today runs unchanged. Use another approach only if the measurement in criterion 9 forces it, and say so in the ADR.
- **The simplest correct writer:** reuse the section serializers in `store/snapshot.rs` (`Assets`, `Damages`, `Threats`, `Trees` and the rest). Do not write a second serializer for any element.
- The snapshot is written compact on one line; the workspace files are pretty-printed. That is the only difference in how a section looks.
- File names come from visual IDs, never from element names, so renaming an element never renames a file.
- Treat the input as hostile, as ADR 0009 does: a file set may come from a customer, a colleague or an attacker.
- Standards that apply most: P4 (deterministic), R3 (caps and streaming), Q3 to Q5 (tests, property tests, fuzzing), L6 and Q7 (no new dependency).
- If something in the layout turns out not to fit the model, stop and write the question down in the job folder; do not invent a new layout.
