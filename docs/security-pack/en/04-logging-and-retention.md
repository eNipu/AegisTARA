# 4. Logging and retention

## 4.1 Summary

The server keeps two records of what happens. The event log holds security
and administration events: logins, exports, user changes, project deletion.
The operation history of each project holds every change to the project,
with who made it and when. Both are chained with SHA-256, so a change made
behind the server's back can be found. The event log is trimmed by age and by
size; the operation history is never trimmed. The server also writes an
ordinary process log to standard output, which the customer's platform
collects. The server sends no log anywhere by itself. The event log never
stores the password field or a session token. A failed login records the typed
user name as typed (4.3).

## 4.2 The logs

| ID | Log | What it records | Where it is stored | Who can read it | How long it is kept |
|---|---|---|---|---|---|
| LG-01 | Event log | Security and administration events (4.3), each with time, actor, kind, target and detail | PostgreSQL table `aegis_event`. Without a database (memory only), in server memory | Server administrators in the app. `Customer` reads the table with its own database tools | 365 days and 1,000,000 events by default (4.5) |
| LG-02 | Project operation history | Every change to a project: its sequence number, time, the actor, the change, and the chain hashes | PostgreSQL table `aegis_project_op`. In standalone mode, files in the project folder | Users who may read the project see who changed what. `Customer` reads the table with its own tools | As long as the project. Never trimmed |
| LG-03 | Server process log | Start and stop, license and login mode, database connection, migrations, errors and warnings, refused requests (text of the error, no token), project IDs in error lines | Standard output of the server. `Customer` collects it with its platform | `Customer` | `Customer` sets it |
| LG-04 | Desktop app | Warnings and errors about the window, the keychain, the saved settings and sync. The app writes no log file | Standard error of the app process, which is not kept anywhere by the app | The person who starts the app from a terminal | Not kept |

What the process log holds, as the code stands:

- No password and no token is written to it.
- No network address is written to it.
- A user name appears in two cases only: the license customer name at start
  (the company that owns the license), and, when a server is built without an
  event log (tests and embedding only), each event is written to the process
  log in place of the event table.
- A project appears by its ID in error lines. The command that imports a
  standalone project folder also writes the project name.
- The `RUST_LOG` variable sets the level; the default is `info`.

## 4.3 Event log contents

Each event has five fields: the time, the actor, the kind, the target and a
short detail. The actor is the user name or, for company login, the subject
from the identity provider. For a failed login the actor is the user name that
was typed. The target is a user name, a project ID, or empty. The event log
never stores the password field or a token. For a failed login, the text typed
as the user name is recorded as typed. A password typed into the name box by
mistake would be kept until the retention removes it. Text fields have control characters removed and
are cut to 256 characters.

| Kind as stored | Meaning |
|---|---|
| `login_ok` | A password login succeeded. The detail is the name of the app. Company logins write no such event: the login happens at the identity provider |
| `login_failed` | A password login failed: wrong name or password, disabled user, too many tries, an invalid name, or password login switched off. The detail says which, when known |
| `logout` | A user logged out |
| `password_changed` | A user changed their own password |
| `user_created` | An administrator created a user. The detail is `admin` for an administrator |
| `user_updated` | An administrator changed a user. The detail lists what changed (display name, administrator flag, disabled flag, password) |
| `user_deleted` | An administrator deleted a user |
| `export` | A user exported a report or a table. The target is the project ID, the detail the kind of export |
| `project_deleted` | A project was deleted. The target is the project ID |
| `baseline_created` | A baseline (frozen version) was created. The target is the project ID, the detail the baseline ID |
| `password_login_changed` | An administrator switched password login on or off |
| `session_revoked` | An administrator ended a session of a user |
| `events_trimmed` | The retention removed old events (4.4, 4.5) |
| `membership_changed` | A project membership was added or its role changed. The detail is the member and the role |
| `membership_removed` | A project membership was removed |

## 4.4 Integrity

- The event log and the operation history of each project are SHA-256 hash
  chains. Each entry carries the hash of the one before it, so a changed,
  removed or inserted entry breaks the chain from that point.
- A server administrator can verify the chains from the app. The check reports
  whether the chain is intact, the first broken position, and the head hash.
  The release checklist requires the check to pass on a database used by the
  previous release.
- When the retention removes old events, the server writes an
  `events_trimmed` event with the number removed. A gap in the log is
  therefore visible, and the chain stays whole from the oldest kept event.
- What the chain proves: someone changed the database behind the server's back
  and did not recompute the chain.
- What it does not prove: a database administrator who can rewrite a whole
  table, recompute every hash, and replace the stored chain head defeats the
  check. The head is stored in the same database. To close this, `Customer`
  copies the head hash shown by the check to a system the database
  administrator cannot write.

## 4.5 Retention

| Data | Rule |
|---|---|
| Event log | Events older than `AEGIS_EVENT_RETENTION_DAYS` are removed (default 365, allowed 1 to 3650). The oldest events beyond `AEGIS_EVENT_MAX_ROWS` are removed (default 1,000,000, allowed 1000 to 100,000,000). Both run at server start and then every hour. At the defaults the table is about 150 MB at most |
| Project operation history | Never trimmed. It lives as long as the project |
| Sessions | Removed when they expire, and at logout, disabling, password change and user deletion (chapter 3) |
| Deleted project | Its operations, baselines and figures are deleted in one transaction |

- A longer legal keeping period: raise the retention, or copy the table out
  before it is trimmed (`Customer`).
- An archive export of trimmed events to a file is not built. A decision record
  lists it as kept for later; no requirement names it, so it is not `Planned`.

## 4.6 Time

Timestamps are the server's own clock, in milliseconds since the Unix epoch
(UTC). A wrong server clock gives wrong times in both logs. Keeping the clock
right is `Customer`.

## 4.7 Sending logs to a SIEM

- The process log goes to standard output, for the customer's collector.
- The event log is a PostgreSQL table. `Customer` can read it with its own
  tools, for example a scheduled query into the SIEM.
- A built-in forwarder (syslog or webhook) is not built, and no requirement
  asks for one.

## 4.8 Sources

- `docs/deployment.md`
- `docs/architecture.md`
- `docs/release-checklist.md`
- `docs/adr/0046-event-log-everywhere.md`
- `docs/adr/0054-security-hardening-sweep.md`
- `crates/aegis-server/src/main.rs`
- `crates/aegis-server/src/service.rs`
- `crates/aegis-server/src/accounts/mod.rs`
- `crates/aegis-server/src/accounts/events.rs`
- `crates/aegis-server/src/store/audit.rs`
- `crates/aegis-server/migrations/004_event_chain.sql`
- `crates/aegis-shell/src/main.rs`
