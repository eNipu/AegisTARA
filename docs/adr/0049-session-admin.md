# ADR 0049: Administrators see and end sessions

- Status: accepted (job 055-session-admin)
- Date: 2026-10-26
- Scope: `aegis.proto` (`ListSessions`, `RevokeSession`), `crates/aegis-server/src/{accounts/mod,service}.rs`, `crates/aegis-client/src/{audit,connection,session,commands,view,state}.rs`, `crates/aegis-shell` (Administration, "Sessions" tab)
- Rules: R3, R6 in `docs/engineering-standards.md`; `direction-v1.md` section 6 ("Admin can see and revoke sessions")
- Builds on: `0035-password-login.md`, `0043-audit-and-license-views.md`

## Context

An administrator could disable a user (which ends their sessions) but could
not see who is logged in from where, or end one stolen or forgotten session
without locking the user out.

## Decisions

### 1. Two admin calls, password login only

`ListSessions` returns the open sessions, newest first, at most 1000 (R3).
`RevokeSession(id)` ends one. Both need an administrator and password
accounts; company login sessions belong to the identity provider, which has
its own session control. A non-admin gets `not_admin`.

### 2. The session ID is not the token

The ID is the first 8 bytes of the session's SHA-256 key as 16 lowercase hex
digits. The server stores only that hash (ADR 0035), so the list reveals
nothing that can be used as a token (tested). It is long enough that two open
sessions sharing it is a one in 10^10 event at 100,000 sessions; if it ever
happened, `RevokeSession` ends neither and answers `unknown_session` rather
than guess.

### 3. Ending is immediate and not a ban

The session leaves memory and storage, so the owner's next request is
unauthenticated and the app shows the login. The user can log in again.
A store failure leaves the session open and tells the admin. Ending an
already ended session answers `unknown_session` (not found). An admin may end
their own session; that is a log out.

### 4. One event

`session_revoked` goes to the event log with the actor, the user whose session
it was, and the device name, so "who ended Bob's laptop" is answerable.

### 5. The window

Administration gets a "Sessions" tab (a table: user, device, started, ends,
both in UTC), a Refresh button and "End this session" with a confirmation.
After any user administration action the list reloads, because disabling or
deleting a user ends sessions.

## Performance

`RevokeSession` scans the open sessions once, comparing raw bytes, with no
allocation: O(sessions). A first version formatted every key as text and was
13 times slower (ending 1,000 of 5,000 sessions took 7 s, now 0.5 s; 1,000 of
100,000 take 1.8 s in a release build). The scan holds the state lock only for
the scan, never across an `.await` (C2).

## Not done

- A session list for company login (the identity provider owns it).
- An idle timeout policy (direction section 6): sessions end only at their
  absolute expiry.
- Ending all sessions of a user in one click (disabling the user does that).

## Tests

Unit (order, expiry, malformed IDs, double revoke, audit), gRPC (admin list,
non-admin refused, end, the user's next call fails, the user logs in again),
client session (list, end, notice, other user logged out), PostgreSQL (an ended
session stays ended after a restart), window (tab, selection, screenshot),
stress (`AEGIS_STRESS_SESSIONS`: 100,000 sessions loaded, list capped, 1,000
concurrent ends each succeed once).
