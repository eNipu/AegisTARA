# AegisTARA

A learning project: a Threat Analysis and Risk Assessment (TARA) tool, written fully in Rust.

- A native desktop client talks to a server that each customer company hosts in its own cloud.
- Users log in with their company credentials.
- Automotive first (ISO/SAE 21434, UN R155). Other methodologies come later as data packs.
- Visual attack trees with feasibility scoring, explain-why for every number, versions and baselines, and executive reports.
- AI features are planned in the backlog.

## Status

The deterministic core, the server, password and OIDC login, the canvas
engine, the client core, a desktop app with every TARA page, offline
licensing, baselines, reports and release scaffolding are built and tested
end to end. Reports share one document model (HTML, PDF and XLSX) with
selectable sections, filters, branding and a per-export audit record, and the
UI and reports are translated into English and Japanese (with a pseudo-locale
for layout tests). A test server runs on Railway
([docs/railway.md](docs/railway.md)). Roadmap and open items:
[docs/status-and-roadmap.md](docs/status-and-roadmap.md).
Running the server: [docs/deployment.md](docs/deployment.md).

| Crate | Role |
|---|---|
| [`aegis-model`](crates/aegis-model) | Domain model, risk engine, analysis, persistence formats |
| [`aegis-proto`](crates/aegis-proto) | gRPC messages and service shared by client and server |
| [`aegis-server`](crates/aegis-server) | The server: actors, edit leases, PostgreSQL storage, audit, license gate |
| [`aegis-auth`](crates/aegis-auth) | Login: OIDC with PKCE, DPoP, keychain, shared TLS setup |
| [`aegis-canvas`](crates/aegis-canvas) | Attack tree layout, hit testing, display list, SVG |
| [`aegis-client`](crates/aegis-client) | Client core with no UI toolkit: editing, sync, view model, update checks |
| [`aegis-shell`](crates/aegis-shell) | Current full-workflow Slint desktop window (hosted and embedded standalone; slated for retirement after GPUI parity) |
| [`aegis-workbench`](crates/aegis-workbench) | Next-generation GPUI desktop workbench (standalone workspace folders, server sign-in/preview, grids, inspector) |
| [`aegis-license`](crates/aegis-license) | License files and signed update manifests |

Start with [docs/current-architecture.md](docs/current-architecture.md) for the
post-hardening overview of all nine crates, runtime modes, authoritative state,
and recovery paths. Struct-level detail lives in
[docs/architecture.md](docs/architecture.md) (Part I: `aegis-model`, Part II:
the rest of the system). Build and test from the repo
root:

```sh
cargo build --workspace
cargo test --workspace
```

## Running in RustRover

Open the repository root (`AegisTARA/`) in RustRover so it attaches the root
`Cargo.toml` workspace (`crates/*`).

### One-click Run Configurations

Use the Run Configuration dropdown in the top-right toolbar and click the green
**Run** button:

| Run Configuration | Purpose |
|---|---|
| **`1. Run Server + GUI (Auto-Login)`** | Starts the local in-memory server on `127.0.0.1:50051` and launches the desktop GUI already logged in as `admin` (skips the login screen). |
| **`2. Run Server + GUI (Password Login)`** | Starts the local server with password authentication and opens the desktop GUI at the login screen. Log in with user **`admin`** and password **`aegis-admin-dev`**. |
| **`3. Server (Password Login)`** | Starts `aegis-server` alone on `127.0.0.1:50051` with password login (`admin` / `aegis-admin-dev`). |
| **`4. GUI (Desktop App)`** | Starts `aegis-shell` alone, pointed at `http://127.0.0.1:50051`. |
| **`5. Server (Dev Auto-Login)`** | Starts `aegis-server` alone on `127.0.0.1:50051` with development auth (`AEGIS_AUTH=dev`). |
| **`6. GUI (Dev Auto-Login)`** | Starts `aegis-shell` alone, auto-logged in as `admin` against `http://127.0.0.1:50051`. |
| **`7. Build Workspace`** | Builds all workspace targets (`build --workspace --features aegis-server/dev-license,aegis-auth/os-keychain`). |
| **`8. Test Workspace`** | Runs the full test suite (`test --workspace`). |
| **`9. Smoke Test Server`** | Runs the `smoke` example client against `http://127.0.0.1:50051` (`admin` / `aegis-admin-dev`) to verify a running password-login server. |

