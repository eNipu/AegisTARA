# AegisTARA — Project Status & Handoff Roadmap

- **Date:** 2026-10-26
- **Baseline Commit:** branch `claude/inspiring-clarke-phmv4u` (jobs 037 to 051). Earlier baselines: `202f296` (job 036), `b7ca167` (job 026).
- **Audience:** the next maintainer or agent picking up this repository.
- **Companion documents:** [architecture.md](architecture.md) (Part I: `aegis-model`; Part II: the whole system), [direction-v1.md](direction-v1.md) (product vision), [engineering-standards.md](engineering-standards.md) (binding rules), [decisions.md](decisions.md) (D1–D11), [review-requirements.md](review-requirements.md) (what to review next), [railway.md](railway.md) (test server), [ai-backlog.md](ai-backlog.md) (post-v1 AI features), [PROJECT_REVIEW.md](../PROJECT_REVIEW.md) (independent review, 2026-10-20).

---

## 1. Status Snapshot

**Product vision (v1):** a native desktop TARA client (Slint) talking to a customer-hosted single-tenant server (Rust + PostgreSQL), automotive first (ISO/SAE 21434, UN R155/R156), with deterministic risk computation, explain-why for every number, and sovereign (air-gapped-capable) deployment.

**Reality today:** the deterministic core, the server, password and OIDC login, the canvas engine, the client core, a desktop app with every TARA page, offline licensing, live observers, baselines, history, exports and release scaffolding are built and tested end to end. One Cargo workspace holds eight crates. A test server runs on Railway (`docs/railway.md`). **The reviews (code, security, UX, UI) for jobs 027 to 051 have not been run yet**; the owner asked for the build first. The table at the top of `docs/review-requirements.md` lists every job with its four reviews, all `pending`, and section 7 lists what the jobs added after 041 need.

| Area | State | Evidence |
|---|---|---|
| Domain model, risk engine, analysis, persistence (`aegis-model`) | **Complete (jobs 001–026)** | See the table below |
| Cargo workspace, CI, supply chain | **Complete (027, 028)** | Root workspace, `scripts/ci.sh`, `scripts/deny.sh`, 9 policy fixtures, Postgres service in CI |
| Server (`aegis-server`, `aegis-proto`) | **Complete (029, 030)** | gRPC API, one actor per project, atomic batches, edit lease (D4), observers, MemoryStore and PostgresStore, hash-chained audit, `VerifyAudit` |
| Login (`aegis-auth`, server accounts) | **Complete (031, 038, 040, 045)** | Password login for testing (Argon2id, opaque tokens, throttle, user admin). OIDC code flow + PKCE, DPoP, JWKS cache, keychain, HTTPS with the OS trust store. `AEGIS_AUTH=both` runs company and password login together, and an admin turns the password form off in the app (045). Company SSO per customer is in the backlog (D10) |
| Canvas (`aegis-canvas`) | **Complete (032)** | Linear-time layout (1M nodes in 51 ms), R-tree hit test, display list, SVG snapshots, keyboard navigation |
| Client core (`aegis-client`) | **Complete (033)** | Optimistic editing, undo as edits, lease handling, view model, 10 end-to-end tests against the real server |
| Desktop app (`aegis-shell`) | **Complete (033, 041)** | Menus with Lucide icons, every TARA page, outline tree editor, baselines and comparison, history, exports, admin, server and login screens; headless UI tests with screenshots |
| Offline licensing (`aegis-license`) | **Complete (034)** | Ed25519 license files, grace period, seat ledger, issuer tool, server gate |
| Live observers | **Complete (035)** | Ops plus recomputed risks streamed to observers; the client cross-checks its own numbers |
| Release scaffolding | **Built and deployed (036, 040, 046, 048)** | Release profile, Dockerfile, Compose files, signed update manifest, now fetched over HTTPS (Help > Check for updates, 048). CI builds the image and starts it (046; its docker lines first run in CI) |
| TLS to PostgreSQL, `https://` clients, seat restore | **Complete (037, 038, 039)** | `AEGIS_DB_TLS`, rustls with ring and OS roots, seats rebuilt from the op log |
| Transport through proxies | **Complete (040)** | gRPC-Web next to gRPC, `/healthz`, `PORT`, `Watch` heartbeats and a client silence watchdog |
| Reports and admin views | **Complete (047, 049, 050, 051, 052, 053)** | PDF report (krilla, Inter font) with attack tree pictures (overview plus readable detail panels for big trees), "numbers checked" mark that follows the confirmed change, admin event log on every server with retention, license page, a screen reader path for the attack tree canvas |
| AI features | **Backlog only** | `docs/ai-backlog.md`; the v1 hooks are in place |

