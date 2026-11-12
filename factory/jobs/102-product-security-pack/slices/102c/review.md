VERDICT: PASS
## For the owner
What changed: two new chapters in the buyer security pack. Chapter 4 covers which logs exist, what they hold, how long they are kept and the hash chain. Chapter 5 lists every piece of personal data the product keeps, with GDPR and APPI tables.
Chapter 3 now says project membership is by user name only. Membership by a company-login group is marked `Planned`.
No code changed. Rework 2 fixed the earlier finding: the chapters no longer say the event log "never holds a password". They now say that a failed login records the typed user name exactly as typed.
Look at: docs/security-pack/en/04-logging-and-retention.md:41-47. This is the corrected password wording. A failed login keeps the raw typed name, so a password typed into the name box by mistake stays in the log until it is trimmed. It is accurate, and a buyer will notice it.
docs/security-pack/en/04-logging-and-retention.md:75-81. This says plainly that the chain head is stored in the same database, so a database administrator can rewrite the log unless the customer copies the head somewhere else.
docs/security-pack/en/05-personal-data.md:53-72. This says a deleted user's name stays in project history, in the event log and in membership rows. It is the main limit a privacy team will ask about.
Risk: low. These are buyer-facing claims, so I checked them against the code:
- 15 event kinds, with the stored spellings (accounts/mod.rs:140-244).
- Text is cut to 256 characters, not bytes, with control characters removed (accounts/mod.rs:523).
- Sessions last 12 hours, with at most 10 per user (accounts/mod.rs:564-565).
- The membership events carry the project ID as the target and the member as the detail (service.rs:1140-1198).
- Company login fills no groups (auth_layer.rs:79).
The GDPR article numbers are all ones the prompt named. No APPI article numbers are given.
View: git -C factory/worktrees/102-product-security-pack diff 22c2e58814adac6be3a79224ced20ef7f051cf68..HEAD
## Findings
- none
## Lesson
[reviewer] When a doc says a log "never holds a password", check what a failed login records: the raw typed user name can be a mistyped password.
NEXT: human
