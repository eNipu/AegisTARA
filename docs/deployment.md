# Deploying the server

This page is the short path from a license file to a running server. The
design is in `docs/adr/0034-release-scaffolding.md`.

## What you need

- A container runtime with Compose.
- The signed license file from the vendor (`aegis-license` makes it).
- A login method: an OIDC identity provider and a client for AegisTARA
  (`AEGIS_AUTH=oidc`), or password login for tests and trials
  (`AEGIS_AUTH=password` with `AEGIS_ADMIN_PASSWORD`, ADR 0035). To start
  with both and let an administrator turn the password form off later in
  the app (Administration, "Login"), use `AEGIS_AUTH=both` with the OIDC
  settings and `AEGIS_ADMIN_PASSWORD` (ADR 0039). If the company login
  ever breaks, start once with `AEGIS_PASSWORD_LOGIN=on` to get back in.
- A TLS terminating proxy in front of the server. The server speaks plain
  HTTP/2 and HTTP/1.1 (gRPC-Web) and is bound to the host's loopback by
  default. Proxies that forward only HTTP/1.1 work with gRPC-Web
  (ADR 0036). `GET /healthz` is the health probe.

## Steps

```sh
cp deploy/.env.example deploy/.env       # fill in the passwords and OIDC values
cp /path/to/license.aegis deploy/license.aegis
docker compose -f deploy/compose.yaml --env-file deploy/.env up --build
```

The database is on an internal network that only the server can reach. The
server stops at start with a clear message if the license is missing, wrong,
or over its grace period.

## Offline install (air-gapped sites)

The offline bundle is one tar file, `aegistara-<release>-offline.tar`, with
its `aegistara-<release>-offline.tar.sha256`. Unpacking it gives one folder,
`aegistara-<release>-offline/`, with:

- `SHA256SUMS` and `SHA256SUMS.sig`: the file hashes and their signature.
- `update.aegis`: the signed update manifest.
- `installers/`: the desktop installers for each platform.
- `images.tar.gz`: the server images. Only bundles for server customers have it.

### Check it

Check the bundle on any machine before the files enter the site. The vendor
gives you the public key file `aegistara-update.pem` separately, with your
contract or read aloud by its fingerprint. Do not trust a key that came inside
the bundle. Checking the signature needs OpenSSL 3 or later (LibreSSL, the
default on macOS, cannot do it).

```sh
openssl pkeyutl -verify -pubin -inkey aegistara-update.pem -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig
# prints: Signature Verified Successfully
sha256sum -c --strict SHA256SUMS      # macOS: shasum -a 256 -c --strict SHA256SUMS
```

### Server

```sh
docker load -i images.tar.gz
cp /path/to/license.aegis deploy/license.aegis
cp deploy/.env.example deploy/.env    # fill in as in Steps, then set:
# AEGIS_SERVER_IMAGE=aegistara-server:<release>
docker compose -f deploy/compose.yaml --env-file deploy/.env up -d --no-build
```

The compose file and `.env.example` come from the vendor with the bundle.

### Desktop

Install the file for your platform from `installers/`. Leave `update_url`
empty.

### Updates

A new release comes as a new bundle. Check it the same way, then run
`docker load -i images.tar.gz`, change `AEGIS_SERVER_IMAGE` in `deploy/.env`,
and run `up -d --no-build`. Hand the new installers to users.

## Known limits of this build

- **Password login has no e-mail reset and no MFA.** Use it for tests and
  trials. Customers get company SSO (decision D10).
- **One server instance per database.** The account cache assumes it
  (ADR 0035).
- **`AEGIS_DB_TLS=encrypt` does not check the database certificate.** Use
  `verify` when the database has a real certificate.

## Event log size

Every server keeps an event log (logins, exports, baselines, deleted
projects, admin actions), also with company login only (ADR 0046). An hourly
job removes events older than `AEGIS_EVENT_RETENTION_DAYS` (default 365) and
anything beyond `AEGIS_EVENT_MAX_ROWS` (default 1,000,000, oldest first). The
row cap stops a flood of failed logins from filling the disk. If an audit
rule needs longer keeping, raise the retention or copy the table out before
the job runs. A row is about 150 bytes, so the default cap is near 150 MB.

## Test server

A shared test server runs on Railway: `docs/railway.md`. A local copy of it:

```sh
docker compose -f deploy/compose.test.yaml up --build
```

## Updates

Clients check a signed update manifest (`AEGIS-UPDATE-1...`). It is signed with
a key that is different from the license key. Help, "Check for updates" fetches
it from `update_url` (settings file) or `AEGIS_UPDATE_URL`, over HTTPS. A
client only offers a release that is newer than the one it runs. Help,
"Download update" then saves the installer to the user's Downloads folder
(`AEGIS_UPDATE_DIR` changes it; with no Downloads folder it makes a new private folder in the temporary folder for each download) and checks its size and SHA-256 against the
manifest while it arrives. A file that fails is deleted. The app never runs
the installer: the user closes the app and opens the file (ADR 0048). A site
without internet leaves `update_url` empty and installs each release from the
offline bundle (see "Offline install (air-gapped sites)").
