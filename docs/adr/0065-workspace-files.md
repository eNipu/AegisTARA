# ADR 0065: Workspace files

- Status: proposed (job 162-workspace-files, slices 162a to 162d)
- Date: 2026-11-10
- Crate: `crates/aegis-model` (`src/store/workspace.rs` for the writer and
  reader, the caps in `src/store/mod.rs`, `WorkspaceProblem` and the
  workspace variants of `StoreError` in `src/error.rs`); fuzz target
  `crates/aegis-model/fuzz/fuzz_targets/workspace.rs`; benchmark
  `crates/aegis-model/benches/workspace.rs`
- Rules: P4 (deterministic output), R3 (decode caps) in
  `docs/engineering-standards.md`
- Decision: D32 in `docs/decisions.md` (a workspace is a folder of JSON
  files; git is the history in standalone mode)
- Builds on: `0009-save-load-format.md` (strict, capped JSON decoding, the
  snapshot) and `0011-methodology-pack-files.md` (the same reader for a
  second kind of file)

## Context

D32 saves a project as a folder of small JSON files. One snapshot cannot be
reviewed in git, two people cannot change it without a conflict, and a
system cannot have a folder of its own. The snapshot stays the format of
server mode and stays readable.

This ADR records the pure part: a `Project` becomes a list of
`(path, bytes)` and back. Neither function touches the disk, the clock or
the environment. Writing and reading the folder is job 163 and 164.

## Decision

### Layout

Paths use `/`, lower-case ASCII letters, digits, `-` and `.`. `aegis.json`
is always written. Every other file is written only when it has content,
as the snapshot omits empty sections today.

| Path | `kind` | Snapshot keys it holds |
|---|---|---|
| `aegis.json` | `workspace` | `methodology`, `languages` |
| `descriptions.json` | `descriptions` | `descriptions` |
| `review-marks.json` | `review-marks` | `review_marks` |
| `shared/damage.json` | `damage` | `damage_scenarios` |
| `shared/controls.json` | `controls` | `security_controls` |
| `shared/goals.json` | `goals` | `cybersecurity_goals` |
| `shared/claims.json` | `claims` | `cybersecurity_claims` |
| `systems/main/item.json` | `item` | `components`, `interfaces`, `data_flows`, `trust_boundaries` |
| `systems/main/assets.json` | `assets` | `assets` |
| `systems/main/threats.json` | `threats` | `threat_scenarios` |
| `systems/main/trees/<visual-id>.json` | `tree` | `attack_trees`, one tree per file |

The tree file name is the tree's visual ID in lower case. The system
folder is `main` until job 170 adds one folder per system.

Each file is a JSON object that starts with `format` (`aegis-workspace`),
`version` (1) and `kind`. The rest of the object holds the same values the
snapshot writes under the same keys. Files are pretty-printed, keys and
elements come out in a fixed order, and the file ends with one newline.

Every top-level snapshot key lands in exactly one file. A test builds a
project with one instance of every element (`common::record_every_op`) and
fails if a key has no file.

### Why descriptions and review marks keep their own files in version 1

Both are side tables that point at elements of other kinds, so neither has
a natural owner file:

- Descriptions are one map on `Project` that covers eleven element kinds
  and the project notes.
- Review marks are keyed by (tree, node). They could sit in the tree files,
  but they then would be split across files by tree.

One file per table keeps the rule that every snapshot key has one home.
Moving each entry next to the element it describes is a later decision. This
reason was written from the code's structure: the spec asks for the
separate files and does not give a reason.

### How the reader reuses `Project::load`

`from_workspace_files` checks the file set first and parses second:

1. Pass 1, on bytes only: path rules, the file and size caps, duplicate
   paths, the layout (a path must map to a known `kind`). Nothing is parsed
   before the caps pass.
2. Pass 2: each file's envelope (`format`, `version`, `kind`), its unknown
   fields, and its held entries. Tree files are joined in visual-number
   order, and a missing visual ID is filled from the file name.
3. The entries are joined into one snapshot object, and `Project::load`
   reads it. All snapshot checks and decode caps (ADR 0009) apply to the
   joined bytes, unchanged.

Errors from `Project::load` are returned as `StoreError::InWorkspace`,
naming the file when it can be told from the error. Otherwise they name
`aegis.json`. A refusal never returns a partly built project. The
methodology key must match the one passed in; the version is not checked,
because `Project::load` accepts any methodology version (ADR 0009, open
question 1).

### Limits

