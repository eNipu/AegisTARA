# Review requirements

This page lists what must be reviewed before the work on this branch is
accepted. The owner asked for the build first and the review after. Nothing
here has been reviewed yet.

## Pending reviews

One row per job. A cell says `pending` until that review has run and passed.
Add a row for every new job.

| Job | Code | Security | UX | UI |
|---|---|---|---|---|
| 027 workspace and README | pending | pending | pending | pending |
| 028 decision cleanup | pending | pending | pending | pending |
| 029 server skeleton | pending | pending | pending | pending |
| 030 server persistence | pending | pending | pending | pending |
| 031 login | pending | pending | pending | pending |
| 032 canvas engine | pending | pending | pending | pending |
| 033 client core and shell | pending | pending | pending | pending |
| 034 license tokens | pending | pending | pending | pending |
| 035 live observers | pending | pending | pending | pending |
| 036 release scaffolding | pending | pending | pending | pending |
| 037 Postgres TLS | pending | pending | pending | pending |
| 038 HTTPS client | pending | pending | pending | pending |
| 039 seat ledger persistence | pending | pending | pending | pending |
| 040 password login and gRPC-Web | pending | pending | pending | pending |
| 041 TARA pages | pending | pending | pending | pending |
| 045 password login switch | pending | pending | pending | pending |
| 046 server image in CI | pending | pending | pending | pending |
| 047 numbers checked mark | pending | pending | pending | pending |
| 048 update manifest fetch | pending | pending | pending | pending |
| 049 admin events and license | pending | pending | pending | pending |

- **Branch:** `claude/inspiring-clarke-phmv4u`
- **Base for the diff:** `master` (`git diff master...claude/inspiring-clarke-phmv4u`)
- **Jobs in scope:** 027 to 041 (`factory/backlog.md`)
- **Reviews:** code, security, UX, UI. The factory agents `review-code`,
  `review-security`, `review-ux` and `review-ui` can run them; a human
  reviewer can use the same lists.
- **References:** `docs/architecture.md` Part II (what exists),
  `docs/engineering-standards.md` (the rules), ADRs 0025 to 0038 (why).

A review passes when every item below is checked, or has a finding with a
file, a line, the problem and a fix.

---

## 1. How to run everything

```sh
scripts/ci.sh                      # fmt, clippy -D warnings, all tests, docs
scripts/deny.sh                    # licenses and advisories
tools/deny-fixtures/run.sh         # license policy fixtures

# PostgreSQL tests (CI has a Postgres service; locally use Docker)
docker run -d --name pg -e POSTGRES_USER=aegis -e POSTGRES_PASSWORD=aegis \
  -e POSTGRES_DB=aegis_test -p 5432:5432 postgres:16
AEGIS_TEST_DATABASE_URL=postgres://aegis:aegis@127.0.0.1:5432/aegis_test \
  cargo test -p aegis-server --test postgres

# Optional: a TLS-only database for the TLS test
AEGIS_TEST_TLS_DATABASE_URL=postgres://aegis:aegis@localhost:5433/aegis_test \
  cargo test -p aegis-server --test postgres tls

# Screenshots of every page and the login screens
cargo test -p aegis-shell
ls target/shell-screenshots/

# The full stack locally: server with password login and PostgreSQL
AEGIS_ADMIN_PASSWORD='choose a long one' docker compose -f deploy/compose.test.yaml up --build

# Smoke test of any running server (login, who am I, projects, logout)
AEGIS_URL=http://127.0.0.1:50051 AEGIS_USER=admin AEGIS_PASSWORD='...' \
  cargo run -p aegis-client --example smoke
```

The test server on Railway (`docs/railway.md`) runs the same branch.

---

## 2. Code review

Area by area. For each: logic is right, edge cases are tested, the rules
cited in comments are really followed.

### 2.1 Server

- [ ] `accounts/mod.rs`: login, sessions, throttle, last-admin rule, user
      changes end sessions. No lock held across `.await` (C2).
- [ ] `accounts/postgres.rs` and `migrations/002_accounts_baselines.sql`:
      SQL matches the in-memory backend. Every write that changes two tables
      is one transaction.
- [ ] `auth_layer.rs`: only `/aegis.v1.Accounts/` passes without a principal.
      A path like `/aegis.v1.AccountsX/` must not pass.
- [ ] `service.rs`: every new handler checks admin where needed
      (`DeleteProject`, user calls, `BreakLock`, `VerifyAudit`). Every input
      length is capped (R3).
- [ ] `registry.rs` and `actor.rs`: rename and delete while a project is open
      and locked by someone else; delete while a `Watch` stream is open.
- [ ] `store/postgres.rs`: `DbTls` modes; baseline limits; delete removes
      baselines, ops and project together.
- [ ] Seat restore at start (`restore_seats`): correct window, correct count
      of distinct editors.
- [ ] `main.rs`: every variable in the module table is read and validated;
      bad values stop the server with a clear message.

### 2.2 Client core

- [ ] `transport.rs`: gRPC-Web URI rewrite; timeouts on connect and calls.
- [ ] `connection.rs`: every new call maps errors to `ServerCode`.
- [ ] `session.rs`: reconnect epochs; `watch_silence` restart; log out
      clears state; `WhoAmI` replaces the configured user.
- [ ] `workspace.rs`: submissions split at `MAX_SUBMIT_OPS`; a rejected batch
      rolls back every later local op.
- [ ] `commands.rs`: each `Edit` expands to valid ops; cascades (remove
      asset, damage, threat, tree, control, goal) leave no dangling link; undo
      of a cascade restores all of it.
- [ ] `outline.rs`: parse and render round trip; shared steps; caps; errors
      name the line.
