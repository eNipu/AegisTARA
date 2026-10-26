# AegisTARA — Security & Rust Systems Architecture Review

**Date:** 2026-10-26  
**Author:** Principal Rust Systems & Security Architect  
**Scope:** Full workspace audit across all 8 production crates (`aegis-model`, `aegis-proto`, `aegis-server`, `aegis-auth`, `aegis-license`, `aegis-canvas`, `aegis-client`, `aegis-shell`), supply-chain configuration ([`deny.toml`](file:///Users/lamda/Documents/GitHub/AegisTARA/deny.toml)), container deployment ([`Containerfile`](file:///Users/lamda/Documents/GitHub/AegisTARA/Containerfile)), and factory governance (`001`–`055`).

---

## 1. Executive Summary

AegisTARA is an exceptionally well-engineered Rust system. Across ~42,000 lines of Rust, the codebase demonstrates rare discipline:

- **Memory & Type Safety (`P1`, `Q3`)**: Every crate in the workspace enforces `unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`, `todo = "deny"`, and `dbg_macro = "deny"` via [`Cargo.toml`](file:///Users/lamda/Documents/GitHub/AegisTARA/Cargo.toml#L36-L52).
- **Concurrency & Ownership (`C1`, `R4`, `R5`)**: Both [`aegis-server::actor::Actor`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/actor.rs#L410-L438) and [`aegis-client::session::Actor`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/session.rs#L394-L426) follow strict single-owner actor semantics over bounded `tokio::sync::mpsc` channels, offloading CPU-heavy serialization, deserialization, Argon2id hashing, and PDF/HTML report generation to `tokio::task::spawn_blocking`.
- **Cryptographic Hygiene**:
  - Password sessions use 256-bit CSPRNG bearer tokens (`ats_...`) whose raw values are never stored in memory or PostgreSQL—only their SHA-256 [`SessionKey`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L94-L115) is persisted.
  - Edit leases use `subtle::ConstantTimeEq` ([`ct_eq`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/lease.rs#L171-L173)) to prevent timing side channels.
  - Licenses (`AEGIS-LICENSE-V1`) and update manifests (`AEGIS-UPDATE-V1`) use domain-separated Ed25519 signatures via `ring` with independent trust roots ([`TrustedKeys`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-license/src/envelope.rs#L49-L119)).
  - OAuth 2.0 + PKCE + RFC 9449 DPoP sender-constrained tokens in [`aegis-auth`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/lib.rs) bind every gRPC request to a client-held ES256 key (`cnf.jkt`) and the access token hash (`ath`).
  - Update downloads stream through [`UpdateOffer::download`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/updates.rs#L140-L212) into `OpenOptions::create_new(true)` `.part` files inside `0700` private directories, enforcing signed byte caps on the fly before atomic rename.

Despite this strong foundation, a deep architectural and adversarial review uncovered **12 concrete security and architectural findings** (**2 High**, **5 Medium**, **3 Low**, **2 Informational**) that should be addressed before enterprise multi-tenant or internet-facing production deployment.

### Summary Matrix of Findings

| ID | Severity | Category | Component | Title |
|---|---|---|---|---|
| **SEC-01** | **High** | Authentication / Brute-Force Protection | `aegis-server::accounts` | Login `Throttle` wipes all lockouts globally when capacity (`10,000`) is reached |
| **SEC-02** | **High** | Access Control / Multi-Tenancy | `aegis-server::service` | Flat authorization model: any authenticated user can read, lock, and modify every project |
| **SEC-03** | **Medium** | Availability / DoS | `aegis-auth::dpop` | Non-monotonic `expires` in `ReplayCache` FIFO queue blocks eviction and enables fail-closed DoS |
| **SEC-04** | **Medium** | Availability / DoS | `aegis-server::accounts` | Unauthenticated `LogIn` saturates global 4-permit Argon2id semaphore without per-IP rate limiting |
| **SEC-05** | **Medium** | Transport Security | `aegis-server::store::postgres` | `DbTls::Encrypt` uses `AnyCertificate` verifier that disables all certificate & hostname validation |
| **SEC-06** | **Medium** | Audit Integrity & Compliance | `aegis-server::accounts::events` | Security event log (`aegis_event`) lacks hash-chain integrity, drops events on DB error, and silently trims |
| **SEC-07** | **Medium** | Data Consistency / Auth Revocation | `aegis-server::accounts::postgres` | Multi-step account & session revocation mutations execute without PostgreSQL transactions |
| **SEC-08** | **Low** | Memory Hygiene / Cryptography | `aegis-auth`, `aegis-server` | Sensitive key material (`pkcs8`, passwords, bearer tokens) is not zeroized on drop (`zeroize` absent) |
| **SEC-09** | **Low** | Local File Safety | `aegis-client::settings`, `session` | `Settings::save` and `Session::export` use predictable paths without `create_new` or `0600` permissions |
| **SEC-10** | **Low** | Network Hardening | `aegis-server::server` | `tonic::transport::Server` lacks explicit HTTP/2 frame/stream/keepalive limits and request timeouts |
| **SEC-11** | **Info** | Supply Chain | `deny.toml`, `aegis-shell` | Unmaintained transitive dependencies via Slint (`ttf-parser`, `zvariant`) require tracking on upgrades |
| **SEC-12** | **Info** | Distributed Systems Architecture | `aegis-server` | Single-node stateful assumptions in `Registry`, `Accounts`, and `ReplayCache` prevent horizontal scaling |

---

## 2. Detailed Security & Architectural Findings

### SEC-01 (High): Login `Throttle` Wipes All Lockouts Globally When Capacity (`10,000`) Is Reached

- **Location**: [`crates/aegis-server/src/accounts/mod.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L482-L539) and [`log_in`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L748-L779)
- **Vulnerability Mechanics**:
  `Throttle` protects accounts from password brute-forcing using exponential backoff (`5` failures -> `2s`, doubling up to `300s`). To bound memory (`R3`), `Throttle::record_failure` checks:
  ```rust
  if self.names.len() >= Self::CAPACITY && !self.names.contains_key(username) {
      self.names.clear();
  }
  ```
  When an attacker sends failed login attempts for `10,000` distinct usernames (e.g., `u00001`..`u10000`), `self.names.clear()` **wipes the entire map**, immediately resetting the failure counter and `blocked_until_ms` lockout of every targeted real account (such as `admin`) back to `0`.
  Furthermore, in [`Accounts::log_in`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L755-L756):
  ```rust
  let normalized = normalize_username(raw_username).ok();
  let key = normalized.clone().unwrap_or_default();
  ```
  Every syntactically invalid username maps to `key = ""`, meaning any user who accidentally types an invalid character shares the `""` throttle bucket with all other malformed login attempts.
- **Architectural Remediation**:
  1. Reject malformed usernames (`normalize_username(raw_username).is_err()`) **before** touching `Throttle` or running Argon2id—username syntax rules (`1..=64` ASCII lowercase/digits/`._-@`) are public and require zero secret lookups.
  2. Separate tracking of **known users** (bounded by `max_users = 10,000`, never evicted by unknown-username noise) from an LRU/FIFO ring or Count-Min sketch for unknown usernames, or evict only the entry with the oldest `last_failed_ms` / smallest `failures` rather than calling `self.names.clear()`.

```rust
// Recommended replacement in Throttle::record_failure:
if self.names.len() >= Self::CAPACITY && !self.names.contains_key(username) {
    // Evict the entry whose lockout/failure is oldest, never clearing active lockouts globally.
    if let Some(oldest_key) = self
        .names
        .iter()
        .min_by_key(|(_, e)| (e.blocked_until_ms, e.last_failed_ms))
        .map(|(k, _)| k.clone())
    {
        self.names.remove(&oldest_key);
    }
}
```

---

### SEC-02 (High): Flat Authorization Model — Any Authenticated User Can Read, Lock, and Modify Every Project

- **Location**: [`crates/aegis-server/src/service.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/service.rs#L228-L553) and [`crates/aegis-server/src/auth.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/auth.rs#L17-L29)
- **Vulnerability Mechanics**:
  [`Principal`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/auth.rs#L17-L29) carries only `subject: Arc<str>`, `admin: bool`, and `method: AuthMethod`. In [`TaraService`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/service.rs):
  - `list_projects` returns **all** projects in `Registry` to any authenticated caller.
  - `open_project`, `watch`, `get_project_history`, `list_baselines`, and `get_baseline` allow any authenticated caller to read any project's full TARA model and history.
  - `acquire_lock`, `submit_ops`, `rename_project`, and `create_baseline` allow any authenticated caller (when a license seat is available) to lock and modify any project, approve/verify cybersecurity controls (`SetControlStatus`), and alter risk treatment decisions.
- **Impact**:
  In automotive OEM/Tier-1 environments (ISO/SAE 21434), TARA models contain sensitive unmitigated attack paths across vehicle programs, and separating **Author/Analyst**, **Reviewer/Assessor** (who can mark controls `Verified` or create baselines), and **Viewer/Auditor** roles per project is a core governance requirement.
- **Architectural Remediation**:
  1. Introduce a per-project membership & role table (`aegis_project_member(project_id, username, role)`) with roles `Viewer`, `Editor`, `Reviewer`, and `Owner` (with global `admin` retaining break-lock and project-management privileges).
  2. Enforce role checks at the [`TaraService`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/service.rs) boundary *and* inside [`Actor::submit`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/actor.rs#L681-L773) (e.g., restricting `Op::SetControlStatus { status: ControlStatus::Verified, .. }` and `CreateBaseline` to `Reviewer`/`Owner`).

---

### SEC-03 (Medium): Non-Monotonic `expires` in `ReplayCache` FIFO Queue Blocks Eviction and Enables Fail-Closed DoS

- **Location**: [`crates/aegis-auth/src/dpop.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/dpop.rs#L254-L282)
- **Vulnerability Mechanics**:
  `ReplayCache::insert` evicts expired `jti` digests from the front of a `VecDeque<(u128, u64)>` (`inner.order`) using an early `break`:
  ```rust
  while let Some(&(k, exp)) = inner.order.front() {
      if exp > now {
          break;
      }
      inner.order.pop_front();
      inner.live.remove(&k);
  }
  if inner.live.len() >= self.max_entries {
      return Err(AuthError::DpopInvalid("replay cache is full".into()));
  }
  ```
  However, `expires` is computed from the **client-supplied** `claims.iat` (`let expires = claims.iat.saturating_add(self.window);`), where `claims.iat` is accepted anywhere in `[now - max_age (60s), now + max_future_skew (5s)]`.
  Because `expires` is **not** monotonic in arrival order:
  - A request with `iat = now + 5` has `expires = now + 65` and sits at the head of `inner.order`.
  - Subsequent requests with `iat = now - 59` have `expires = now + 1`, meaning they expire in 1 second, **but they are never evicted from `inner.order` or `inner.live` for the next 64 seconds** because the front element has `exp = now + 65 > now`!
  - Because `ReplayCache` is fail-closed at `MAX_REPLAY_ENTRIES = 200,000` **and** runs in [`Authenticator::authenticate`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/validate.rs#L229-L263), this head-of-line blocking extends cache residency by up to `max_age + max_future_skew` (`65s`), making it significantly easier to fill `inner.live` and deny all DPoP-authenticated RPCs server-wide.
- **Architectural Remediation**:
  Set `expires = now.saturating_add(self.window)` (where `self.window = max_age + max_future_skew = 65s`) instead of `claims.iat + self.window`. Because `now` is the server's monotonic/wall clock at insertion time, `expires` is guaranteed to be monotonically non-decreasing in `inner.order`, making the `VecDeque` eviction exact in `O(1)` amortized time and keeping every `jti` blocked for its entire validity window.

```rust
// In ReplayCache::insert (crates/aegis-auth/src/dpop.rs):
let window = self.max_age.saturating_add(self.max_future_skew);
let expires = now.saturating_add(window);
```

---

### SEC-04 (Medium): Unauthenticated `LogIn` Saturates Global 4-Permit Argon2id Semaphore Without Per-IP Rate Limiting

- **Location**: [`crates/aegis-server/src/accounts/mod.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L748-L785) and [`AccountsConfig`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L406-L429)
- **Vulnerability Mechanics**:
  To prevent CPU starvation on unknown usernames, [`Accounts::log_in`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L774-L779) intentionally runs Argon2id against `self.decoy` when the username does not exist. All Argon2id evaluations share a global semaphore (`hashing_permits: 4`, `hashing_wait: Duration::from_secs(5)`).
  - At ~25 ms per Argon2id hash across 4 permits, total server throughput is ~160 logins/sec.
  - Because `LogIn` is unauthenticated and rotates usernames to bypass per-username `Throttle`, a single unauthenticated client sending ~800 concurrent `LogIn` requests with random unknown usernames keeps all 4 permits saturated beyond `hashing_wait` (5s), causing every legitimate user's `LogIn` and `ChangePassword` call to fail with `AccountError::Busy` (`Status::unavailable("the server is busy; try again")`).
- **Architectural Remediation**:
  1. Reserve separate semaphore permits for **authenticated** password operations (`ChangePassword`, admin `CreateUser`/`UpdateUser`) vs. **unauthenticated** `LogIn`, and/or prioritize known-user verification over unknown-user decoy hashing when permits are >50% utilized.
  2. Add a keyed token-bucket rate limiter per peer IP / `X-Forwarded-For` (when behind a trusted proxy) at the Tower layer for `LogIn`.

---

### SEC-05 (Medium): `DbTls::Encrypt` Uses `AnyCertificate` Verifier That Disables Certificate & Hostname Validation

- **Location**: [`crates/aegis-server/src/store/postgres.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/store/postgres.rs#L88-L133) and [`crates/aegis-server/src/main.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/main.rs#L171-L185)
- **Vulnerability Mechanics**:
  When `AEGIS_DB_TLS=encrypt` is configured (which [`docs/deploy-railway.md`](file:///Users/lamda/Documents/GitHub/AegisTARA/docs/deploy-railway.md) recommends for managed Postgres providers with private CAs), `make_tls` constructs a `rustls::ClientConfig` with `AnyCertificate`:
  ```rust
  impl ServerCertVerifier for AnyCertificate {
      fn verify_server_cert(...) -> Result<ServerCertVerified, rustls::Error> {
          Ok(ServerCertVerified::assertion())
      }
  ```
  This corresponds to PostgreSQL's `sslmode=require` (encryption without server authentication) and is vulnerable to active man-in-the-middle attacks on the database network path.
- **Architectural Remediation**:
  1. Add `AEGIS_DB_CA_FILE` support to [`DbTls::Verify`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/store/postgres.rs#L48-L55) so operators using private CAs or cloud-managed Postgres instances can load a pinned CA bundle via `rustls_pemfile` and use full certificate + hostname verification (`DbTls::Verify`) instead of falling back to `DbTls::Encrypt`.
  2. Emit a `tracing::warn!` at startup whenever `DbTls::Encrypt` or `DbTls::Disable` is used in non-loopback deployments.

---

### SEC-06 (Medium): Security Event Log (`aegis_event`) Lacks Hash-Chain Integrity, Drops Events on Error, and Trims Silently

- **Location**: [`crates/aegis-server/src/accounts/events.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/events.rs#L173-L237) and [`crates/aegis-server/src/accounts/postgres.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/postgres.rs#L276-L318)
- **Vulnerability Mechanics**:
  While project operations in `aegis_project_op` are protected by a cryptographic SHA-256 hash chain (`AEGIS-AUDIT-V1` in [`crates/aegis-server/src/store/audit.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/store/audit.rs)), the security event log (`aegis_event`, recording logins, failed logins, user creations/deletions, session revocations, lock breaks, and document exports) has three weaknesses:
  1. **No Tamper Evidence**: Rows in `aegis_event` have no `prev_hash` / `hash` columns; a compromised DB role or insider with SQL access can delete or alter `export_recorded`, `lock_broken`, or `user_created` rows undetected.
  2. **Silent Trimming**: [`trim_events`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/postgres.rs#L298-L318) deletes all rows beyond `max_events = 10,000`. Because `login_failed` events share the same `10,000`-row table as high-value administrative events (`project_deleted`, `user_created`, `export_recorded`), an attacker generating `10,000` failed logins will **purge all historical administrative and export audit events** from `aegis_event`!
  3. **Best-Effort Fire-and-Forget**: [`EventLog::record`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/events.rs#L219-L237) logs a `tracing::warn!` and discards the event if `put_event` fails.
- **Architectural Remediation**:
  1. **Prevent `login_failed` from Evicting Admin/Governance Events**: Partition retention by event class (e.g., cap `login_failed`/`login_ok`/`logout` separately from governance events like `user_*`, `project_deleted`, `lock_broken`, `export_recorded`), or never auto-delete governance events.
  2. **Extend Hash-Chain Tamper Evidence**: Add `prev_hash` and `hash` (`AEGIS-EVENT-AUDIT-V1`) to governance audit events so `VerifyAudit` covers both project history and administrative/export events.

---

### SEC-07 (Medium): Multi-Step Account & Session Revocation Mutations Execute Without PostgreSQL Transactions

- **Location**: [`crates/aegis-server/src/accounts/mod.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L884-L902), [`change_password`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L983-L1003), [`update_user`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L1155-L1161), and [`crates/aegis-server/src/accounts/postgres.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/postgres.rs#L156-L275)
- **Vulnerability Mechanics**:
  Unlike [`PostgresStore::append`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/store/postgres.rs#L422-L501) (which uses an explicit SQL transaction), `Accounts` performs multi-step mutations as separate pooled queries:
  - In `update_user` (when disabling a user or resetting their password) and `change_password`:
    1. `self.backend.put_user(&user).await?` succeeds.
    2. In-memory `state.sessions` removes the user's sessions.
    3. `self.backend.delete_sessions(&ended).await?` runs as a separate DB connection acquisition and query.
  - If the server crashes or loses its DB connection between step 1 and step 3, the revoked sessions remain in `aegis_session` in PostgreSQL. On the next server restart (`Accounts::open`), if the user was only having their password changed (and is still `!disabled`), those pre-password-change sessions are reloaded from `aegis_session` into memory and become valid again!
- **Architectural Remediation**:
  Combine `put_user` + `delete_sessions` (and `put_password_login(false)` + `delete_sessions`) into a single atomic backend method backed by a PostgreSQL transaction (`client.transaction().await`).

---

### SEC-08 (Low): Sensitive Key Material (`pkcs8`, Passwords, Bearer Tokens) Is Not Zeroized on Drop

- **Location**:
  - [`crates/aegis-auth/src/dpop.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/dpop.rs#L35-L41) (`DeviceKey { pkcs8: Vec<u8>, ... }`)
  - [`crates/aegis-auth/src/oidc.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/oidc.rs#L74-L87) (`TokenSet { access_token, refresh_token, ... }`)
  - [`crates/aegis-server/src/accounts/mod.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L244-L253) (`NewSession { token: String, ... }`)
- **Vulnerability Mechanics**:
  Although [`PROJECT_REVIEW.md`](file:///Users/lamda/Documents/GitHub/AegisTARA/PROJECT_REVIEW.md) §5.2 mentions `zeroize` for secret material, `zeroize` is not currently a dependency in [`Cargo.toml`](file:///Users/lamda/Documents/GitHub/AegisTARA/Cargo.toml). When `DeviceKey`, `TokenSet`, `PkcePair`, or password strings are dropped, their heap buffers remain in process memory until reallocated, exposing them to core dumps, swap files, or crash dumps.
- **Architectural Remediation**:
  Add `zeroize = { version = "1.8", features = ["derive"] }` to `[workspace.dependencies]` and derive `Zeroize` + `ZeroizeOnDrop` on `DeviceKey` (`pkcs8`), `PkcePair` (`verifier`), `TokenSet`, and `NewSession`.

---

### SEC-09 (Low): `Settings::save` and `Session::export` Use Predictable Paths Without `create_new` or `0600` Permissions

- **Location**: [`crates/aegis-client/src/settings.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/settings.rs#L106-L114) and [`crates/aegis-client/src/session.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/session.rs#L1185-L1192)
- **Vulnerability Mechanics**:
  - `Settings::save` writes to a fixed temporary path `path.with_extension("json.tmp")` via `std::fs::write(&tmp, text)` with default umask permissions.
  - `Session::export` writes confidential TARA HTML/PDF/CSV/SVG reports directly via `std::fs::write(&path, bytes)` with default umask permissions (typically `0644` on Unix, making confidential TARA reports world-readable on shared workstations).
- **Architectural Remediation**:
  Set `OpenOptionsExt::mode(0o600)` on Unix when creating `settings.json.tmp` and exported TARA report files, and write exports via a `.part` temporary file renamed into place just like [`UpdateOffer::download`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/updates.rs#L362-L383).

---

### SEC-10 (Low): `tonic::transport::Server` Lacks Explicit HTTP/2 Keepalive, Concurrency, and Timeout Settings

- **Location**: [`crates/aegis-server/src/server.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/server.rs#L94-L111)
- **Vulnerability Mechanics**:
  `serve_with_shutdown` builds `tonic::transport::Server::builder().accept_http1(true)` with default Hyper HTTP/1.1 and HTTP/2 settings: no `concurrency_limit_per_connection`, no `max_concurrent_streams`, no `http2_keepalive_interval`, and no `tcp_keepalive`. Slowloris or idle connection floods can hold open socket descriptors and HTTP/2 streams indefinitely.
- **Architectural Remediation**:
  Configure explicit transport bounds on `tonic::transport::Server::builder()`:
  ```rust
  tonic::transport::Server::builder()
      .accept_http1(true)
      .tcp_nodelay(true)
      .tcp_keepalive(Some(Duration::from_secs(60)))
      .http2_keepalive_interval(Some(Duration::from_secs(30)))
      .http2_keepalive_timeout(Some(Duration::from_secs(10)))
      .max_concurrent_streams(Some(128))
      .concurrency_limit_per_connection(64)
  ```

---

### SEC-11 (Informational): Unmaintained Transitive Dependencies in `aegis-shell` via Slint

- **Location**: [`deny.toml`](file:///Users/lamda/Documents/GitHub/AegisTARA/deny.toml#L77-L82)
- **Observation**:
  Four RustSec `unmaintained` advisories (`RUSTSEC-2026-0192` `ttf-parser`, `RUSTSEC-2026-0370` `zvariant`, `RUSTSEC-2026-0371` `zbus_names`, `RUSTSEC-2026-0372` `zvariant_derive`) are explicitly documented and confined to `aegis-shell` via `[bans]` (`wrappers = ["aegis-shell"]`). Notably, job `050` ([`crates/aegis-client/src/pdf.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/pdf.rs#L19-L24)) smartly bypassed `krilla`'s default text shaper (`rustybuzz` -> `ttf-parser`) by using `skrifa` directly, keeping `aegis-client` 100% free of advisory ignores.
- **Recommendation**:
  Re-evaluate the four ignores on every `slint` version bump and remove them as soon as `winit` / `accesskit_unix` migrate upstream.

---

### SEC-12 (Informational): Single-Node Stateful Server Architecture

- **Location**: [`crates/aegis-server/src/registry.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/registry.rs), [`crates/aegis-server/src/accounts/mod.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs), [`crates/aegis-auth/src/dpop.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/dpop.rs)
- **Observation**:
  `aegis-server` is architected as a **single-instance stateful server** backed by PostgreSQL:
  - `Registry` owns in-memory `Actor` tasks and `LeaseState` per project; two `aegis-server` instances pointing at the same PostgreSQL database would not share edit leases, `SeatTable` counts, in-memory `Accounts` session tables, or `ReplayCache`.
- **Recommendation**:
  Document explicitly in deployment docs that `aegis-server` must be deployed as a single active replica (`replicas: 1`) per PostgreSQL database, or acquire a PostgreSQL session-level advisory lock (`pg_try_advisory_lock`) in [`PostgresStore::connect`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/store/postgres.rs#L172-L202) so a second replica accidentally started by an orchestrator fails fast at boot instead of causing split-brain state.

---

## 3. Crate-by-Crate Architectural Assessment

### 3.1 `aegis-model` (Core TARA Engine)
- **Strengths**:
  - Textbook data-oriented Rust design: generational [`Arena<T>`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/src/arena.rs) with `Idx<T>` indices, sparse refcount maps, and explicit non-recursive graph algorithms (`dfs`, topological evaluation, minimal cut sets with absorption and `CutSetLimits`).
  - Every mutation goes through [`Op`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/src/op.rs) and returns its exact inverse `Op`, making undo/redo and transactional server rollback (`Actor::rollback`) provably symmetric.
  - Binary snapshot format v4 ([`save.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/src/save.rs)) and [`OpCodec`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/src/codec.rs) enforce strict pre-allocation caps (`MAX_SNAPSHOT_BYTES = 64 MiB`, `MAX_OP_BYTES = 256 KiB`, `MAX_ATTACK_TREE_NODES = 10,000`) and are backed by 4 `cargo-fuzz` targets and proptest suites.
  - [`ProjectEvalCache`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/src/cache.rs) achieves ~9.5 µs incremental single-leaf updates on 5,000-node projects with proptest-verified equivalence against fresh evaluation.

### 3.2 `aegis-proto` (Wire Schema)
- **Strengths**:
  - Compiles `proto/aegis/v1/tara.proto` via pure-Rust `protox` in `build.rs`, eliminating external `protoc` binary supply-chain risk.
  - Clean separation between domain types and protobuf wire types; `aegis-model` has zero dependency on `prost` or `tonic`.

### 3.3 `aegis-auth` (OIDC + PKCE + DPoP + Keychain)
- **Strengths**:
  - Minimal, auditable JOSE/JWS implementation ([`jose.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/jose.rs)) supporting only `ES256` and `RS256` via `ring`—explicitly rejecting `"none"`, symmetric `HS*`, and key-confusion vectors.
  - [`HyperClient`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/http.rs) enforces HTTPS-only (except loopback for tests), zero HTTP redirects, 10s request timeouts, 1 MiB response body limits, and 30s per-chunk streaming download timeouts.
  - Loopback OAuth listener ([`loopback.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/loopback.rs)) binds exclusively to `127.0.0.1:0`, caps the HTTP request at 8 KiB, and verifies the CSRF `state` parameter in constant time.
- **Improvement**: Fix `ReplayCache` non-monotonic expiry (**SEC-03**) and add `ZeroizeOnDrop` (**SEC-08**).

### 3.4 `aegis-license` (Ed25519 Signed Licenses & Update Manifests)
- **Strengths**:
  - Separate compile-time Ed25519 public key sets for licenses (`TrustedKeys::built_in`) and updates (`TrustedKeys::for_updates`) plus distinct domain prefixes (`AEGIS-LICENSE-V1\n` vs `AEGIS-UPDATE-V1\n`), preventing cross-protocol signature reuse.
  - Strict line-oriented parser with zero external serialization dependencies, rejecting unknown critical fields and enforcing strict SemVer monotonic upgrades.

### 3.5 `aegis-server` (Collaborative Server & Persistence)
- **Strengths**:
  - [`Actor`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/actor.rs) guarantees linearizable per-project op application and rolls back in-memory state via inverse ops if `store.append` fails, ensuring memory is never ahead of durable storage.
  - [`store/audit.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/store/audit.rs) chains every project operation with SHA-256 (`AEGIS-AUDIT-V1`) and verifies the chain inside ` PostgresStore::append` (`SELECT ... FOR UPDATE`).
- **Improvements**: Harden `Throttle` (**SEC-01**), add per-project RBAC (**SEC-02**), isolate Argon2id login permits (**SEC-04**), support custom CA files in `DbTls::Verify` (**SEC-05**), protect governance events from `login_failed` eviction (**SEC-06**), and wrap multi-step account updates in SQL transactions (**SEC-07**).

### 3.6 `aegis-canvas` (2D Attack-Tree Layout, SVG, & Tiling)
- **Strengths**:
  - Pure mathematical layout and rendering crate with zero GUI framework dependencies.
  - [`tiles.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-canvas/src/tiles.rs) plans overview + detail panels (`TileLimits`) in `O(n log n)` so arbitrarily large attack trees render legibly on fixed A4 PDF pages without page-count explosion.

### 3.7 `aegis-client` (Headless Client State Machine & Reports)
- **Strengths**:
  - [`Session`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/session.rs) never awaits network calls inline on the actor loop; epoch counters cleanly discard late responses from prior project opens or resyncs.
  - Continuous dual-engine verification (`compare_server_risks`, ADR 0043) compares client-computed risks against server `RisksEvent` broadcasts and fails safe to server numbers on any mismatch.
  - Deterministic HTML ([`report.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/report.rs)) and native PDF ([`pdf.rs`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-client/src/pdf.rs)) generators with CSV formula-injection guards (`csv_field`).

### 3.8 `aegis-shell` (Slint Desktop Application)
- **Strengths**:
  - Thin UI shell (`ui/app.slint`, `controller.rs`, `launcher.rs`, `provider.rs`) strictly isolated by `cargo-deny` (`wrappers = ["aegis-shell"]`).
  - Full headless UI integration test coverage (`i-slint-backend-testing`) exercising login, project creation, attack-tree editing, baselines, and admin workflows without requiring a display server.

---

## 4. Prioritized Remediation Roadmap

### Phase 1 — Immediate Security Hardening (Pre-Production Gate)
1. **Fix `Throttle` Eviction & Pre-Filter Invalid Usernames (SEC-01)**:
   - Reject `normalize_username(raw_username).is_err()` before throttling/hashing, and replace `self.names.clear()` in [`Throttle::record_failure`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/mod.rs#L520-L523) with single-entry oldest-first eviction.
2. **Make `ReplayCache` Expiry Monotonic in Server Time (SEC-03)**:
   - Compute `let expires = now.saturating_add(self.max_age.saturating_add(self.max_future_skew));` in [`ReplayCache::insert`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-auth/src/dpop.rs#L254-L282).
3. **Protect Governance Audit Events from `login_failed` Eviction (SEC-06)**:
   - Separate authentication noise (`login_ok`, `login_failed`, `logout`) from administrative/governance events in [`trim_events`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/postgres.rs#L298-L318) and [`EventLog`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/events.rs#L173-L237).
4. **Wrap Multi-Step Account & Session Mutations in SQL Transactions (SEC-07)**:
   - Execute `put_user` + `delete_sessions` atomically inside a PostgreSQL transaction in [`PostgresAccounts`](file:///Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-server/src/accounts/postgres.rs).

### Phase 2 — Enterprise Multi-Tenancy & Resilience
5. **Project-Level RBAC (`Viewer`, `Editor`, `Reviewer`, `Owner`) (SEC-02)**:
   - Add per-project membership and enforce role permissions on `OpenProject`, `AcquireLock`, `SubmitOps` (specifically `ControlStatus::Verified`), and `CreateBaseline`.
6. **Split Argon2id Semaphores & Add Per-IP Rate Limiting (SEC-04)**:
   - Separate unauthenticated login hashing permits from authenticated password-change permits, and add per-IP rate limiting on `LogIn`.
7. **Support Custom CA Bundles (`AEGIS_DB_CA_FILE`) for Database TLS (SEC-05)** and **PostgreSQL Single-Leader Advisory Lock (SEC-12)**.

### Phase 3 — Defense-in-Depth Polish
8. **Zeroize Secret Material on Drop (SEC-08)**:
   - Add `zeroize` to workspace dependencies and derive `ZeroizeOnDrop` on `DeviceKey`, `PkcePair`, `TokenSet`, and `NewSession`.
9. **Harden HTTP/2 Transport Limits & Local Export File Permissions (SEC-09, SEC-10)**:
   - Configure `max_concurrent_streams`, `concurrency_limit_per_connection`, and keepalive timeouts on `tonic::transport::Server`, and write local exports with mode `0600` on Unix.
