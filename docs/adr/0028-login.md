# ADR 0028: Login: OIDC code flow with PKCE, DPoP-bound tokens, keychain

- Status: proposed (job 031-login)
- Date: 2026-10-25
- Scope: `crates/aegis-auth`, `crates/aegis-server/src/auth_layer.rs`, `crates/aegis-proto`
- Rules: R3, R4, R6, Q3, Q4 in `docs/engineering-standards.md`; D5 in `docs/decisions.md`
- Builds on: `direction-v1.md` section 6, `0026-server-skeleton.md` (the `Principal` seam)

## Context

Direction section 6 sets the login design: company single sign-on through the
system browser (RFC 8252, code flow with PKCE and a loopback redirect), a
short-lived access token, a refresh token in the OS keychain, and tokens
bound to a device key with DPoP (RFC 9449). The server validates the token
and the proof on every call.

## Decisions

### 1. Hand-written protocol layer on `ring`, not an OAuth framework

| Option | Why not |
|---|---|
| `openidconnect` / `oauth2` crates | Pull a large HTTP and TLS stack and a second JOSE implementation; no DPoP; harder to bound sizes and to test the exact checks. |
| `jsonwebtoken` | Its RSA support uses the `rsa` crate, which carries an open RUSTSEC advisory (Marvin attack) that `cargo-deny` would fail. We only *verify*, but the advisory gate is not negotiable. |
| **`ring` for signatures, ~1,000 lines of protocol code** | `ring` verifies ES256 and RS256 and signs ES256; license `Apache-2.0 AND ISC` passes D5; no advisory. The protocol code is small enough to test against the RFC vectors. |

JOSE scope is deliberately tiny: compact JWS, `ES256` and `RS256` only, public
JWKs only. `none`, HMAC, `crit`, private JWKs, RSA below 2048 bits and tokens
over 16 KiB are refused by name (`jose.rs`). Tests include the RFC 7636
appendix B PKCE vector and the RFC 7638 section 3.1 thumbprint vector, and an
RS256 token signed offline by a throwaway RSA key.

### 2. DPoP on every call, as a layer

The DPoP proof covers the HTTP method and URL, which a tonic interceptor never
sees. So both sides use tower layers: `SignRequests` on the client channel
and `AuthLayer` in front of the server. The server rebuilds the URL from
`AEGIS_PUBLIC_URL` plus the request path.

Server checks (all tested): token signature against the provider's JWKS,
`iss`, `aud`, `exp`, `nbf`, `iat` (60 s skew), `sub`; for a bound token
(`cnf.jkt`): the `DPoP` scheme, a proof with type `dpop+jwt`, ES256, an
embedded public key whose thumbprint equals `cnf.jkt`, `htm`, `htu`, `iat`
inside a 60 s window, `ath` equal to the token hash, and a `jti` never seen in
the window. The replay cache is bounded (200,000 entries, about 10 MB) and
**fails closed**: when full it refuses new proofs instead of forgetting old
ones. A bound token sent as `Bearer` is refused.

`AEGIS_DPOP=optional` accepts unbound tokens as plain bearer tokens, for
identity providers that cannot bind tokens. It is off by default, logged at
startup, and its weaker guarantee ("whoever holds the token") is documented.

### 3. Signing keys: cached, rotated, throttled

`JwksCache` fetches keys on first use and after a 10 minute TTL. A token with
an unknown `kid` triggers one refetch, at most once per 30 s, so random
`kid`s cannot turn the server into a request amplifier against the provider
(test: 5 unknown-`kid` calls cause 0 extra fetches inside the throttle). One
fetch is shared by concurrent callers. A key without a `kid` is used only if
it is the single usable key.

### 4. Session on the client

`Session` holds the tokens in memory and the refresh token in a `SecretStore`.
`credentials()` refreshes 30 s before expiry, **single flight**: concurrent
calls wait for one refresh, because providers that rotate refresh tokens
would reject a second use (test). A refresh answered `invalid_grant` clears
the stored token and reports `LoginRequired`, which the UI turns into the
login screen. The device key is generated once, stored as PKCS#8 in the
keychain, never leaves the machine, and survives logout. Secrets are never
printed: `Debug` for `TokenSet`, `Credentials`, and `DeviceKey` omit them.

### 5. Loopback receiver

Binds `127.0.0.1` on a random port; serves only `GET /callback` with the
exact `Host`; ignores requests whose `state` differs (a forged redirect cannot
end the wait or inject a code); caps requests at 8 KiB with a 5 s read
timeout; times out after the caller's deadline. The provider's `error` code
is filtered to `[A-Za-z0-9_.-]` and 64 characters before it is shown.

### 6. Secrets in the OS keychain

`SecretStore` is a three-method trait. `OsKeychain` (feature `os-keychain`,
`keyring-core` plus one store crate per platform, all MIT or Apache-2.0) uses
Windows Credential Manager, macOS Keychain, or the Linux Secret Service.
`MemorySecretStore` backs tests and a "do not remember me" mode. Server builds
do not compile the keychain.

The umbrella `keyring` crate was rejected on Linux: its Secret Service store
pulls the pure-Rust `zbus` 5 stack, whose `zvariant`, `zvariant_derive` and
`zbus_names` crates carry unmaintained advisories with no fix, and
`deny.toml` keeps `advisories.ignore` empty. The `dbus` based store (libdbus
built from source with `vendored`) has no advisories. Cost: a C compiler at
build time on Linux. All four `cargo-deny` checks pass for every crate.

### 7. Shared API crate

The generated gRPC code moved to `aegis-proto`, so the client depends on the
API without the server's database code.

## What this does not cover

- **HTTPS to the identity provider.** `HyperClient` speaks `http://` to
  loopback only and refuses everything else with a clear error. TLS needs
  rustls plus a crypto provider and the OS trust store; the candidates
  (ring, rustls, rustls-native-certs) look D5-clean, but the review and the
  `cargo-deny` run belong to their own job: backlog `038-https-client`. Until
  then a real provider can only be reached through a local TLS-terminating
  proxy, or in development with a loopback provider.
- **Device-code flow and local accounts** (direction section 6): not started.
- **Cedar policies, step-up login, session list and revoke:** later. The only
  authorization today is the admin role from a token claim.
- **Provider quirks beyond the RFCs** (for example Entra ID not issuing
  DPoP-bound tokens): covered by `AEGIS_DPOP=optional`, untested against the
  real provider.

## Verification

`crates/aegis-auth/tests/login.rs`: login through a mock provider and
the loopback, refresh with rotation, single flight, restore from the keychain,
revocation, nonce retry, timeout, replay and stolen-token cases, the DPoP
policy matrix, key rotation, and the RS256 fixture.
`crates/aegis-server/tests/login.rs`: the same flow through the real gRPC
server: the lease holder is the token's `sub`, admin comes from the token, and
unauthenticated calls get one generic message.
