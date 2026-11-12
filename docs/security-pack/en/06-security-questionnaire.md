# 6. Security questionnaire answers

## 6.1 How to use this chapter

These are ready answers to the questions a buyer's security team usually asks before a proof of concept. Copy an answer into the customer's own form, and give the reference as the place to read more.

- The answers describe the product as shipped. Each rests on a section of chapters 1 to 5, or on the file named in the reference column.
- `Yes`: the product does it as built.
- `No`: the product does not do it.
- `Partly`: the answer explains the limit.
- `Customer`: the customer does this in its own environment, because the customer hosts the product.
- `Planned`: decided but not built.
- `Vendor`: a question about the vendor's organisation, not the product. The vendor answers it in the contract papers for each customer.

## 6.2 Hosting and architecture

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-01 | Who hosts the product? | Customer | The customer runs the server container, the PostgreSQL database and the TLS proxy in its own cloud account or data center. The vendor hosts nothing. | 1.1 |
| Q-02 | Is the product multi-tenant? | No | Each company runs its own deployment with its own database. Several projects share one server of that company only. | 1.2 |
| Q-03 | Does our data leave our network? | Partly | By default no project data leaves it. Optional flows are company login at the customer's own identity provider and the update check, which sends no project data. | 1.4, 1.6 |
| Q-04 | Can the vendor reach our data? | No | The vendor receives no data and has no access path through the product. | 1.1, 1.6 |
| Q-05 | Does the product send telemetry or usage data? | No | There is no telemetry, no license server and no vendor-hosted service. A search of the source code found no analytics or error-reporting service. | 1.6 |
| Q-06 | Can it run on a network with no internet? | Yes | Leave the update address empty and install each release from the signed offline bundle. The license is a file checked offline. | 1.6, 6.8 |
| Q-07 | Which outside services does it need while running? | Partly | None by default. With company login it talks to the customer's identity provider. Cloud AI providers are not built. | 1.4, 1.6 |
| Q-08 | Where is our data stored? | Customer | In the customer's PostgreSQL database. In standalone mode it is a project folder on the user's disk. | 1.7 |

## 6.3 Access control

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-09 | Does it support single sign-on? | Yes | Company login uses OpenID Connect with PKCE in the system browser, and tokens are bound to the device. This is the way to run the product in production. | 3.2 |
| Q-10 | Does it support multi-factor login? | Partly | The product has no multi-factor login of its own. With company login the customer's identity provider supplies it. Password login has none. | 3.2, 3.3 |
| Q-11 | What is the password policy? | Yes | For password login: at least 10 characters, at most 256 bytes, not only spaces and not the user name. There are no composition rules. | 3.3 |
| Q-12 | Is there protection against password guessing? | Yes | Password login waits longer after 5 failures per user name, doubling up to 15 minutes, and slows an address after 20 failures. An unknown name takes the same time as a known one. | 3.3 |
| Q-13 | What is the session timeout? | Partly | A password session ends 12 hours after login by default (1 to 720 hours), whatever the use. There is no separate idle timeout. | 3.4 |
| Q-14 | Is access role based? | Yes | Every call is checked against the user's role in the project. There are six built-in roles, and they cannot be edited. | 3.5, 3.8 |
| Q-15 | Is access limited per project? | Yes | Membership is per project. A project with no membership is invisible to users who are not administrators. Group membership is not built. | 3.5 |
| Q-16 | Are administrator duties separated? | Partly | No project role gives server administration. A server administrator has every permission in every project, and the last administrator cannot be removed. | 3.5 |
| Q-17 | How is a leaver removed? | Partly | With password login an administrator disables or deletes the account and its sessions end. With company login the customer removes the person at its identity provider. In both cases membership rows stay until someone removes them. | 3.4, 5.2, 5.4 |
| Q-18 | Are there shared or default accounts? | Partly | There is no built-in default password. The customer sets the first administrator password, and the server uses it only while no user exists. The development login must be off, and the release checklist requires it. | 2.3, 3.2 |

## 6.4 Cryptography

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-19 | Is data encrypted in transit? | Partly | The desktop app uses TLS to the server address. The server itself speaks plain HTTP, so the customer's proxy ends TLS and keeps the plain port off other networks. | 1.5, 2.2 |
| Q-20 | Which TLS versions are used? | Yes | The app and the database connection use TLS 1.2 and 1.3 and check the certificate. TLS to the database is off by default and is switched on with `AEGIS_DB_TLS`. | 2.2 |
| Q-21 | Is data encrypted at rest? | Customer | The product does not encrypt stored data itself. Disk or database encryption is the customer's duty. | 2.5 |
| Q-22 | How are passwords stored? | Yes | Only as Argon2id hashes with a random salt for each password. Session tokens are stored only as SHA-256 hashes. | 2.2, 2.5 |
| Q-23 | How are keys managed? | Partly | The vendor keeps the license and update signing keys offline. The customer keeps the proxy certificate, the database password and the identity provider settings. | 2.3 |
| Q-24 | Does it use algorithms of its own design? | No | Signatures, TLS and random numbers come from open source libraries. The product has no algorithm of its own. | 2.1, 2.6 |
| Q-25 | Is the cryptography FIPS 140 validated? | No | The cryptographic code is not validated, and the pack claims no certification. | 2.6 |

