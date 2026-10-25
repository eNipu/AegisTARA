# ADR 0037: Baselines, history and reports

- Status: accepted (job 041-tara-pages)
- Date: 2026-10-25
- Scope: `crates/aegis-server/src/{registry,service}.rs`, `store/`, `crates/aegis-client/src/{report,pages,settings}.rs`
- Rules: R3, D7, D10, P4 in `docs/engineering-standards.md`
- Builds on: `0020-project-diff.md`, `0027-server-persistence.md`

## Context

A TARA is reviewed against a frozen version, and auditors ask what changed
and who changed it. Users also need the work as files: a report, a risk
register, tree pictures.

## Decisions

### 1. A baseline is a stored snapshot

`CreateBaseline` writes the current snapshot with a name and the op sequence
number to `aegis_baseline`. It never changes. At most `max_baselines` per
project (R3). Deleting a project deletes its baselines in the same
transaction.

### 2. Comparison runs on the client

The client downloads a baseline (`GetBaseline`, chunked and size-capped like
`OpenProject`) and runs `Project::diff` locally. The server stays simple and
the comparison works the same offline.

### 3. History reads the op log

`ListHistory` returns the newest ops (who, when, what kind) from the stored
log. No second table: the op log is already the hash-chained record.

### 4. Exports are made on the client and logged on the server

The client writes the file and then calls `RecordExport`. The server's event
log keeps who exported what and when. File names are sanitized so a project
name cannot escape the export folder. CSV fields starting with `=`, `+`,
`-`, `@`, tab or CR are prefixed with `'` (CSV injection). HTML text is
escaped.

### 5. Seat ledger rebuilt from the op log

At start the server reads the distinct editors with ops in the seat window
and restores the ledger (job 039). A restart no longer frees seats.

## Consequences

- A large baseline costs one download per comparison. That is fine at the
  project size caps.
- The report format is ours. A customer template engine is a later item.
