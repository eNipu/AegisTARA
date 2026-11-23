# ADR 0072: Authentication origin and attempt isolation

- Status: accepted (job 214-auth-origin-isolation)
- Date: 2026-11-23
- Crates: `aegis-auth`, `aegis-client`, `aegis-shell`
- Source: architecture review 2026-11-22; `factory/backlog.md` job 214

## Context

Password and OIDC login used asynchronous tasks whose completions did not identify the attempt that produced them. A user could select server B while a login for server A was outstanding; A's late result could then be installed into B's provider. Password credentials also ignored the request URL. Similar races allowed an older OIDC login, restore or automatic refresh to overwrite or clear newer remembered credentials.

Checking only the URL in a completion handler is insufficient: retries against the same server are also distinct attempts, and queued completions can remain after task abortion. Discarding a UI completion cannot by itself undo a token already written to the keychain.

## Decision

### Origin boundary

`Origin` compares the normalized scheme, host and effective port. RPC paths and queries do not change the origin; scheme changes, including HTTP to HTTPS upgrades, do. Malformed or relative URLs, userinfo and fragments are refused.

`PasswordLogin` stores its token and bound origin under one lock and issues credentials only to that origin. A provider cannot be retargeted, including after its token is cleared. `OidcSettings.server_url` identifies the API server separately from the identity-provider issuer and OAuth resource; the provider checks it before refreshing or signing a request.

Password keychain entries use the canonical origin, preserving existing names for already-canonical URLs. Legacy aliases migrate only after the canonical write succeeds. Replacing or forgetting a login cannot leave an equivalent-origin alias behind.

### Attempt boundary

`AuthAttempts` gives every discovery, password login, OIDC login and restore a unique generation and captured server. Only the current completion is accepted, once. Server changes, same-server retries, login-method switches, sign-out, standalone transitions and shutdown cancel the old task and invalidate queued completions before they can change UI, settings or credentials.

`LoginAttempt` also guards token and keychain commits inside `aegis-auth`. Login, restore and automatic-refresh methods recheck it while holding the asynchronous token lock, then perform the synchronous storage and in-memory commit while holding the cancellation lock. Cancellation therefore prevents a stale network result from committing; it does not pretend to undo a commit that had already finished.

Replacing or stopping a provider invalidates its lifetime. Password invalidation clears only the old in-memory token, not a remembered keychain entry that a newer provider may own. OIDC invalidation prevents later credentials and automatic-refresh commits. Both launchers use the same attempt guard rather than maintaining different generation schemes.

## Not chosen

- URL-prefix matching or issuer-only binding: paths are not origins, and one identity provider can serve several API servers.
- Checking only the server string: it does not distinguish same-server retries.
- UI-only dropping of stale events: it cannot prevent keychain side effects already in flight.
- A process-global account registry or compare-and-swap extension to every secret-store backend: provider lifetime cancellation covers the desktop owners without changing the storage API. Separate processes remain responsible for their own OS-account login coordination.
- Detached OIDC logout followed immediately by another login: delayed logout could erase the newer login. Existing logout behavior is not broadened by this job.

## Consequences

- Late successes and failures are harmless, not only late tokens.
- Existing canonical password entries remain compatible; alias entries migrate on restore.
- Public password-login setup now supplies the server URL and can fail on an invalid or different origin. `OidcSettings` explicitly requires `server_url`.
- Tests cover delayed discovery/login/restore, same-server retry, sign-out and standalone transitions, origin confusion, keychain aliases, cancelled token commits and automatic-refresh success/`invalid_grant` races.
