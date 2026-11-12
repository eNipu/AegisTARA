Commit: 2953346 Add the logging and retention and personal data chapters to the security pack (102c)
Files: docs/security-pack/en/04-logging-and-retention.md (124 lines), docs/security-pack/en/05-personal-data.md (136 lines)

Done when:
1. `sh scripts/check-security-pack.sh` -> `security pack ok (5 files)`. Also `cargo fmt --all` and `scripts/ci.sh crates/aegis-license` passed.
2. AuditKind variants = 15 (`accounts/mod.rs` 140-173, `ALL: [AuditKind; 15]` line 178); rows of the 4.3 table = 15. Equal.
3. `grep -c '^| PD-'` -> 14; PD-14 is the AI audit log, marked `Planned`.
4. `grep -n -i compliant` on chapter 5 prints nothing.
5. `grep -c '^## 4\.'` -> 8; `grep -c '^## 5\.'` -> 7.

Notes: facts collected (step 1)
- Process log (`tracing::` in aegis-server, ~40 hits): no password, token or network address anywhere. Project ID in error lines (`registry.rs:414`, `actor.rs`, `import.rs:147`); license customer name `main.rs:307`; project NAME only in `import-folder` (`main.rs:428-433`). No user name, except `service.rs:160-166`: with no event log (tests/embedding), the event (actor, target) goes to the process log. `auth_layer.rs:236` logs the auth error text, no token. `accounts/mod.rs:947` logs "created the first administrator" without a name.
- Network address: only in memory. `AddrThrottle` is a `HashMap<IpAddr, ..>` (`accounts/mod.rs:656-700`, capacity 10,000); `service.rs:214` takes `remote_addr().ip()`; audit calls at 1048-1096 pass the typed name, never the address. Not written to the database, event log or log.
- Company login writes no `login_ok`: `LoginOk` is written only at `accounts/mod.rs:1170` (password path); ADR 0046 decision 1 agrees.
- `delete_user` (`accounts/mod.rs:1571-1603`, `postgres.rs:178-189`): deletes the user row; sessions go by ON DELETE CASCADE; writes `user_deleted`. It does NOT touch the event log, `aegis_project_op.actor`, `aegis_baseline.created_by`, `aegis_figure.created_by`, or `aegis_project_member` rows.
- Desktop: no log file. `aegis-shell/src/main.rs:43` sets `tracing_subscriber::fmt()` (stderr). Client/shell warn lines only.
- Standalone folder: `ops.meta` holds the actor (`architecture.md` 1634-1642). The actor is the fixed dev user `standalone` (`launcher.rs:416`), not a person.
- Client name = `AegisTARA desktop on <COMPUTERNAME|HOSTNAME>` or `AegisTARA desktop` (`aegis-shell/src/launcher.rs:115-123`); stored in the session and written as the `login_ok` detail (`accounts/mod.rs:1170`) and `session_revoked` detail (1666).
- `update_user` (`accounts/mod.rs:1480+`) can change display name, admin, disabled, password; the user name cannot change.
- Chain: head hash is stored in table `aegis_event_chain` (`migrations/004_event_chain.sql`), same database; `store/audit.rs` header says to export the head to a system the DB admin cannot write. So the chapter says the head is not anchored elsewhere. Admin verification: `VerifyAudit` (`service.rs:542`, client command `session.rs:2354`).

Differences between prompt/ADR/architecture and the code
- OIDC groups: the prompt says group names are taken from the token on each call. In code `AccessClaims` (`aegis-auth/src/validate.rs:72`) has only subject, admin, jkt, expires, and `OidcAuthn` sets `groups: Vec::new()` (`auth_layer.rs:79`). Group matching exists in `access.rs`, but nothing fills the groups. PD-08 says group names are not read from the token in this release. Chapter 3 (written earlier) says membership works "for a group from the identity provider"; the reviewer or planner may want to reconcile it.
- ADR 0046 "Kept for later" lists an event hash chain; ADR 0054 and the code build it. The chapter follows the code.
- ADR 0054 names a `server_events` table; the code table is `aegis_event` (+ `aegis_event_chain`). Chapter uses `aegis_event`.
- Prompt says the standalone actor is a name held in `ops.meta`; the code's actor is the fixed `standalone`.
- A `login_failed` event's actor is the typed text, which can be a mistyped password; mentioned only as "the user name that was typed".

Article numbers: GDPR 5, 25, 30, 32, 15 to 17, 33, 34, 44, 28, all named in the prompt (I am sure of them: principles, by design, records, security, access/rectification/erasure, breach to authority and to person, transfers, processor). APPI: no article numbers given.

Reviewer notes: no archive export or forwarder is in requirements.md (grepped), so both are "not built", not `Planned`. AI audit log `Planned` per AI-05. Subject format ("opaque ID or e-mail") is stated as the identity provider's choice, not promised. 4.4 says the integrity head is not anchored outside the database.
NEXT: reviewer

## Rework 1
Fixed PD-10 (report profiles, profile.rs in 5.7), PD-11 (company-login keychain items, entry naming, "until logout"), PD-12 (exception for a server without event log). Per the orchestrator note, chapter 3 now says membership is by user name and group membership is `Planned` (no groups filled, auth_layer.rs:79); the closing line about group claims was trimmed. PD-08 unchanged. `sh scripts/check-security-pack.sh` passes.
NEXT: reviewer

## Rework 2
Fixed the "never holds a password" claim: 4.1 and 4.3 now say the event log never stores the password field or a token, and 4.3 and PD-04 add that a failed login records the typed user name as typed, so a mistyped password in the name box is kept until retention removes it. LG-01 made no password claim, so it is unchanged. `sh scripts/check-security-pack.sh` passes.
NEXT: reviewer
