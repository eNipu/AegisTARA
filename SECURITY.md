# Security policy

AegisTARA holds security analyses of vehicles and their parts. A flaw in it
can leak a customer's threat model, so we treat reports about it seriously.

## Reporting a vulnerability

Report it privately through GitHub: **Security → Report a vulnerability** on
this repository (private vulnerability reporting). Do not open a public issue.

Please include:

- the version and the engine version (the status bar shows both while connected),
- what an attacker can do and what they need first (a login, a role, network
  access to the server, a crafted project or import file),
- the steps to reproduce it, or a file that does.

## What happens next

| When | What we do |
|---|---|
| 3 working days | We confirm we received the report. |
| 10 working days | We tell you whether we accept it, and how severe we think it is. |
| 90 days at most | We release a fix, or agree a later date with you. |

We credit you in the release notes unless you ask us not to.

If the flaw is being exploited, we also report it to the CSIRT and ENISA as
the EU Cyber Resilience Act requires (Article 14): an early warning within
24 hours of learning of it, a notification within 72 hours, and a final
report within 14 days after a fix is available.

## Supported versions

Each release receives security updates for **five years** from its release
date, or until the next major version plus one year, whichever is later. The
release notes state the end date of every release.

Security fixes ship as signed updates (the update manifest is signed with the
update key, see `docs/release-checklist.md`). Air gapped customers receive
the same signed package by hand.

## Software bill of materials

Every CI run on `master` and every release tag produces a CycloneDX SBOM per
crate (`sbom` job in `.github/workflows/ci.yml`). The SBOM of a release is
published next to its artifacts.

## Scope

In scope: the desktop app (`aegis-shell`), the server (`aegis-server`), the
licence and update checks (`aegis-license`), the project file format and its
importers, and the server image in `deploy/`.

Out of scope: findings that need a compromised operating system account, and
denial of service by a logged in administrator.

## Deployment and standalone trust boundary

- Shipped `aegis-server` binaries and customer container images are built without
  `dev-auth` or `dev-license`: they refuse `AEGIS_AUTH=dev`,
  `AEGIS_AUTH=development`, and `AEGIS_DEV_AUTH=1` at startup.
- When `dev-auth` is explicitly compiled into a local development build,
  `aegis-server` refuses to bind to non-loopback addresses.
- Licensed standalone mode in the desktop app embeds a loopback-only
  (`127.0.0.1`) server on the user's workstation. It relies on the
  trusted-local-machine assumption (another process running under a compromised
  OS account on the same host is outside the threat model).
