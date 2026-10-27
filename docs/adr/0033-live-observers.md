# ADR 0033: Live observers with server-checked numbers

- Status: superseded by job 108-remove-live-view (decision D23); kept for the record of the accuracy check, which stays
- Date: 2026-10-25
- Scope: `aegis.proto` (`RisksEvent`), `crates/aegis-server` (actor, service), `crates/aegis-client` (session, view)
- Rules: A1, R4, D10 in `docs/engineering-standards.md`; `direction-v1.md` section 7
- Builds on: `0026-server-skeleton.md`, `0030-client-core.md`

## Context

Observers already follow the editor: `Watch` streams accepted ops and the
client applies them to its own engine (ADR 0030). Direction section 7 also
wants the server's recomputed results sent to every open client. Two reasons
make that more than decoration:

1. **Accuracy.** Two engines (client and server) compute the same numbers. If
   they ever disagree, a person is looking at a wrong risk. We want to know.
2. **Engine mismatch.** A client with another engine version must show server
   results only (direction section 3).

## Decisions

### 1. One `RisksEvent` per accepted batch

`WatchEvent` gains `risks`: `as_of_seq` and the full list of threat risk keys
(the same entries `GetRisks` returns). It is sent

- once after the backlog, so a new observer has the numbers for its position;
- once after every accepted batch, after that batch's ops.

The server computes it from its incremental cache (rule A1), only when at
least one observer is subscribed, and once per batch, not per op. Each event
is the full list, so no event depends on an earlier one: a client that
resynchronises needs no history.

### 2. Bounded (R4)

Projects with more threat scenarios than `risks_event_max_threats` (2,000 by
default) get ops only. Their observers compute locally, and `GetRisks` still
answers on demand. The event also goes through the same bounded broadcast
buffer as ops, so a slow observer lags and resynchronises as before, and the
editor never waits.

### 3. The client checks its own numbers

When a `RisksEvent` describes exactly the state the client shows (same
sequence number, nothing pending), the client compares its engine's numbers to
the server's.

- **Equal:** the view records `risks_verified_seq`.
- **Different:** the server's numbers are official. The client says so once,
  reloads the project once, and from then on shows the server's numbers for
  that project. A second difference after the reload does not reload again, so
  a deterministic engine bug cannot cause a reload loop.
- **Different engine version:** the client shows the server's numbers in the
  threat list without comparing.

Server numbers replace only the threat list's risk labels. The tree and the
inspector still come from the local engine, which is why a version mismatch
keeps editing off.

## Consequences

- Every accepted batch costs the server O(threats) when observed. The cache
  makes the evaluation itself incremental; the list copy is the O(threats).
- The shell shows the check as a status in the project header (job 047,
  `docs/adr/0041-numbers-check-mark.md`).
- The tree view in a mismatched client is still local. Server tree results
  would need a new RPC and are not planned.

## Update: job 108-remove-live-view (2026-10-27, decision D23, rule P7)

Nobody needs to watch edits as they happen, and the stream cost a lot of
machinery. Decisions 1 and 2 are gone; decision 3 (the client checks its own
numbers) stays, fed by an ordinary call instead of a stream.

### What replaces it

- `Watch`, `WatchRequest`, `WatchEvent`, `AcceptedOp`, `LockEvent`,
  `RisksEvent` and `Heartbeat` are removed from `aegis.proto`.
- `GetProjectStatus` returns the newest sequence number and the lock holder.
  The actor answers from memory; nothing is serialized.
- A read-only client asks every `version_check` (30 s) and, when the server is
  ahead, shows "A newer version is available" with a Reload button. It never
  changes its view by itself. An editor asks nothing: it is the only writer.
- Taking the lock on an old copy reloads it first (the lease carries the
  sequence number it was granted at), so nobody saves onto an old base.
- The editor learns of a lost lease from the answer to `RenewLock` (every third
  of the lease) or to `SubmitOps`. Both already rolled back unsaved edits and
  reloaded read-only.
- "Numbers checked" (decision 3, ADR 0041): after the project opens and after
  every confirmed batch the client calls `GetRisks` and compares. One call at
  a time; a batch confirmed while one is in flight asks again. If this engine
  differs from the server's, or disagrees once, the server's numbers are shown
  as before.

### What was deleted

| Part | Where |
|---|---|
| Observer broadcast channel, the recent-ops ring, `Accepted`, `Event`, `Subscription`, `observer_risks`, the `Lock` events | `aegis-server/src/actor.rs` (-134 / +26 lines with the new `Status`) |
| `Watch` handler, `forward`, `watch_event`, the heartbeat | `aegis-server/src/service.rs` (-211 / +13) |
| `ServerConfig::{observer_buffer, recent_ops, risks_event_max_threats, heartbeat}`, `ServerError::OutOfRange` | `config.rs`, `error.rs` |
| Watch task, restart and silence watchdog (`watch_silence`), `on_watch`, `on_lock_event`, `on_remote_op`, `Workspace::apply_remote`, `WorkspaceError::{PendingEdits, Gap}`, `ServerCode::OutOfRange` | `aegis-client` (`session.rs`, `workspace.rs`, `error.rs`) |
| Messages `Watch*`, `AcceptedOp`, `LockEvent`, `RisksEvent`, `Heartbeat` | `aegis.proto` (-41 / +12) |

Tests removed (six server, three client, one workspace): observers follow the
editor live; a new observer gets the current risks after the backlog; a
project over the threat limit sends ops but no risks; a late observer catches
up from the recent ops; an idle observer never slows the editor; watch
streams outlive the call timeout; a watch answer that lands late does not take
the lease away; a second user follows the editor live; a silent live stream is
restarted; remote ops apply in order. The late-watch-answer test guarded a
race that only existed because the lock state arrived on the stream, so it
has no replacement.

Tests added: the project status gives the sequence and the lock holder; a
second user reads a saved version and is offered the newer one (with reload,
the numbers check and the lock hand-over); taking the lock on an old copy
reloads it first; only a read-only view checks for a newer version, and not
too often; the shell shows the offer and reloads.

### Consequences

- A read-only user sees changes up to 30 s late, and only after Reload.
- A lost lease is noticed within a third of the lease, not at once.
- The server no longer keeps a stream open per viewer, so a project with many
  viewers costs one cheap call each per 30 s.
