# ADR 0046: The event log on every server, with a size limit

- Status: accepted (job 052-event-log-everywhere)
- Date: 2026-10-26
- Scope: `crates/aegis-server/src/accounts/{events,mod,memory,postgres}.rs`, `service.rs`, `main.rs`
- Rules: R3, R4, R6, C2 in `docs/engineering-standards.md`
- Builds on: `0043-audit-and-license-views.md`

## Context

ADR 0043 left a gap. The event log lived inside `Accounts`, so a server with
company login only (`AEGIS_AUTH=oidc`) had none and `ListEvents` answered
`no_event_log`. Those are the customer servers. Also, the log had no limit:
`aegis_event` grew for ever, and an unauthenticated caller makes one row per
failed login.

## Decisions

### 1. `EventLog` is its own type

`EventLog` wraps the account backend (memory or PostgreSQL) without the user
table. `Accounts` holds its backend in an `Arc` and hands out `event_log()`;
the service uses that log. For `oidc` and `dev` servers `main.rs` builds a
standalone log over the same database. The table and the migration do not
change. The `no_event_log` answer stays for a service built without a log
(tests, embedding).

Company logins happen at the identity provider, so the server writes no
`login_ok` for them. It does write exports, baselines, project deletions and
the other service events.

### 2. Two limits, enforced hourly

`trim(retention)` removes events older than `AEGIS_EVENT_RETENTION_DAYS`
(default 365, 1 to 3650), then the oldest beyond `AEGIS_EVENT_MAX_ROWS`
(default 1,000,000, 1000 to 100,000,000). The age pass deletes in batches of
5000 from the oldest row in primary key order and stops at the first batch
that holds a young row (events are written in time order), so a log with
nothing old costs one indexed read. The row pass deletes everything older than
the keep-th newest id. No new index is needed. The first trim runs at start.

### 3. Storage and encoding (decision for the questions raised)

- **Rows, not text files.** The log is read by an admin screen with a limit
  and a newest-first order; typed columns and the `(org_id, event_id DESC)`
  index serve that. Text log files would need parsing and a second store.
- **Plain `text` columns.** PostgreSQL compresses large values on its own
  (TOAST) and these values are at most 256 characters; a custom encoding
  would save little and make audits harder. `kind` stays a short text so an
  old app can show a kind from a newer server.
- **Bounded text.** Control characters are removed and each field is cut to
  256 characters before the write (R3), so a row has a known maximum size.
- **Retention is the main saving.** The cap bounds the table at about
  150 MB with the defaults.
- **Kept for later:** copying old events out to a compressed archive
  (newline-delimited JSON with zstd) before they are trimmed, for customers
  with a legal keeping period; time partitions (drop a month at a time) when
  the cap is raised far above a million; a hash chain over events like the
  project audit has. The project operation log is separate: it is the
  source of truth, is never trimmed, and is already stored as canonical JSON
  with a hash chain.

## Tests

Unit tests (order, cuts, age, row cap, underflow, the timer task), an
integration test through gRPC on a server without accounts (a record, a read,
a non-admin refused), a PostgreSQL test (12,000 old rows across batches, row
cap), and a stress test (16 writers plus a trimmer; 200,000 events at about
8,000 per second on a laptop-class machine, none lost, cap holds).

## Consequences

- A failed audit write is still only logged, as before.
- Trimming deletes audit history. A customer who must keep it longer raises
  the limits or exports first (see `docs/deployment.md`).