### Factory jobs

| Jobs | Delivered | ADRs |
|---|---|---|
| 001–008 | Arena DAG, cycle detection, cargo-deny, parent-storage benchmark, evaluation, explain-why, property tests, CI | 0001–0007 |
| 009–016 | Domain entities, sparse ref-counts, save/load, methodology packs, threat↔tree links, provenance, cleanups, tree/provenance wire format | 0008–0014 |
| 017–024 | Risk determination, direct feasibility, controls & residual risk, linter, top-k paths, project diff, attack potential, goals & claims | 0015–0022 |
| 025–026 | Incremental evaluation cache; choke points, hitting sets & sensitivity | 0023–0024 |
| 027–028 | Cargo workspace, README, decision cleanup | 0025 |
| 029–030 | Server skeleton, persistence and audit | 0026, 0027 |
| 031 | Login | 0028 |
| 032 | Canvas engine | 0029 |
| 033 | Client core and Slint shell | 0030, 0031 |
| 034 | License files | 0032 |
| 035 | Live observers | 0033 |
| 036 | Release scaffolding | 0034 |
| 037–039 | Database TLS, HTTPS clients, seat restore | 0036, 0037 |
| 040 | Password login, gRPC-Web, Railway test server | 0035, 0036 |
| 041 | Every TARA page, baselines, history, reports | 0037, 0038 |
| 045 | Password login switch | 0039 |
| 046 | Server image in CI | 0040 |
| 047 | Numbers checked mark | 0041 |
| 048 | Update manifest over HTTPS | 0042 |
| 049 | Admin event log and license page | 0043 |
| 050 | PDF report | 0044 |
| 051 | Screen reader path for the canvas | 0045 |
| 052 | Event log on every server, retention, stress test | 0046 |
| 053 | Attack tree pictures in the PDF | 0047 |

Jobs 027 to 051 have **not** been through the factory review stages (code, security, UX, UI). The list is in `docs/review-requirements.md`.

---

## 2. Where the Code Stands vs. the v1 Vision

The implementation keeps evaluation and persistence in `aegis-model` and compiles the same crate into client and server, which satisfies the *engine parity* requirement. Around it:

| Crate | Role |
|---|---|
| `aegis-model` | Domain model, risk engine, analysis, persistence formats, `OpCodec`, `ENGINE_VERSION` |
| `aegis-proto` | gRPC messages and service (built with `protox`, no `protoc`) |
| `aegis-server` | The server binary and library: actors, leases, stores, audit, license gate |
| `aegis-auth` | Login: JOSE, PKCE, DPoP, JWKS, OIDC, keychain, request layers |
| `aegis-canvas` | Layout, hit testing, display list, SVG |
| `aegis-client` | Client core with no UI toolkit: workspace, session, view model, update checks |
| `aegis-shell` | The Slint window. The only crate allowed to depend on Slint (`deny.toml` ban) |
| `aegis-license` | License files and update manifests: verify, sign (feature), seat ledger |

`direction-v1.md` §13 next steps, scored against today:

1. UI spike: **done** (D7, Slint).
2. Domain model v0: **done**.
3. Engine v0: **done**.
4. Workspace skeleton: **done** (root workspace, CI, cargo-deny).
5. Server and login skeleton: **done** (029 to 031).
6. Methodology pack format: **done** (job 012).

Not yet built from the direction: groups and policies in the admin screens, the attack tree picture inside the PDF (it has the outline), installer download and install after an update check, the AI waves.

---

## 3. Open Gaps & Housekeeping (updated 2026-10-25)