## 6.5 Logging and monitoring

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-26 | Are security events logged? | Yes | The server keeps an event log of logins, logouts, password and user changes, exports, baselines, project deletion and membership changes. Every project change is also kept with who made it. | 4.2, 4.3 |
| Q-27 | What does a log entry contain? | Yes | The time, the actor, the kind, the target and a short detail. It never holds a password or a session token. A failed login keeps the user name as typed. | 4.3 |
| Q-28 | Can the logs be changed? | Partly | Both logs are SHA-256 hash chains, so a change behind the server's back is found. A database administrator who rewrites a whole table can defeat the check unless the customer keeps the head hash elsewhere. | 4.4 |
| Q-29 | How long are logs kept? | Yes | The event log keeps 365 days and 1,000,000 events by default, and both limits can be changed. Project history is never trimmed. | 4.5 |
| Q-30 | Can logs go to our SIEM? | Partly | There is no built-in forwarder. The customer collects the process log from standard output and reads the event table with its own tools. | 4.7 |
| Q-31 | Where do log times come from? | Customer | From the server's own clock in UTC. Keeping that clock right is the customer's duty. | 4.6 |

## 6.6 Secure development

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-32 | Is it written in a memory-safe language? | Yes | The product is written in Rust. | `docs/engineering-standards.md` |
| Q-33 | Is unsafe code used? | Partly | The standard forbids it in the model, engine, method and lint crates. Elsewhere each use needs a safety comment and Miri tests, as the rule states. | `docs/engineering-standards.md` |
| Q-34 | Is code reviewed? | Partly | The standard requires a review checklist for every change, even when one person works alone. This is not an independent audit. | `docs/engineering-standards.md` |
| Q-35 | Are there automated tests? | Yes | Unit tests run on every change that lands on the main branch, and the release checklist requires the database tests to run too. | `README.md`, `docs/release-checklist.md` |
| Q-36 | Are property tests used? | Yes | Property tests are used in several crates, for example for rules that must hold after any change. | `docs/engineering-standards.md` |
| Q-37 | Is fuzzing used? | Partly | Fuzz targets exist for project files, history files and methodology packs. They are not run in the automatic build. | `README.md` |
| Q-38 | Is static analysis used? | Yes | Formatting and lints run on every change, and lint warnings fail the build. | `README.md` |
| Q-39 | Are dependencies checked? | Yes | `cargo-deny` checks licenses, known advisories, banned crates and sources on every pull request and push. | `README.md` |
| Q-40 | Are build tools and dependencies pinned? | Yes | The build pins one Rust version and uses locked dependencies. A release is built from a clean checkout of a tag. | `.github/workflows/ci.yml`, `docs/release-checklist.md` |
| Q-41 | Is there a software bill of materials? | Yes | Every run on the main branch and every release tag makes a CycloneDX list per crate, published next to the release files. | `SECURITY.md` |

## 6.7 Vulnerability management

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-42 | How do we report a vulnerability? | Yes | Privately through GitHub private vulnerability reporting on the repository. Public issues are not used. | `SECURITY.md` |
| Q-43 | How fast does the vendor respond? | Yes | Receipt is confirmed in 3 working days. Acceptance and severity follow in 10 working days. A fix, or an agreed date, comes within 90 days at most. | `SECURITY.md` |
| Q-44 | How long does a release get security updates? | Yes | Five years from release, or until the next major version plus one year, whichever is later. The release notes give the end date. | `SECURITY.md` |
| Q-45 | Has an independent penetration test been done? | Vendor | No record of a penetration test is kept in the repository. The vendor answers this in the contract papers for each customer. | `SECURITY.md` |
| Q-46 | Does the vendor follow Cyber Resilience Act reporting? | Yes | An exploited flaw is reported to the CSIRT and ENISA within 24 hours, 72 hours and 14 days after a fix, as the Act requires. | `SECURITY.md` |

