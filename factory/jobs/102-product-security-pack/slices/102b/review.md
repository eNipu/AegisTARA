VERDICT: PASS
## For the owner
What changed: two new buyer-facing chapters, 2 (cryptography, 16-row inventory) and 3 (authentication and access).
The wrong "encrypt does not check the database certificate" text is gone from the deployment guide and the postgres.rs module doc.
In postgres.rs only `//!` lines changed. No code changed.
Look at: docs/security-pack/en/03-authentication-and-access.md:80 and :116-117. The code gives `methodology_owner` read access only, the same as `viewer`. The chapter now says this to buyers. Decide whether that is what you want, or whether the role should be fixed in code before the pack ships.
docs/security-pack/en/02-cryptography.md:79-82 says the PostgreSQL client library contains MD5 code for the old database password method and tells the customer to use SCRAM. This is accurate (md-5 comes in through postgres-protocol), but it puts "MD5" in front of a buyer. Decide whether to keep the wording.
docs/security-pack/en/03-authentication-and-access.md:97-100: the lock renewal and "an administrator can break a lock" come from architecture.md. The builder did not check them in the service code (build.md says so).
Risk: low. I checked these claims against the code: the database TLS (`rustls_config` uses the same verifying config for Encrypt and Verify, CA file capped at 1 MiB); the role table (matches `role_allows`); the throttle (5 free failures, then 1 s doubling to 15 min, 2 in flight per address, 20 address failures, 5 s hashing wait); 10 sessions; `AEGIS_SESSION_HOURS` 1 to 720; TLS 1.2 and 1.3 (`with_safe_default_protocol_versions`); RSA 2048 to 8192; JWKS cache 600 s; DPoP 60 s window; random numbers from `SystemRandom`; secrets wiped (zeroize in pkce, oidc, dpop, accounts); standalone binds 127.0.0.1:0; dev login bypasses access (ADR 0061:53).
View: git -C factory/worktrees/102-product-security-pack diff 91a35b38013631f8359af1875a59d51185ca102f..HEAD
## Findings
- none blocking.
## Lesson
[planner] When a prompt asks for a doc to be built from a code table (roles, permissions), say in the prompt that a surprising result (for example a role with fewer rights than its name suggests) must go to the owner as a decision, not just be written down.
NEXT: human
