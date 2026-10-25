# ADR 0033: Live observers with server-checked numbers

- Status: proposed (job 035-live-observers)
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
