# 5. Personal data handling (GDPR and APPI)

## 5.1 Scope and roles

This chapter says what the product stores and does with personal data. It is
not legal advice.

- The customer runs the server and decides who uses it. The customer is the
  controller under the GDPR and the business operator handling personal
  information under Japan's Act on the Protection of Personal Information
  (APPI).
- The vendor receives no personal data through the product. Nothing is sent to
  the vendor, there is no telemetry and there is no license server (chapter 1).
- The vendor would handle personal data only if the customer hands some over
  for support. That is covered by the contract, not by the product.
- The product is made to analyse vehicles and parts, not people. Project
  content should hold no personal data. Free-text fields hold whatever a user
  types, so keeping personal data out of them is `Customer`.
- The personal data the product itself keeps is user names, a few related
  facts (times, a computer name, a network address held in memory) and the
  name of whoever made each change. Section 5.2 lists all of it.

## 5.2 Inventory

| ID | Data | Where stored | Purpose | Kept how long | How removed |
|---|---|---|---|---|---|
| PD-01 | Account: user name, display name, administrator flag, disabled flag, created time, last login time. Password login only | Database table `aegis_user` | Let the person log in; show who they are | Until an administrator deletes the account | Administrator deletes the account in the app |
| PD-02 | Password: an Argon2id hash only, never the password | Database table `aegis_user` | Check the password at login | With the account | Goes with the account |
| PD-03 | Session: SHA-256 of the session token, user name, created and expiry time, client name. The client name is what the desktop app sends: `AegisTARA desktop on` plus the computer name of the person's machine, or `AegisTARA desktop` when no name is available | Database table `aegis_session` | Keep the person logged in; let an administrator see and end sessions | Until it expires (12 hours by default). At most 10 per user | Expiry, logout, administrator ends it, disabling, password change, deleting the account |
| PD-04 | Event log: actor, target user name, time, kind, detail. The detail of a login is the client name of PD-03 | Database table `aegis_event` | Security and administration trail (chapter 4) | 365 days and 1,000,000 events by default | The retention removes the oldest events (5.3). One event cannot be removed alone |
| PD-05 | Project operation history: the actor and the time of every change | Database table `aegis_project_op` | Show who changed what; tamper evidence | As long as the project | Deleting the project. One entry cannot be changed alone (5.3) |
| PD-06 | Baselines and figures: author (`created_by`) and created time | Database tables `aegis_baseline` and `aegis_figure` | Show who froze a version or added a figure | As long as the project | Deleting the project |
| PD-07 | Project membership: the user (`user:` plus the user name or the subject) or a group, and the role | Database table `aegis_project_member` | Decide who may read or edit a project | Until a project owner removes it, or the project is deleted | Project owner or administrator removes the membership. Deleting an account does not remove its membership rows |
| PD-08 | Company login identity: the subject of the token, and whether it names an administrator. Nothing else from the token is stored. Group names are not read from the token in this release | Memory for the length of a call. The subject is then written as the actor in PD-04, PD-05 and PD-06, and in PD-07 | Authorise each call; name the actor | The subject stays wherever it was written | As PD-04 to PD-07 |
| PD-09 | Network address of a login: held in memory only, to slow a flood of failed logins. Never written to the database, the event log or the process log | Server memory, at most 10,000 addresses | Protect the server from a flood of logins (chapter 3) | Until the server restarts or the entry is replaced | Restart of the server |
| PD-10 | Desktop settings file: the server address, the last user name typed at the password login, and saved report profiles, which can hold approver names and revision entries that the user types | A file in the user's own settings folder, readable only by that user | Fill the login form again; fill the report title page | Until the user deletes the file or the app is removed | The user deletes the profile or the file |
| PD-11 | Operating system keychain: with password login, the session token, when "keep me logged in" is ticked; the entry is named after the server and the user name. With company login, the device key and the refresh token from the identity provider. Never the password | The keychain of the user's computer | Keep the person logged in | Until logout (an expired password-login token stays until the next logout) | Logout, or the user clears the keychain |
| PD-12 | Server process log: no user names (except a server built without an event log, tests only, see 4.2), no network addresses, no passwords or tokens. Project IDs and error text only (chapter 4, LG-03) | Standard output, collected by the customer's platform | Run and repair the server | `Customer` sets it | `Customer` |
| PD-13 | Standalone project folder: the actor of each change, which is the fixed name `standalone`, not a person | Files in the project folder on the user's computer | Operation history of a local project | As long as the folder exists | The user deletes the folder |
| PD-14 | AI audit log. `Planned`: not built, and no AI feature ships in this release. When built it will record the user, the feature, the provider, the model, the context references, the response, and whether it was accepted or rejected. Its retention will be stated here | Not applicable | Not applicable | Not applicable | Not applicable |

## 5.3 Retention and deletion

