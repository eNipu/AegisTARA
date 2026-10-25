# ADR 0027: Server persistence: PostgreSQL, snapshot cache, audit hash chain

- Status: proposed (job 030-server-persistence)
- Date: 2026-10-25
- Scope: `crates/aegis-server/src/store/`, `crates/aegis-server/migrations/`
- Rules: R3, R4, R6, D10, P4, Q4 in `docs/engineering-standards.md`; D3 in `docs/decisions.md`
- Builds on: `0026-server-skeleton.md` (the `ProjectStore` contract), `0009-save-load-format.md`

## Context

Job 029 defined `ProjectStore` and an in-memory backend. The product needs a
durable backend on the database the customer already runs (direction
section 2: PostgreSQL), and an audit trail for every accepted op (roadmap
phase 1; direction section 6: hash-chained audit log).

## Options

| Question | Options | Chosen |
|---|---|---|
| Driver | `sqlx` (compile-time checked queries, large); **`tokio-postgres` + `deadpool-postgres`** (small, no macros, MIT/Apache) | tokio-postgres |
| What is the source of truth | project rows only; **append-only op rows plus a snapshot cache** | ops are truth (D10) |
| Op insert | one `INSERT` per op; **one `INSERT ... UNNEST` per batch** | UNNEST |
| Tamper evidence | none; **per-project SHA-256 chain**; signed ledger | chain, head exportable |
| Migrations | external tool; **embedded SQL, versioned table, advisory lock** | embedded |

## Decision

### Schema (`migrations/001_init.sql`)

- `aegis_project`: facts, `last_seq`, `last_hash` (chain head), `snapshot_seq`,
  `snapshot` (bytea). CHECK `snapshot_seq <= last_seq`. `org_id` is a column
  from day one (v1 serves one org; hosted later needs no schema change) and
  every query filters on it.
- `aegis_project_op`: `(project_id, seq)` primary key, canonical op bytes,
  actor, time, `prev_hash`, `hash`.
- Numbers are `bigint`; conversions to `u64` are checked in both directions.

### Append: one transaction, one round trip

`FOR UPDATE` on the project row serializes writers. The batch must continue
`last_seq` exactly. All ops go in with one `UNNEST` statement, then
`last_seq` and `last_hash` move, then commit. Test: eight racing writers
append the same sequence numbers; exactly one wins; the chain verifies.

### Load: consistent pair

Snapshot and the ops after it are read in one `REPEATABLE READ` read-only
transaction. A snapshot is a cache: saving it needs `snapshot_seq <= seq <=
last_seq`, so it can never run ahead of the history or go back.

### Audit chain (`store/audit.rs`)

`hash = SHA-256("aegis-audit-link-v1" || prev || seq || time || len+actor ||
len+op)`, genesis `SHA-256("aegis-audit-genesis-v1" || project_id)`.
Fixed-width numbers and length prefixes make the encoding unambiguous
(tested: `("ab","c") != ("a","bc")`). `verify_chain` streams the rows through
`ChainVerifier` (constant memory) and also compares the final hash with
`last_hash`, which catches a deleted tail. It detects edits, deletions,
reordering, and another project's rows. It does **not** stop someone who can
rewrite the whole table and recompute every hash; the report exposes the head
hash so an operator can store it somewhere the DBA cannot write. The admin
RPC `VerifyAudit` runs the check.

### Operations

- Pool of 16 (`AEGIS_DB_POOL`), 5 s acquire and connect timeouts, server side
  `statement_timeout` 30 s (R6).
- Migrations run at startup under `pg_advisory_xact_lock`, so servers
  starting together cannot race. A database migrated by a newer build is
  refused.
- `AEGIS_DATABASE_URL` selects PostgreSQL; unset keeps the in-memory store
  and the binary logs a warning.

## Verification

`tests/postgres.rs` (needs `AEGIS_TEST_DATABASE_URL`, skipped otherwise;
CI provides a PostgreSQL 16 service): store contract, gap and conflict
rules, snapshot rules, racing writers, 2,000-op batch, tamper detection by
SQL edits (changed actor, deleted tail), idempotent and refused migrations,
org isolation, and the full server restart scenario over PostgreSQL. The
same restart and audit scenarios also run over the memory store.

## Not in this job

- TLS to the database. Managed PostgreSQL usually requires it. The driver
  supports it through `rustls`, but that adds crypto crates whose licenses
  need a D5 review first. Until then use a private network or a local
  TLS proxy. Tracked in the backlog.
- Non-op audit events (logins, exports, permission changes): they arrive
  with the features that cause them.
- Pruning ops before the snapshot. Ops stay forever, by design, for audit.
