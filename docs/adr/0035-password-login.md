# ADR 0035: Password login

- Status: accepted (job 040-password-login-and-grpc-web)
- Date: 2026-10-25
- Scope: `crates/aegis-server/src/accounts/`, `auth_layer.rs`, `migrations/002_accounts_baselines.sql`, `crates/aegis-client/src/connection.rs`, `crates/aegis-shell/src/{launcher,provider}.rs`
- Rules: R3, R4, R5, R6, C2, D10, D11, Q3 in `docs/engineering-standards.md`; decision D10
- Builds on: `0028-login.md`

## Context

The test server needs logins that work without a company identity provider.
Customer projects will use SSO (ADR 0028, backlog), so this login must be
simple, safe enough for test data, and easy to turn off.

## Decisions

### 1. One server setting picks the method

`AEGIS_AUTH` is `password`, `oidc` or `dev`. The public `Accounts` service
tells the app which one (`GetLoginOptions`), so the app needs no setting.
With `oidc`, `LogIn` answers `FAILED_PRECONDITION` (password login off).

### 2. Opaque session tokens, not JWTs

A token is `ats_` plus 32 random bytes. The server stores only its SHA-256.
A database leak gives no usable token. Logging out, disabling a user or
changing a password deletes the sessions at once, which a JWT cannot do
without a deny list. Sessions end at an absolute time (12 h by default).

### 3. Argon2id with fixed cost

19 MiB, 2 passes, 1 lane (OWASP minimum). Hashing runs on `spawn_blocking`
behind a semaphore with a wait timeout, so a login burst cannot starve the
async runtime or the memory (R4, R5, R6). An unknown name is checked against
a decoy hash, so response time does not reveal which names exist.

### 4. Throttle per user name

Five free failures, then the wait doubles from 1 s to 15 min. The reply is
`RESOURCE_EXHAUSTED` with a `retry-after` header. The map is capped (R3).
There is no per-IP throttle: behind Railway or a load balancer every client
can share one address. A per-IP limit belongs in the proxy.

### 5. One fixed error message

Every login failure (wrong name, wrong password, disabled) gives the same
`UNAUTHENTICATED` text. The event log keeps the real reason.

### 6. First admin from the environment

When the user table is empty, the server creates `AEGIS_ADMIN_USERNAME`
with `AEGIS_ADMIN_PASSWORD`. After that the variables are ignored. The last
enabled admin cannot be deleted, disabled or demoted.

### 7. The app keeps a token, never a password

"Keep me logged in" stores the session token in the OS keychain under the
server and user name. The settings file holds the server, transport, user
name and export folder only.

## Consequences

- No password reset by e-mail and no MFA. That is acceptable for testing and
  is the reason customers get SSO.
- The in-memory account state is a cache of the database, rebuilt at start
  (D10). One server instance per database is assumed; two instances would not
  see each other's new sessions until restart. That matches D4 (one server).
