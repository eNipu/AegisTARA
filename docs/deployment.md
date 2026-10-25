# Deploying the server

This page is the short path from a license file to a running server. The
design is in `docs/adr/0034-release-scaffolding.md`.

## What you need

- A container runtime with Compose.
- The signed license file from the vendor (`aegis-license` makes it).
- An OIDC identity provider and a client for AegisTARA.
- A TLS terminating proxy in front of the server. The server speaks plain
  HTTP/2 and is bound to the host's loopback by default.

## Steps

```sh
cp deploy/.env.example deploy/.env       # fill in the passwords and OIDC values
cp /path/to/license.aegis deploy/license.aegis
docker compose -f deploy/compose.yaml --env-file deploy/.env up --build
```

The database is on an internal network that only the server can reach. The
server stops at start with a clear message if the license is missing, wrong,
or over its grace period.

## Known limits of this build

These are open backlog jobs, not surprises:

- **The server cannot reach an `https://` identity provider yet** (backlog
  038). Login needs the provider's discovery and key set, and the HTTP client
  in `aegis-auth` refuses `https://` and cleartext to other hosts on purpose.
  Until 038 is done, run the identity provider where the server reaches it
  over a local `http://` address, or keep this build for evaluation only.
- **The server talks to PostgreSQL without TLS** (backlog 037). The compose
  file keeps the database on an internal network for that reason.
- **The seat count restarts with the server** (backlog 039).
- **The image was written but not built in the authoring environment**, which
  had no container daemon. Build it once and run `docs/release-checklist.md`
  before relying on it.

## Updates

Clients check a signed update manifest (`AEGIS-UPDATE-1...`). It is signed with
a key that is different from the license key. Fetching the manifest over the
network needs the HTTPS client (038); until then it can be loaded from a
file or a share. A client only offers a release that is newer than the one it
runs, and checks the installer's size and SHA-256 before it is used.