| Limit | Value | Reason |
|---|---|---|
| `MAX_WORKSPACE_FILES` | 10,010 | Ten fixed files plus up to 10,000 attack trees. The model has no tree cap of its own; a project with 2,000 threats has about 2,000 trees. The 10,000 figure is a choice, not an existing cap. |
| `MAX_WORKSPACE_FILE_BYTES` | 16 MiB | Well above the largest typical file. The doc comment in `src/store/mod.rs` says the largest typical file (the threats file for 2,000 threats) is about 1 MB. |
| Sum of all files | `MAX_PROJECT_FILE_BYTES`, 64 MiB | The same total as a snapshot. Checked before parsing. |
| Path length | 256 bytes (`MAX_PATH_IN_ERROR`) | Longer paths are refused before any lookup, so error text stays bounded. |
| Error text | at most 512 bytes | Paths are cut to 256 bytes and escaped, so a control-character path cannot grow an error. A test covers a 1 MB path, field and tree name. |
| Unknown field name in errors | `MAX_KEY_BYTES` | The existing key cap. |
| Everything inside a file | the existing decode caps (ADR 0009) | Applied through `Project::load` on the joined snapshot. |

Each cap has tests just under and just over its value (`tests/workspace_limits.rs`).

## Measurements

Command: `cargo bench --locked -p aegis-model --bench workspace` and
`cargo bench --locked -p aegis-model --bench store`. Both run on the same
machine in the same session. Workload: `benches/store_kit` `typical` (300
assets, 2,000 damage scenarios, 2,000 threats). Machine: Apple M2 Max, 32 GiB,
macOS 27.2. This is not the reference machine in `docs/engineering-standards.md`
(a 2022 4-core laptop), so the numbers show ratios more than absolute budget
fit. Each figure is the median of 100 samples; fastest and slowest in brackets.

| Operation | Median time | Throughput (median) | Bytes |
|---|---|---|---|
| `to_workspace_files` (write) | 4.02 ms (3.72 to 4.41) | 397 MB/s | 1,596,534 in 4 files |
| `save` (snapshot write, `store` bench) | 1.18 ms (1.09 to 1.33) | 933 MB/s | 1,097,749 |
| `from_workspace_files` (read) | 10.69 ms (10.21 to 11.42) | 149 MB/s | 1,596,534 in 4 files |
| `load` (snapshot read, `store` bench) | 5.75 ms (5.43 to 6.10) | 191 MB/s | 1,097,749 |

- Write is about 3.4 times the snapshot save. Read is about 1.9 times the
  snapshot load: the reader rebuilds the joined snapshot and then runs the
  same `Project::load`.
- The workspace is about 45 % larger than the snapshot, because of the
  pretty-printing and the repeated envelope.
- Budget: `docs/engineering-standards.md` section 2 gives "open project from
  local cache <= 1.0 s" for the `large` project. This record measures the
  `typical` project (4,300 entities), which reads in 10.69 ms. The `large`
  project and peak memory were not measured here; see "Left for later".

## Fuzz result

Not run. `cargo fuzz` is not installed on this machine (`cargo fuzz --version`
reports "no such command"), and this slice does not install it. The
target builds on stable: `cargo check --locked --manifest-path
crates/aegis-model/fuzz/Cargo.toml --bin workspace` passes.

The target is `fuzz/fuzz_targets/workspace.rs`. Its input is split on `0x00`
into chunks; the text up to the first `\n` in each chunk is the path, and the
rest is the file. The property: `from_workspace_files` returns an error, or
the project's workspace files read back to an equal project with equal
`save()` bytes. Two seeds are in `fuzz/seeds/workspace/`
(`fixture-sorted` and `fixture-reversed`, the four files of
`tests/fixtures/project-v1.json` in two orders).

To run it when `cargo-fuzz` is installed:
`cargo +nightly fuzz run workspace corpus/workspace seeds/workspace -- -max_total_time=600`
from `crates/aegis-model/fuzz`. The `run.sh` script in that folder does not
list `workspace` yet.

## Left for later

- Reading and writing the folder, atomic writes, and removing stale files
  (jobs 163 and 164).
- One folder per system (job 170).
- `item.md` and the structure block replace `item.json` (job 193).
- The `large` reference project and peak memory, as the spec asks, were not
  measured in this slice.

## Consequences

- A project's folder diffs line by line in git, at the cost of about 45 %
  more bytes and about 3.4 times the write time of the snapshot.
- The reader adds no new decoder: every value is still checked by the
  snapshot reader, so the strict, capped rules of ADR 0009 apply unchanged.