### Manual Setup / Reference

The shared run configurations are tracked in `.idea/runConfigurations/`. To
recreate or customize them manually in **Run → Edit Configurations…**
(`+` → **Cargo** / **Compound**) with working directory `$PROJECT_DIR$`:

1. **Server (`aegis-server`)**
   - **Command (Password Login):** `run --package aegis-server --bin aegis-server --features aegis-server/dev-license,aegis-auth/os-keychain`
   - **Command (Dev Auto-Login, loopback-only opt-in):** `run --package aegis-server --bin aegis-server --features aegis-server/dev-license,aegis-server/dev-auth,aegis-auth/os-keychain`
   - **Environment variables (Password Login):**
     `AEGIS_DEV_LICENSE=1`; `AEGIS_AUTH=password`; `AEGIS_ADMIN_USERNAME=admin`; `AEGIS_ADMIN_PASSWORD=aegis-admin-dev`; `AEGIS_LISTEN=127.0.0.1:50051`; `AEGIS_SERVER_NAME=AegisTARA Local`; `RUST_LOG=info`
   - **Environment variables (Dev Auto-Login — rejected in normal builds without `dev-auth`):**
     `AEGIS_DEV_LICENSE=1`; `AEGIS_AUTH=dev`; `AEGIS_LISTEN=127.0.0.1:50051`; `AEGIS_SERVER_NAME=AegisTARA Dev`; `RUST_LOG=info`
2. **GUI (`aegis-shell`)**
   - **Command:** `run --package aegis-shell --bin aegis-shell --features aegis-server/dev-license,aegis-auth/os-keychain`
   - **Environment variables (Password Login):**
     `AEGIS_URL=http://127.0.0.1:50051`; `RUST_LOG=info`
   - **Environment variables (Dev Auto-Login):**
     `AEGIS_URL=http://127.0.0.1:50051`; `AEGIS_DEV_USER=admin`; `AEGIS_DEV_ADMIN=1`; `RUST_LOG=info`
3. **Both at once (`Compound`)**
   - Add a **Compound** configuration containing the Server and matching GUI configurations so both launch with one click.

> **Tip:** Passing `--features aegis-server/dev-license,aegis-auth/os-keychain` to both the server and GUI configurations keeps a single unified Cargo feature graph, preventing recompilation when switching between targets. The root `Cargo.toml` also sets `[profile.dev.package."*"] opt-level = 2` (and `opt-level = 1` for workspace crates) so Slint's CPU software renderer, path rasterizer, and font shaping run smoothly even in debug/dev runs.

## Documents

1. [docs/decisions.md](docs/decisions.md): decisions made so far. Start here.
2. [docs/direction-v1.md](docs/direction-v1.md): desktop client, customer-hosted server, login, v1 scope.
3. [docs/ai-backlog.md](docs/ai-backlog.md): how AI features fit, and in which order.
4. [docs/engineering-standards.md](docs/engineering-standards.md): coding requirements for performance, data structures, algorithms, and learning.
5. [docs/current-architecture.md](docs/current-architecture.md): post-hardening architecture overview (all nine crates, modes, recovery, security/identity/compatibility boundaries).
6. [docs/status-and-roadmap.md](docs/status-and-roadmap.md): historical delivery log and handoff roadmap.
7. [docs/architecture.md](docs/architecture.md): struct-level architecture reference.
8. [spikes/gpui-shell/RESULTS.md](spikes/gpui-shell/RESULTS.md) and [spikes/RESULTS.md](spikes/RESULTS.md): UI toolkit spikes (Slint for v1; GPUI chosen for the workbench replacement in ADR 0066).
9. [docs/brainstorm.md](docs/brainstorm.md): the full first brainstorm.
10. [docs/deployment.md](docs/deployment.md) and [docs/release-checklist.md](docs/release-checklist.md): running and shipping the server.
11. [docs/adr/](docs/adr): decision records `0001` through `0075` (covering the model, workspace, server, auth, canvas, client, shell, licensing, reports, GPUI workbench, origin isolation, qualified identities, methodology pins, and engine compatibility).
12. [docs/review-requirements.md](docs/review-requirements.md): what to review before the work is accepted.
13. [docs/railway.md](docs/railway.md): the test server on Railway.
14. [docs/security-pack/](docs/security-pack/README.md): the product security pack, in English and Japanese, for a buyer's security team and for answering vendor questionnaires.

