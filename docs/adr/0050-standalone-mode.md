# ADR 0050: Standalone mode and project-folder import

- Status: accepted (job 107-standalone-mode)
- Date: 2026-10-27
- Scope: `crates/aegis-server/src/{store/folder,import,main}.rs`, `crates/aegis-shell/src/{standalone,launcher,main,controller}.rs`, `crates/aegis-shell/ui/app.slint`, `crates/aegis-client/src/settings.rs`
- Rules: R3, R4, R5, R6, C2, D10 in `docs/engineering-standards.md`; decision D22
- Requirements: SHP-01, SHP-02, SHP-03
- Builds on: `0009-save-load-format.md`, `0027-server-persistence.md`, `0032-license-tokens.md`

## Context

The owner's consultancy works alone, under NDA, on the consultant's machine,
and hands the project to the client with the report (D22). That needs the app
to run without a server, keep a project in a folder, and let the client's
administrator import that folder into their own server without the history
changing.

## Decisions

### 1. The app embeds the real server

`Standalone` starts `AegisService` over a `Registry<FolderStore>` inside the
app process and the client connects through its normal gRPC path, so every
feature works in both modes and there is no second code path to keep equal.
The listener is `127.0.0.1:0` only. The signed license is read and verified
before anything is bound, so a bad license or a damaged folder never opens a
port. The embedded server uses the development authenticator: there is no
login, and a process of the same machine that finds the port can act as the
user. That is the stated cost of "no login" on a single-user desktop; nothing
is reachable from the network. `max_projects` is 1.

### 2. `FolderStore` writes the formats that already exist

`ops.jsonl` is an `aegis-oplog` v1 stream, byte for byte (a test reads it with
`OpLogReader`), and snapshots are `Project::save` bytes (a test loads them
with `Project::load`). What the op-log format cannot carry (actor, acceptance
time, audit `prev` and `hash`) goes in a binary sidecar, `ops.meta`, in the
same order. A `manifest.json`, replaced atomically, is the commit record: it
holds the project facts, `last_seq`, the chain head, the snapshot's name and
SHA-256, and the baselines. Alternatives rejected: SQLite (a new dependency
and a second format for no gain over files the model already reads and
writes) and extending the op-log format (it is a compatibility promise, ADR
0009).

### 3. Crash safety is the manifest rename

Log, then sidecar, are written and synced; the manifest rename then makes the
ops visible. Bytes past `last_seq` are debris. Opening a folder reads the
committed history once, reports damage as `Corrupt` (never repairs data it
cannot prove uncommitted), trims both files to the committed lengths, and
removes unnamed snapshot files and manifest temporaries. The store remembers
the committed lengths, so an append never re-reads the log and a failed append
is undone before the next one. One OS file lock per folder gives one writer,
also across processes. Blocking I/O runs in `spawn_blocking` and no lock is
held across an await (C2, R5).

### 4. Import copies; it never replays

`Registry::import_folder` keeps the project ID, every op's bytes, actor and
time, the folder's snapshot and the baselines with their IDs. The audit chain
is a function of project ID, sequence, time, actor and op bytes, so
re-appending the same ops gives the same hashes; the import checks the head
and count against the folder's. Replaying the ops as edits would stamp them
with the importing admin and a new time and break the chain. The folder is
verified first (chain, methodology key and exact version), the copy streams in
batches of 256 ops or 4 MiB, and the result is compared with the folder and
rebuilt through `Loaded::rebuild`. A failure after the project exists deletes
it. An ID the server already holds is refused without touching it. The
command is `aegis-server import-folder <folder>`; it needs the same license
and database variables as the server and creates nothing for a wrong path.

An exact methodology version match is deliberately strict: the project's
meaning depends on the pack. Loosen it when a real upgrade path exists.

## Consequences

- Imported actors keep their names. If their edits are inside the license's
  seat window they take a seat after the next server start.
- Reads that walk the history are linear scans (O(history) time, O(result)
  space). A byte-offset index would make `recent_ops` O(limit); add it when a
  standalone log grows large enough to notice.
- Opening a folder with the same path twice needs the first embedded server
  to stop first; the launcher stops the old one before it starts the next.
- The shell now links `aegis-server` and `aegis-license` in release builds.
