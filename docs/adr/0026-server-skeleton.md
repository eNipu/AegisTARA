# ADR 0026: Server skeleton: gRPC, project actors, edit lease

- Status: proposed (job 029-server-skeleton)
- Date: 2026-10-25
- Scope: `crates/aegis-server`, `OpCodec` and `ENGINE_VERSION` in `aegis-model`
- Rules: C1, C2, C4, R3, R4, R5, R6, Q3, Q4, P4, D10 in `docs/engineering-standards.md`; decisions D3, D4 in `docs/decisions.md`
- Builds on: `0009-save-load-format.md` (snapshot and op log formats)

## Context

The server is the source of truth (direction section 3). It must accept typed
ops from one editor at a time (D4), run the same engine as the client, keep
history in an append-only log, and show every accepted op to read-only
observers. This job builds that core without a database (job 030), login
(031), or the live-view polish of job 035.

## Decisions

### 1. One actor per open project (C1)

`actor.rs` owns the `Project`, its `ProjectEvalCache`, the lease, and the
observer channel. Callers send commands through a bounded mailbox (64) and
wait at most 5 s for room (R4, R6). No lock protects a project; ops apply in
mailbox order, so the audit order is the apply order.

| Option | Why not |
|---|---|
| `Arc<Mutex<Project>>` | A lock across the store `await` breaks C2; ordering depends on lock fairness. |
| Per-request load and apply | O(project) per op. The cache (rule A1) needs a long-lived project. |

### 2. Ops cross the wire in the op log's own format

`OpCodec` (new, `aegis-model`) encodes and decodes one op as the `op` member
of a log record. One strict, capped decoder (R3) guards the file reader and
the network. The server stores the **canonical re-encoding**, never the
client's bytes (P4: same op, same bytes).

### 3. Atomic batches, memory never ahead of storage

`SubmitOps` decodes every op first, applies them with `apply_cached`, stores
the batch, and only then answers and notifies. A rejected op undoes the
earlier ops of the batch with their inverses. A refused append undoes the
whole batch. If an inverse itself is refused, the actor marks itself broken
and unloads; the next request reloads from storage. Tests compare the
snapshot bytes before and after a failed batch.

`base_seq` must equal the project's sequence number. Only the lease holder
writes, so a mismatch means the lease changed hands or the client is stale.
It also makes a retry after an unloaded actor safe.

### 4. Lease: pure state machine, injected time

`LeaseState` (`lease.rs`) is plain data and takes `tokio::time::Instant` as
an argument. 2-minute lease, renewed by the client. The same subject may
re-acquire its own live lease (a restarted client), which invalidates the
old token. Tokens are 122 random bits from the OS, compared in constant
time. An expired lease cannot be renewed. Admins break leases. The lease
is not persisted: it exists only while the project is loaded, and a loaded
project with a live lease never unloads.

### 5. Observers: ring buffer plus bounded broadcast

The actor keeps the last 1,024 accepted ops (`recent_ops`) and a broadcast
channel (1,024). `Watch(after_seq)` takes the backlog from the ring and a
receiver in one actor step, so nothing is missed or repeated. An observer
that falls behind the broadcast buffer, or asks for ops older than the ring,
gets `OUT_OF_RANGE` and must reopen the project. The editor never waits for
an observer. The ring costs at most 1,024 x 13 KB in the worst case and a
few hundred KB for realistic ops.

### 6. Whole-project work leaves the async worker (R5)

Serializing a project moves it into `spawn_blocking` and back, so no async
worker blocks. The mailbox queues meanwhile. Applying an op stays on the actor
task: O(op) plus an incremental cache update.

### 7. Snapshots are a cache of the log (D10)

A snapshot is written every 1,000 ops, when an idle project unloads, and on
shutdown. Loading replays the ops after the snapshot in `spawn_blocking`.
A gap or an undecodable op is `StoreFault::Corrupt`, never silently skipped.

### 8. Authentication is outside the service

The service reads a `Principal` from the request extensions. A tower layer
or interceptor puts it there. This job ships only a development
interceptor (a header, refused by the binary unless `AEGIS_DEV_AUTH=1`);
job 031 replaces it with OIDC and DPoP validation as a layer, which can see
the HTTP method and URL that DPoP binds to.

### 9. Limits (all in `ServerConfig`)

Request size 17 MiB (a full batch), 256 ops per batch, op size 64 KiB (the
model's cap), 30 s call timeout for unary calls (not stream bodies, tested),
64 concurrent requests per connection, 256 loaded projects, 15 min idle
unload, HTTP/2 keep-alive pings so dead observers are dropped.

## Not in this job

PostgreSQL and the audit hash chain (030), OIDC (031), recomputed-result
streaming (035), TLS termination (load balancer), Cedar policies (later;
the only role today is admin for breaking leases), per-module locks.

## Consequences

- Engine parity: client and server both call `Project::apply`. `GetServerInfo`
  returns `ENGINE_VERSION` so a client can refuse mismatched servers.
- A restart loses nothing that was acknowledged (tested with a restarted
  server over the same store).
- The memory store is the executable specification of the `ProjectStore`
  contract that the PostgreSQL backend must meet.
