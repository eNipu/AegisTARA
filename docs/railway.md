# Test server on Railway

The shared test server runs on the owner's Railway account (decision D11).
It is for testing and demos. It is not a customer deployment.

## What is there

| Item | Value |
|---|---|
| Railway project | `aegistara-test`, environment `production` |
| Services | `aegis-server` (this repository), `Postgres` (Railway template) |
| Source | GitHub `eNipu/AegisTARA`, branch `claude/inspiring-clarke-phmv4u`. Every push to the branch builds and deploys |
| Build | `deploy/Dockerfile`, chosen by `railway.json` |
| Address | `https://aegis-server-production-2bf0.up.railway.app` |
| Health check | `GET /healthz` (Railway checks it after each deploy) |
| Login | User name and password (`AEGIS_AUTH=password`) |

## Service variables

Names only. Values are in the Railway dashboard, service `aegis-server`,
tab Variables.

| Variable | Meaning |
|---|---|
| `AEGIS_AUTH` | `password` |
| `AEGIS_ADMIN_USERNAME` | The first administrator, created only when the user table is empty |
| `AEGIS_ADMIN_PASSWORD` | Its first password. A random value, set only in Railway. Change it in the app after the first login |
| `AEGIS_SERVER_NAME` | Shown on the login screen |
| `AEGIS_DATABASE_URL` | Reference to the `Postgres` service's private URL |
| `AEGIS_DB_TLS` | `off`: the database is on Railway's private network |
| `AEGIS_DEV_LICENSE` | `1`: no license file. Works only because of the next line |
| `CARGO_FEATURES` | `dev-license`, passed to the Docker build |
| `PORT` | `50051`, the port the server listens on and the domain targets |
| `RAILWAY_DOCKERFILE_PATH` | `deploy/Dockerfile` |
| `RUST_LOG` | Log filter |

## Connect the desktop app

1. Start the app: `cargo run -p aegis-shell --release`.
2. Server address: `https://aegis-server-production-2bf0.up.railway.app`.
   Leave the transport on **Automatic** (it picks gRPC-Web for `https://`).
   Railway's edge forwards HTTP/1.1 only, so plain gRPC does not work here.
3. Log in as the admin user with the password from the Railway variables.
4. Open **Admin**, change your password, and create users for the testers.

Or skip the server screen with
`AEGIS_URL=https://aegis-server-production-2bf0.up.railway.app cargo run -p aegis-shell --release`.

## Operate

- **Logs:** Railway dashboard, service `aegis-server`, Deployments, View logs.
  A good start shows `applied database migration`, `login: password` and
  `serving gRPC and gRPC-Web listen=0.0.0.0:50051`.
- **Reset the admin password:** log in as another admin and set it in
  **Admin**. If no admin can log in, empty the `aegis_user` table in the
  `Postgres` service (this removes every user and logs everyone out) and
  redeploy; the server then creates the admin again from the variables.
- **Wipe all data:** delete and recreate the `Postgres` service volume, then
  redeploy. Migrations run at start.
- **Turn it off:** remove the domain, or set the service replicas to 0.

## Limits

- `dev-license` skips the license check, so there is no seat limit. Never
  use this image for a customer.
- One replica only. The account cache assumes one server per database
  (ADR 0035).
- Proxies may close idle HTTP streams. The server sends a heartbeat on `Watch`
  every 20 s and the app restarts a stream that is silent for 60 s
  (ADR 0036).