## Continuous integration

Every push and pull request runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
on each production crate (`crates/*`, one Cargo workspace, ADR 0025): `cargo fmt --check`,
`cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`,
the `cargo-deny` policy below, and `cargo check --locked` at the minimum
supported Rust version (1.99.1). Rationale: [docs/adr/0007-workspace-ci.md](docs/adr/0007-workspace-ci.md).

Run the same gates locally before you push. Pull requests on GitHub only
run `cargo-deny`; the other jobs run once a change lands on `master`. The
pre-push hook runs `scripts/ci.sh` for you (skip once with
`git push --no-verify`); enable it once per clone:

```sh
git config core.hooksPath .githooks
```

The commands:

```sh
scripts/ci.sh                    # fmt, clippy, test for the whole workspace
scripts/ci.sh crates/aegis-model # one crate
scripts/deny.sh                  # cargo-deny (see below)
tools/deny-fixtures/run.sh       # deny.toml policy fixtures
```

`scripts/ci.sh` prints one `ok` or `FAIL` line per step, shows the
output of failing steps only, and exits 1 if any step failed. CI pins one
Rust version (`RUST_TOOLCHAIN` in `ci.yml`); a newer local compiler may
report lints CI does not see yet.

## Dependency licenses

Production crates (`crates/*`) must pass the `cargo-deny` policy in
[`deny.toml`](deny.toml) (decision D5, standard Q7): licenses, advisories,
bans, and sources. Rationale: [docs/adr/0003-cargo-deny-policy.md](docs/adr/0003-cargo-deny-policy.md).

Install the pinned version (cargo-deny 0.20.2):

```sh
cargo install cargo-deny --version 0.20.2 --locked
```

Run it from the repo root:

```sh
scripts/deny.sh                          # all checks, every crate (fetches the advisory database)
scripts/deny.sh licenses bans sources    # offline
tools/deny-fixtures/run.sh               # proves the policy rejects what D5 forbids

# one crate by hand (cargo-deny 0.20: --config goes before `check`)
cargo deny --manifest-path crates/aegis-model/Cargo.toml --config deny.toml check
```

Only the 11 licenses allowed by D5 pass. To accept a "review first" license
(for example MPL-2.0), first record the approval as a decision in
`docs/decisions.md`, then add one exception per crate and version to
`[licenses] exceptions` in `deny.toml`, with a comment naming that decision:

```toml
exceptions = [
    # Approved in D<N>: <reason>.
    { crate = "name@1.2.3", allow = ["MPL-2.0"] },
]
```

Never widen `allow` for a review-first license, and never add a `deny` list.

Non-shipped tooling (dev-dependencies and fuzz crates) may also use NCSA
(D5), again only through a per-crate exception, plus a `[bans] deny` entry
with `wrappers` that keeps the crate out of everything that ships.
`scripts/deny.sh` also checks each `crates/*/fuzz/Cargo.toml`.

## Fuzzing

`crates/aegis-model/fuzz/` is a `cargo-fuzz` crate (not a production crate,
not in CI) with three targets: `snapshot` (`Project::load`), `op_log`
(`OpLogReader`), and `pack` (`Methodology::read_pack`). Seeds are the golden
v1 files and the two methodology packs in `seeds/`. Rationale:
[docs/adr/0009-save-load-format.md](docs/adr/0009-save-load-format.md) and
[docs/adr/0011-methodology-pack-files.md](docs/adr/0011-methodology-pack-files.md).

```sh
crates/aegis-model/fuzz/run.sh                # all three targets, 60 s each, stable toolchain
crates/aegis-model/fuzz/run.sh 600 op_log     # one target, 10 minutes

# or, with a nightly toolchain and cargo-fuzz (adds AddressSanitizer)
cd crates/aegis-model/fuzz
cargo +nightly fuzz run snapshot corpus/snapshot seeds/snapshot -- -max_total_time=60
```

A crash is saved under `artifacts/<target>/`; replay it by passing that file
to the target binary (or `cargo fuzz run <target> <file>`).