| # | Gap | State | Suggested action |
|---|---|---|---|
| G1 | README status | **Done** (027) | |
| G2 | Root workspace | **Done** (027) | |
| G3 | Dangling worktrees and branches | **Done** locally. The four merged remote branches are listed in D9 | Owner deletes them on GitHub |
| G4 | D4 (single-editor lock) | **Working assumption** in D9, built as written | Owner confirms |
| G5 | D7 (Slint attribution condition) | **Working assumption** in D9: About window plus website badge. `deny.toml` carries an exception and a ban that confines Slint to `aegis-shell` | Owner confirms, or deletes `crates/aegis-shell` |
| G6 | `direction-v1.md` §4 drift | **Done** (028) | |
| G7 | Node deletion semantics for `AttackTree` | Not needed yet | Decide when the first delete op is specified |
| G8 | **Reviews of jobs 027 to 051** (code, security, UX, UI) | **Not run** (deferred by the owner) | Follow `docs/review-requirements.md` |
| G9 | `https://` for login, key sets and update fetching | **Done** (038) | |
| G10 | TLS to PostgreSQL | **Done** (037) | |
| G11 | Seat counts reset when the server restarts | **Done** (039) | |
| G12 | The server image and Compose file were never run | **Mostly done** (046): CI builds the customer and test images and starts the test one. The docker lines have not run outside CI yet | Check the first CI run of job `server image`; run the release checklist once |
| G13 | `vendor.keys` and `update.keys` hold no key | **Open by design** | Owner generates the two production keys (`aegis-license keygen`) before the first release |
| G14 | Accessibility: no screen reader path for canvas items | **Built** (051): a sentence per node on screen, spoken selection, Tab and arrow keys, Enter selects. Not tried with a real screen reader | Hands-on pass with Orca, NVDA or VoiceOver in the UX review |
| G15 | The shell shows no "numbers checked with the server" mark, no admin screens, no reports | **Done** (041, 047, 049, 050) | |
| G16 | Company SSO (Entra ID, Google, other OIDC) | **Waiting for a customer order** (D10) | `factory/backlog.md`, section "Waiting for a customer order" |

---

## 4. Handoff Roadmap

```mermaid
flowchart TD
    P0["Phase 0 — Housekeeping<br/>(G1–G6, root workspace, README)"]
    P1["Phase 1 — Server skeleton<br/>(tonic gRPC, lease lock D4, audit log,<br/>PostgreSQL snapshots + oplog)"]
    P2["Phase 2 — Client core<br/>(canvas display list, Slint shell,<br/>OIDC PKCE + DPoP + keychain)"]
    P3["Phase 3 — Release readiness<br/>(installers/updates, offline Ed25519<br/>license tokens, live read-only view)"]
    P4["Phase 4 — AI waves 1–4<br/>(per docs/ai-backlog.md; hooks already built)"]
    P0 --> P1
    P0 --> P2
    P1 --> P2
    P2 --> P3
    P3 --> P4
```

> **Phases 0 to 3 are built** on the branch named above, with the open items in section 3. The review stages and a customer deployment are left before Phase 4.

### Phase 0 — Housekeeping (unblocks everything) — done
- Root workspace, README refresh, worktree cleanup, D4/D7 decisions, §4 drift fix.
- **Exit:** `cargo build` from the repo root; README describes the real state.

### Phase 1 — `aegis-server` skeleton — done
- tonic gRPC API per `direction-v1.md` §8; project lease lock (D4, 2-minute renewal); append-only op-log persistence in PostgreSQL; audit log for every op.
- Reuse `aegis-model` as the authoritative engine: validate + apply ops server-side, stream accepted ops to observers.
- **Rules to honor:** R3 (decode caps at every trust boundary), P4 (determinism), D10 (derived state stays rebuildable).
- **Exit:** headless server round-trip test: apply op → persisted → reload → identical `Project`.

### Phase 2 — `aegis-client` core + Slint shell — done
- Canvas engine emitting the toolkit-agnostic display list (`direction-v1.md` §5) so desktop, SVG export, and Typst PDF reports share one pipeline.
- Slint desktop shell (D7) on the mockups in `docs/design/`; optimistic local evaluation via `ProjectEvalCache` (5 ms budget, rule A1); OS keychain token storage, OIDC PKCE + DPoP login.
- **Exit:** edit a step rating in the GUI, see residual risk update within one frame.

