# 2. Cryptography

## 2.1 Summary

The product uses standard algorithms from open source libraries. TLS comes
from rustls, and signatures and random numbers come from ring. Password
hashing uses Argon2id, and hash chains use SHA-256, both from well-known
open source crates. The product has no algorithm of its own. TLS in the
desktop app and towards PostgreSQL uses versions 1.2 and 1.3 and the
operating system's trust store. The product does not encrypt data at rest
itself; the customer's disk or database encryption does that (`Customer`).
The cryptographic code is not FIPS 140 validated, and the pack claims no
certification.

## 2.2 Inventory

| ID | Purpose | Algorithm and parameters | Library | Where the key or secret lives |
|---|---|---|---|---|
| CR-01 | TLS from the desktop app to the server address | TLS 1.2 and 1.3. The server certificate is checked against the operating system trust store. | rustls with the ring provider | No key in the app. The certificate and its key are at the `Customer` proxy. |
| CR-02 | TLS at the server side | The server itself speaks plain HTTP. The `Customer` proxy ends TLS. | The customer's proxy | `Customer`: the certificate and key at the proxy. |
| CR-03 | TLS to PostgreSQL | TLS 1.2 and 1.3 with `AEGIS_DB_TLS` set to `encrypt` or `verify`. Both check the certificate chain and the host name, against the operating system trust store or against the file in `AEGIS_DB_CA_FILE` (at most 1 MiB). Off by default. | rustls with ring | `Customer`: the database certificate. The database password is in the server's configuration. |
| CR-04 | Password hashing | Argon2id, 19 MiB memory, 2 passes, 1 lane, a random salt per password. | The `argon2` crate | Only the hash is stored, in PostgreSQL or the account store. |
| CR-05 | Session tokens | `ats_` plus 32 random bytes. Only the SHA-256 of the token is stored. | ring (random numbers) and the `sha2` crate (SHA-256) | The token is on the user's computer, in memory or, with "keep me logged in", in the operating system keychain. The server keeps only the hash. |
| CR-06 | Check of company login tokens | Signed tokens (JWS), only ES256 and RS256. RSA keys of 2048 to 8192 bits. `none` and HMAC are refused. | ring | Public keys fetched from the customer's identity provider and cached for 10 minutes. The server holds no private key. |
| CR-07 | PKCE for company login | Method S256 only: SHA-256 of a random verifier. | ring | The verifier is in memory during the login and is wiped after. |
| CR-08 | DPoP proofs | ES256 (P-256). The proof covers the request method, address and token hash, and is valid for about one minute. | ring | A device key per computer, kept in the operating system keychain. It is kept in memory only if the machine has no keychain. |
| CR-09 | License files | Ed25519 over a prefix and the license text. | ring | Public keys are compiled into the build. The private key is the vendor's, offline. |
| CR-10 | Update manifests | Ed25519, with a different key from the license key. | ring | Public keys compiled into the build. The private key is the vendor's, offline. |
| CR-11 | Offline bundle | The `SHA256SUMS` file is signed with Ed25519 by the update key. Each file is listed with its SHA-256. | ring | Public update keys compiled into the build. |
| CR-12 | Installer download check | Size and SHA-256 of the downloaded file must match the signed manifest. | ring | None. The expected values come from the signed manifest. |
| CR-13 | Project operation hash chain | SHA-256 over each operation and the hash of the one before it. | The `sha2` crate | None. Anyone with read access can recompute it. |
| CR-14 | Event log hash chain | SHA-256 over each server event and the hash of the one before it. | The `sha2` crate | None. The server can verify the chain on request. |
| CR-15 | Figure digests | SHA-256 of each attached figure, stored with it. | The `sha2` crate | None. |
| CR-16 | Operating system keychain | The platform's own protection: Windows Credential Manager, macOS Keychain, Linux Secret Service. | The operating system | The operating system, under the user's account. |

## 2.3 Key management

Vendor keys:

- The license private key and the update private key are kept offline. They
  are never in a repository or in a CI secret that builds pull requests.
- The update key is a different key from the license key and is held by
  fewer people.
- A build accepts only the public keys compiled into it. A build with no key
  accepts no license. A license names its key, so a key can be replaced
  without breaking older licenses.

Customer keys and secrets (`Customer`):

- The TLS certificate and its private key at the proxy.
- The database password and, for a private authority, the CA file.
- The identity provider's client registration for company login.

Secrets in the server's configuration: the first administrator password is
used only while the user table is empty. After that the server ignores it.
Secrets that the product holds in memory (passwords, PKCE verifiers, client
secrets) are overwritten when they are no longer needed.

## 2.4 Random numbers

Session tokens, salts and the DPoP device key use the operating system's
random generator, through ring. The product has no random generator of its
own.

## 2.5 Data at rest

- The product stores projects in PostgreSQL or in a project folder. It does
  not encrypt them itself. Disk or database encryption is `Customer`.
- Passwords are stored only as Argon2id hashes.
- Session tokens are stored only as SHA-256 hashes.
- A search of the source code for AES, ChaCha and "encrypt" found no code
  that encrypts stored data. The only hits are text about TLS settings and a
  comment about RSA keys.

## 2.6 What is not used

- No home-made algorithm. Signatures, TLS and random numbers come from ring
  and rustls.
- No MD5 or SHA-1 in the product's own code. A search of the source code
  found none. The PostgreSQL client library carries code for the old MD5
  database password method. The product does not use it for its own
  purposes. `Customer` should set the database to SCRAM password login.
- The cryptographic module is not FIPS 140 validated.

## 2.7 Sources

- `Cargo.toml`
- `docs/adr/0028-login.md`
- `docs/adr/0032-license-tokens.md`
- `docs/adr/0054-security-hardening-sweep.md`
- `docs/release-checklist.md`
- `docs/deployment.md`
- `crates/aegis-auth/src/tls.rs`
- `crates/aegis-auth/src/jose.rs`
- `crates/aegis-auth/src/dpop.rs`
- `crates/aegis-auth/src/pkce.rs`
- `crates/aegis-auth/src/store.rs`
- `crates/aegis-license/src/lib.rs`
- `crates/aegis-license/src/bundle.rs`
- `crates/aegis-client/src/updates.rs`
- `crates/aegis-server/src/accounts/mod.rs`
- `crates/aegis-server/src/accounts/password.rs`
- `crates/aegis-server/src/store/postgres.rs`
- `crates/aegis-server/src/store/audit.rs`
- `crates/aegis-server/src/store/blob.rs`