## 6.8 Updates and supply chain

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-47 | How are updates delivered? | Yes | The app checks a signed update manifest only when an update address is set. The user then downloads the installer on request. | 1.4, `docs/deployment.md` |
| Q-48 | Are updates signed? | Yes | The manifest is signed with an update key that differs from the license key, and the downloaded file must match its size and SHA-256. A file that fails is deleted. | 2.2 |
| Q-49 | Can the app update itself silently? | No | The app never runs the installer. The user closes the app and opens the file. | 1.4, `docs/deployment.md` |
| Q-50 | Can we update with no internet? | Yes | A release comes as an offline bundle, which the customer loads with its own tools. | `docs/deployment.md` |
| Q-51 | How do we verify an offline bundle? | Yes | Check the signature of the `SHA256SUMS` file with the update public key, which the vendor gives separately, then check each file against it. | 2.2, `docs/deployment.md` |
| Q-52 | Does the license check call the vendor? | No | The license is a signed file checked offline. A license in its grace period starts with a warning. | 1.6, 2.2 |
| Q-53 | Are the installers code signed? | Planned | Signing of installers for the operating system, such as a Windows installer, is decided but not built. The checksum file and the update manifest are signed. | `docs/requirements.md` |

## 6.9 Privacy

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-54 | Which personal data does the product keep? | Yes | User names, a display name, session facts, the actor of each change and a few settings on the user's computer. Section 5.2 lists all of it. | 5.2 |
| Q-55 | Is personal data sent to the vendor or a third party? | No | Nothing is sent to the vendor or to any third party. Company login contacts the customer's own identity provider. | 5.1, 5.5 |
| Q-56 | Is data moved across borders? | Customer | The product sends no data out of the deployment. Where the server and database run is the customer's choice. | 5.5, 5.6 |
| Q-57 | Can a person's data be deleted? | Partly | An administrator deletes the account and its sessions. The name stays in the project history until the project is deleted and in the event log until retention removes it, because the logs are hash chains. | 5.3, 5.4 |
| Q-58 | Do we need a data processing agreement? | Vendor | Only if the customer hands support data to the vendor. The vendor answers this in the contract papers for each customer. | 5.1, 5.5 |

## 6.10 Availability and recovery

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-59 | Who backs up the data? | Customer | The customer backs up the PostgreSQL database, the license file and the server configuration. | 1.7 |
| Q-60 | How do we check a restore? | Customer | After a restore a server administrator runs the history chain check in the app and compares the head hash with the copy kept elsewhere. | 4.4 |
| Q-61 | Is there high availability? | No | There is one server instance per database. The customer's platform can restart it. | 1.8 |
| Q-62 | What happens when the license expires? | Yes | The license sets a grace period of up to 90 days, and the server starts with a warning. After that reading and listing still work. Only taking the edit lock is refused. | `docs/adr/0032-license-tokens.md` |
| Q-63 | Can we take our data out if we leave? | Partly | The data is in the customer's own database. Users export reports and tables to files. No single full export of a project is promised here. | 1.7, 5.4, `docs/architecture.md` |

## 6.11 Vendor organisation

| ID | Question | Status | Answer | Reference |
|---|---|---|---|---|
| Q-64 | Does the vendor hold security certifications such as ISO/IEC 27001 or TISAX? | Vendor | Answered by the vendor in the contract papers for each customer; not a property of the product. | 5.1 |
| Q-65 | Does the vendor have an information security policy? | Vendor | Answered by the vendor in the contract papers for each customer; not a property of the product. | 5.1 |
| Q-66 | Are vendor staff screened and trained? | Vendor | Answered by the vendor in the contract papers for each customer; not a property of the product. | 5.1 |
| Q-67 | Can vendor staff reach customer data? | Vendor | Answered by the vendor in the contract papers for each customer; not a property of the product. Through the product there is no path, see 1.6. | 1.6 |
| Q-68 | Does the vendor use subcontractors? | Vendor | Answered by the vendor in the contract papers for each customer; not a property of the product. | 5.1 |
| Q-69 | How does the vendor tell customers about an incident? | Vendor | Answered by the vendor in the contract papers for each customer; not a property of the product. | `SECURITY.md` |
| Q-70 | Does the vendor carry insurance? | Vendor | Answered by the vendor in the contract papers for each customer; not a property of the product. | 5.1 |
| Q-71 | Does the vendor have a business continuity plan? | Vendor | Answered by the vendor in the contract papers for each customer; not a property of the product. | 5.1 |

## 6.12 Sources

- `docs/security-pack/en/01-architecture-and-data-flow.md`
- `docs/security-pack/en/02-cryptography.md`
- `docs/security-pack/en/03-authentication-and-access.md`
- `docs/security-pack/en/04-logging-and-retention.md`
- `docs/security-pack/en/05-personal-data.md`
- `SECURITY.md`
- `README.md`
- `.github/workflows/ci.yml`
- `docs/release-checklist.md`
- `docs/deployment.md`
- `docs/engineering-standards.md`
- `docs/requirements.md`
- `docs/architecture.md`
- `docs/adr/0032-license-tokens.md`