### Phase 3 — Release readiness — built, test server running
- Installers + auto-update (§10), offline Ed25519 license tokens (§12), live read-only observers (§7).
- **Exit:** a sovereign (air-gapped) demo deployment. The Railway test server is the first step.

### Phase 4 — AI features (post-v1, per `docs/ai-backlog.md`)
- Wave 1: smart catalog search, threat suggestions, damage writer, summary wording, wording cleanup.
- The v1 hooks AI needs are already in place: provenance (`ProvenanceSource::Ai`), typed invertible ops, catalog IDs, engine-as-tool, derivation traces, oplog audit.

---

## 5. Next Factory Backlog Entries

Jobs 027 to 041 are ticked in `factory/backlog.md`. What remains there:

```text
- [waiting] SSO with Microsoft Entra ID (042), Google (043), other OIDC (044): when a customer orders it
```

Everything else in the backlog is ticked. Open items, in order:

1. Run the four reviews on jobs 027 to 051 (G8, `docs/review-requirements.md`).
2. Watch the first CI run of the `server image` job and run the release checklist once (G12).
3. ~~Event store for company-login-only servers~~ done (052, ADR 0046).
4. ~~Attack tree picture inside the PDF~~ done (053, ADR 0047). Still open: make the HTML report use the same document model (see `brainstorm-logs-and-reports.md`).
5. Installer download, hash check and install after the update check (ADR 0042).
6. Admin: groups and policies (direction §6).
7. Phase 4, AI wave 1.

---

## 6. Handoff Quick Start

**Build & verify (mirrors CI):**

```bash
scripts/ci.sh                  # fmt, clippy -D warnings, all tests, doc build
scripts/deny.sh                # license/advisory policy (cargo-deny 0.20.2)
tools/deny-fixtures/run.sh     # 9 policy fixtures
```

**Orientation in 15 minutes:**

1. Read `docs/architecture.md`: Part I covers every struct and call flow in `aegis-model`; Part II covers the server, client, app, database and flows.
2. Skim `docs/decisions.md` (D1–D11) then the ADRs for the area you touch (`docs/adr/0001`–`0038`).
3. Treat `docs/engineering-standards.md` as binding: code comments cite its rules (`D*`, `A*`, `P*`, `Q*`, `R*`).

**Running it:**

```bash
# A server for development (in memory, no login, no license check)
AEGIS_DEV_AUTH=1 AEGIS_DEV_LICENSE=1 cargo run -p aegis-server --features dev-license
# A server with password login (first admin from the variables)
#   AEGIS_AUTH=password AEGIS_ADMIN_PASSWORD='choose a long one' AEGIS_DEV_LICENSE=1 cargo run -p aegis-server --features dev-license
# The window, as the development user "alice"
AEGIS_DEV_USER=alice cargo run -p aegis-shell      # other variables: crates/aegis-shell/src/main.rs
# Make keys and a license
cargo run -p aegis-license --features issuer -- keygen --key-id dev --out /tmp/dev.pk8
```

Production deployment: `docs/deployment.md`. Release steps: `docs/release-checklist.md`.

**Invariants that will bite you if broken:**

- **D10 — derived state is rebuildable.** `RefCounts`, `Evaluation`, caches are never source of truth; source state lives in `Project` arenas, `AttackTree`, `Methodology`.
- **Ops are validated and invertible.** Every mutation goes through `Project::validate` + `Project::apply`, which returns the inverse `Op`. Never mutate arenas directly.
- **R3 — cap before you allocate.** All deserialization paths enforce `Limit`/`BoundedReader`/`CapSeq` before allocation.
- **R6 — no entity names in lint/error output.** Deterministic, leak-free diagnostics.
- **A4 — never materialize all attack paths.** Use the lazy `AttackPaths` iterator.
- **P4 — determinism.** Same ops in → same bytes out; iteration order comes from arenas and `BTreeMap`s, never `HashMap`s.
- **Wire format uses string keys, never indices** — reordering a methodology pack must never change stored meaning.
