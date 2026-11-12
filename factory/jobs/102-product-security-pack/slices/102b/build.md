Commit: 22c2e58 Add cryptography and authentication chapters, fix database TLS text (102b)
Files: docs/security-pack/en/02-cryptography.md, docs/security-pack/en/03-authentication-and-access.md, docs/deployment.md, crates/aegis-server/src/store/postgres.rs (`//!` lines only)
Done when:
1. `sh scripts/check-security-pack.sh` -> `security pack ok (3 files)`
2. both stale-text greps print nothing
3. diff of postgres.rs minus `//!` lines prints nothing
4. `grep -c '^| CR-'` -> 16
5. `grep -c '^## 3\.'` -> 9
Also: `cargo fmt --all` and `scripts/ci.sh crates/aegis-server` passed.
Notes:
- Step 2.5 grep (aes|chacha|encrypt in crates/*/src): 3 files, all harmless: jose.rs (comment "key meant for encryption"), main.rs (AEGIS_DB_TLS doc), postgres.rs (DbTls::Encrypt TLS). No at-rest encryption in code. Cargo.lock has transitive `aes` and `openssl` (only via dbus-secret-service, Linux keychain) and `chacha20` (via rand); the product does not call them.
- Step 2.6 grep (md5|sha1|sha-1 in crates/*/src and Cargo.toml): none. Cargo.lock has `md-5` via `postgres-protocol` (old MD5 database password method); chapter says so and advises SCRAM (Customer). No `sha1` in the lock.
- Code differs from prompt:
  * `methodology_owner` grants Read only in `role_allows` (not Write/Manage). Table and 3.8 say so. Worth the owner's attention: probably surprising.
  * Argon2id and SHA-256 chain hashes come from RustCrypto crates `argon2` and `sha2`, not ring. Summary says "rustls and ring plus Argon2id and SHA-256 from open source crates", not "two libraries only".
  * Throttle per address (2 concurrent, 20 free failures) and the 5 s hashing wait are in 3.3 beyond the prompt, from AccountsConfig defaults.
  * No SCIM found in code; stated in 3.8.
  * Device key (DPoP): PKCS#8 in the OS keychain (store.rs), memory fallback without a keychain (matches chapter 1).
- Not independently re-verified beyond docs: "A project with no membership rows is invisible to non-admins" (ADR 0061, written 2026-10-30), and lock "renewed while the editor works" and "administrator can break a lock" (architecture.md line 1290/1722, not read in service code).
NEXT: reviewer
