# ADR 0030: Client core: optimistic editing, one actor, view model

- Status: proposed (job 033-client-core-and-slint-shell, part 1)
- Date: 2026-10-25
- Scope: `crates/aegis-client`, lease sequence numbers in `aegis-proto` and `aegis-server`
- Rules: A1, C1, C2, R4, R5, R6, D10, P4 in `docs/engineering-standards.md`; `direction-v1.md` sections 3 and 7
- Builds on: `0026-server-skeleton.md`, `0028-login.md`, `0029-canvas-engine.md`

## Context

Direction section 3 says the client applies an edit to its embedded engine at
once, sends it to the server, and rolls back if the server refuses. Section 7
says one user edits at a time under a lease while others watch live. The UI
toolkit (Slint, D7) must stay replaceable. The core therefore owns all
behaviour and has no UI dependency.

## Decisions

### 1. `Workspace`: the edit protocol as pure state

`Workspace` holds the project, the incremental `ProjectEvalCache`, the
pending edits, and the undo and redo stacks. It does no I/O, so the protocol
is tested by a model-based property test: random interleavings of edit, undo,
redo, send, accept and reject against a "server" that replays the accepted
ops. Invariants: the confirmed sequence always equals the server's, and with
nothing pending the two projects are equal.

- An edit is applied locally at once and queued with its inverse ops.
- **One submission in flight.** All queued edits go out as one `SubmitOps`
  call, so the base sequence number is always the confirmed one and cannot be
  raced. Edits made meanwhile wait for the next call.
- **Rejection rolls back every pending edit, newest first**, because later
  edits may depend on the rejected one, and clears undo history, which can no
  longer be trusted. A rollback whose inverse is refused rebuilds the cache.
- **Undo is an edit.** The reverse of the last edit is applied and submitted
  like any other, and its own inverse becomes redo. The server's log never
  rewrites history. Bounds: 1,024 pending edits, 1,000 undo steps (R4).
- A remote op (observer mode) applies only in sequence, only with nothing
  pending, and clears undo history.

### 2. One session actor; the network never blocks it

`Session` is a handle to one task (C1) that owns the workspace, lease and
selection. Every network call runs in its own task and returns as an event, so
the actor applies the next edit while a save is in flight. Test: with a 300 ms
slow link, the sample TARA is on screen within 200 ms and confirmed after
300 ms. Events carry an epoch; opening, closing or reloading starts a new one,
so a late answer cannot touch the wrong project. All channels are bounded.

### 3. Lease handling

- Acquire returns the server's sequence number (new field `Lease.last_seq`).
  If the client's copy is older, it reloads first, then enters edit mode, so
  it never edits stale data.
- Renew at a third of the lease length. A refused renewal, or none for a
  whole lease length, loses the lease.
- **Losing the lease** rolls back unsaved edits, tells the user how many were
  discarded, goes read-only and reloads. Tested by an admin breaking the lock
  while a change is in flight.
- While editing, watch events that are ops are ignored: only this client can
  write, and its own ops can reach the stream before the save's answer.
- A transient network error keeps the edits and retries the same call. If the
  first attempt did arrive, the retry is refused as stale and the client
  reloads, which already contains the change.

### 4. The view model is plain data

`ViewModel` (connection, project list, open project, notices) is rebuilt from
the workspace after every change and published through a `watch` channel as
an immutable snapshot. It carries labels and scene data and no model types
the UI could misuse: UIs send back list positions, and the session maps them
to typed values (`Edit::to_ops`). Threat rows read the incremental cache
(O(threats)). The tree of the selected threat is laid out by the canvas
engine and annotated from the evaluation: score text `12 · High` (attack
potential points and rating), color by rating, the deciding path highlighted.

### 5. Engine parity

`GetServerInfo.engine_version` is compared with `aegis_model::ENGINE_VERSION`.
On a mismatch the client warns, shows its own numbers and **turns editing
off**. (Showing the server's `GetRisks` numbers instead is future work.)

### 6. Dev login

`DevLogin` sends the development headers. It exists for tests and for a
development server (`AEGIS_DEV_AUTH=1`); a production build uses the OIDC
`Session` from `aegis-auth` through the same `CredentialProvider` trait.

## Verification

12 unit and property tests for the workspace; 5 view model tests over the
sample TARA; 8 end-to-end session tests against the real server (live editor
and observer, lock refusal, lock loss with discard and reload, undo and redo
saved as ordinary edits, slow link, unreachable server, release on close).

## Not in this job

Offline cache and offline editing (direction section 3: cache only if policy
allows); merging or co-editing; what-if branches; server-side risk display on
engine mismatch; editing structure (adding steps, controls) beyond the
rating, factor and rename edits; a break-lock command (admin tool).