- [ ] `pages.rs`, `view.rs`, `state.rs`: deterministic order (D7, P4); no
      O(n squared) on large projects (A9).
- [ ] `report.rs`: HTML escaping, CSV formula guard, file names.
- [ ] `settings.rs`: read cap, atomic write, damaged file gives defaults.

### 2.3 Desktop app

- [ ] `launcher.rs`: every login path (password, OIDC, dev) and every error
      ends on a screen the user can leave.
- [ ] `controller.rs`: no work on the UI thread that can block (network,
      file writes of big exports).
- [ ] `provider.rs`: keychain entry names include server and user.

### 2.4 Tests

- [ ] Each ADR decision has a test. Missing tests are findings.
- [ ] No test sleeps for long or depends on wall-clock timing more than it
      must (the new watch test uses 300 ms windows).

---

## 3. Security review

Threat model: an attacker on the internet reaches the server's HTTPS
address; a tester with a normal account tries to do admin things; a stolen
laptop.

- [ ] **Password storage:** Argon2id parameters (19 MiB, 2, 1); the decoy
      hash makes unknown and known users take the same time.
- [ ] **Brute force:** per-name throttle; the error text is the same for
      every failure; no per-IP limit (by design, ADR 0035). Is that enough
      for the Railway test server?
- [ ] **Session tokens:** 32 random bytes from `ring`; only the SHA-256 is
      stored; expiry is absolute; logout, disable and password change delete
      sessions; tokens never appear in logs or errors.
- [ ] **Authorization:** each `Aegis` RPC checks the principal; a normal user
      cannot reach admin calls, break locks, delete projects, or change
      another user.
- [ ] **First admin:** `AEGIS_ADMIN_PASSWORD` is used only when the user
      table is empty; it is not logged.
- [ ] **Public surface:** `/healthz` and `Accounts` only. `GetLoginOptions`
      leaks nothing but the method and server name.
- [ ] **gRPC-Web:** no CORS headers that let a web page call the API with a
      user's token (the app is native; browsers are not a client).
- [ ] **Input limits:** user names, display names, project names, baseline
      names, export detail strings, outline text, stream sizes.
- [ ] **Database TLS:** `encrypt` mode accepts any certificate; is the
      warning in the docs clear? `verify` uses the OS roots.
- [ ] **Exports:** path traversal through project names; CSV injection;
      HTML injection in the report.
- [ ] **Keychain:** what is stored (token only), under which name, removed
      on logout.
- [ ] **dev-license and dev auth:** cannot be enabled in a release build
      (`docs/release-checklist.md`). The Railway build uses `dev-license`;
      confirm that is acceptable for a test server.
- [ ] **Dependencies:** `scripts/deny.sh` passes; new crates (argon2,
      rustls, hyper-rustls, tonic-web, tokio-postgres-rustls,
      rustls-native-certs) are permissive (D5).
- [ ] **Secrets in the repository:** none (`git grep -i password` shows only
      code and docs).

---

## 4. UX review

Walk through each flow in the app against the Railway server or the local
compose stack.

- [ ] **First start:** server screen, a wrong address, an unreachable
      server, a server with password login, a server with company login.
- [ ] **Login:** wrong password, throttled (6 wrong tries), disabled user,
      "keep me logged in" and restart, log out, change server.
- [ ] **A TARA from scratch:** create a project, add assets, damage
      scenarios with impact ratings, threats, an attack tree in the outline
      editor, step ratings, risk values, treatment, controls on steps,
      goals and claims. Every step can be done with the keyboard.
- [ ] **Problems page:** each finding leads to the thing to fix.
- [ ] **Two users:** one edits, one watches; the watcher sees changes
      live and is told why they cannot edit; lock release and takeover.
- [ ] **Versions:** create a baseline, change things, compare; the
      comparison is clear.
- [ ] **History and exports:** the history page reads well; exports land in
      the chosen folder and open in a browser or spreadsheet.
- [ ] **Admin:** create, disable, promote and delete users; the app
      refuses to remove the last admin with a clear message.
- [ ] **Errors and notices:** every error says what happened and what to do,
      in short plain sentences.
- [ ] **Network loss:** unplug the network while editing; the app shows it,
      keeps the edits, and saves them when the network is back.
- [ ] **Accessibility:** focus order, visible focus, contrast, text size;
      the canvas has no screen reader path yet (known gap G14).

---

## 5. UI review

Use the screenshots in `target/shell-screenshots/` and the running app.

- [ ] Menu bar: groups, order, shortcuts shown, Lucide icons match the
      action and are the same size and weight everywhere.
- [ ] Sidebar: TARA steps in order, counts, selected state.
- [ ] Each page: alignment, spacing, column widths, empty states. The risk
      table gives its short columns a minimum width; check long threat names
      at small window sizes.
- [ ] Inspector: labels, field widths, long names.
- [ ] Login screens: layout at 1100 x 760 and at a small window.
- [ ] Colors: risk levels are told apart without color alone.
- [ ] Icon license: `crates/aegis-shell/ui/icons/LICENSE-lucide.txt` is
      present and the About window credits it.

---

## 6. Architecture check

- [ ] `docs/architecture.md` Part II matches the code: every struct, enum,
      table and RPC listed exists, and nothing important is missing.
- [ ] The guideline map (Part II section 9) is true for each row.
- [ ] Crate graph: no new dependency direction (for example
      `aegis-client` must not depend on `aegis-server` outside tests).

---

## 7. Known gaps (do not report as new findings)

- Company SSO for customers is in the backlog, waiting for an order
  (decision D10).
- Password reset by e-mail and MFA are not planned for password login.
- No screen reader path for canvas items (G14).
- `vendor.keys` and `update.keys` hold no production key yet (G13).
- One server instance per database (ADR 0035).
