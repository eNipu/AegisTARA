# ADR 0043: Event log and license views for administrators

- Status: accepted (job 049-admin-events-and-license)
- Date: 2026-10-26
- Scope: `aegis.proto` (`ListEvents`), `crates/aegis-server/src/{service,accounts/service}.rs`, `crates/aegis-client/src/{audit,state,session,connection}.rs`, `crates/aegis-shell/{ui/app.slint,src/controller.rs}`
- Rules: R3, R6, C2 in `docs/engineering-standards.md`; `direction-v1.md` section 6
- Builds on: `0035-password-login.md`, `0032-license-tokens.md`

## Context

The server already wrote an event log (logins, exports, admin actions) and
knew the license, but an administrator could see neither in the app.

## Decisions

### 1. One read call, admin only, bounded

`ListEvents(limit)` returns the newest events first. `0` means 200, the
server caps it at 1000 (R3), and the store limit is the same cap. A user who
is not an admin gets `not_admin`. The call reads the event store only; it
cannot write. Events never hold a password or a token (tested).

### 2. No event store, no pretending

Events are stored by the accounts module. A server with company login only
(`AEGIS_AUTH=oidc`) or development login has none, and answers
`FAILED_PRECONDITION` with `no_event_log`. The app says "This server keeps no
event log." instead of showing an empty table. This is a real gap for
customers who run company login only: their log lines go to the server log
only. `AEGIS_AUTH=both` keeps a store. A later job should give company-login
servers an event store as well (open item in the status page).

### 3. Words, not codes

The client turns each kind into a sentence and a group (logins, exports,
admin actions) in `audit.rs`, with a test that every kind the server writes
has words. A kind from a newer server is shown as it is and grouped under
`Other`, so an old app never hides an event. The filter is applied in the
app on the rows it loaded (200), so it costs no call.

### 4. The license page needs no new call

`GetServerInfo` already carries the license. `LicenseView` now also keeps the
dates (in UTC) and the sorted features, and `lines()` words them, including
the grace and expired states. A server without a license (development) says
that no license is checked.

### 5. Two tabs on the Administration page

"Users and login" (the existing content) and "Event log". The license sits
in the side panel next to the project tools. The tab is window state only.

## Consequences

- The event log shows the newest 200 events. Paging back further is not built;
  an auditor with more needs an export (a later job).
- The page is taller than 760 px with the users tab open; the UI review
  should look at scrolling.