| Data | How long | What removes it |
|---|---|---|
| Account, password, sessions | Until deleted | The administrator deletes the account. The sessions go with it |
| Sessions | Until they expire | Expiry, logout, or an administrator |
| Event log | The retention (4.5) | The hourly trim, oldest first |
| Operation history, baselines, figures | As long as the project | Deleting the project |
| Membership | Until removed | A project owner or administrator |
| Desktop settings and keychain | On the user's computer | The user |

The limit, stated plainly: the operation history and the event log are hash
chains (chapter 4). A user name inside them cannot be changed or removed one
entry at a time without breaking the chain. So:

- A deleted account's name stays in the operation history of every project
  that person edited, until the project is deleted.
- It stays in the event log until the retention removes those events.
- Membership rows with the name stay until someone removes them.

What a customer can do:

- Use company login. The stored identifier is then the subject that the
  identity provider puts in the token. The product does not choose its form.
  Many providers use an opaque ID, some use an e-mail address, so `Customer`
  checks what its provider sends. With an opaque ID, the stored identifier is
  not a personal name.
- With password login, use role or pseudonymous user names instead of personal
  names. The display name can still show a real name, and it is not kept in
  the history, only in the account.
- Set `AEGIS_EVENT_RETENTION_DAYS` to the customer's own policy.

## 5.4 Rights of the person

How a customer answers a request, with the product as built.

| Right | How |
|---|---|
| Access | An administrator sees the account, the sessions and the events in the Administration pages. The history of a project shows who changed what. The customer can also read the tables in its own database |
| Correction | An administrator changes the display name. The user name cannot be changed after it is created: create a new account instead |
| Deletion | An administrator deletes the account, and its sessions end. The limit in 5.3 applies to the history and the event log |
| Restriction | An administrator disables the account. Its sessions end and it cannot log in |
| Portability | Not a product function. The data is in the customer's own database |
| Objection | Not a product function. The customer decides who has an account |

For company login there is no account in the product: the customer corrects or
removes the person at its identity provider, and removes the membership rows
in the product.

## 5.5 GDPR mapping

| Article | How the product supports the customer | Status |
|---|---|---|
| 5 (minimisation, storage limitation) | The product keeps few personal facts (5.2). The event log has a set retention. The history is kept for the life of the project | `Customer` decides the retention and what goes into free text |
| 25 (by design and by default) | No telemetry, no vendor server, no network address stored, hashed passwords and session tokens, log text bounded | `Customer` sets the retention and the login method |
| 30 (records of processing) | The inventory in 5.2 is the input | `Customer` keeps the record |
| 32 (security) | See chapters 2, 3 and 4: encryption, login, roles, the hash-chained logs | `Customer` runs the deployment, the database and the backups |
| 15 to 17 (access, correction, deletion) | See 5.4 and the limit in 5.3 | `Customer` answers the person |
| 33 and 34 (breach) | The event log and the project history show what happened and when. The vendor's handling of reported vulnerabilities is in `SECURITY.md` | `Customer` decides on and makes any notice |
| 44 (transfers) | The product sends no data outside the customer's deployment | `Customer` chooses where the server and database run |
| 28 (processor) | The vendor is not a processor, unless the customer hands over support data | `Customer` signs a contract if it does |

## 5.6 APPI mapping

Described by duty, not by legal text.

| Duty | How the product supports the customer | Status |
|---|---|---|
| Specify the purpose of use | The purposes are in the inventory (5.2) | `Customer` states them to the person |
| Security control measures for personal data | See chapters 2, 3 and 4 | `Customer` runs the deployment |
| Supervision of employees and of entrusted parties | Roles and the event log show who did what (chapters 3 and 4) | `Customer` |
| Limits on provision to a third party | The product provides nothing to any third party | `Customer` for what it exports itself |
| Provision to a third party in a foreign country | The product sends no data out. Where the server runs is the customer's choice | `Customer` |
| Requests from the person: disclosure, correction, stopping use | See 5.4 | `Customer` answers the person |
| Reporting a leak | The event log helps find what happened. The vendor's handling of reported vulnerabilities is in `SECURITY.md` | `Customer` reports to the authority and the person |

## 5.7 Sources

- `SECURITY.md`
- `docs/architecture.md`
- `docs/requirements.md`
- `docs/adr/0046-event-log-everywhere.md`
- `docs/adr/0054-security-hardening-sweep.md`
- `crates/aegis-server/migrations/002_accounts_baselines.sql`
- `crates/aegis-server/migrations/006_project_access.sql`
- `crates/aegis-server/src/auth.rs`
- `crates/aegis-server/src/auth_layer.rs`
- `crates/aegis-server/src/access.rs`
- `crates/aegis-server/src/accounts/mod.rs`
- `crates/aegis-server/src/accounts/events.rs`
- `crates/aegis-server/src/accounts/service.rs`
- `crates/aegis-server/src/store/audit.rs`
- `crates/aegis-client/src/settings.rs`
- `crates/aegis-client/src/report/profile.rs`
- `crates/aegis-shell/src/launcher.rs`
- `crates/aegis-shell/src/standalone.rs`
