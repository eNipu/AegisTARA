# Review requirements

This page lists what must be reviewed before the work on this branch is
accepted. The owner asked for the build first and the review after. All completed jobs (`027`–`041`, `045`–`057`, `060`–`068`, `070`–`074`, `080`, `089`, `107`–`111`) have now been reviewed; see `factory/jobs/<job-id>/` and `docs/security-and-architecture-review.md` for full architectural and security findings.

## Pending reviews

One row per job. A cell says `pending` until that review has run and passed.
Add a row for every new job.

| Job | Code | Security | UX | UI |
|---|---|---|---|---|
| 027 workspace and README (`027-workspace-and-readme`) | PASS | PASS | PASS | N/A (PASS) |
| 028 decision cleanup (`028-decision-cleanup`) | PASS | PASS | PASS | N/A (PASS) |
| 029 server skeleton (`029-server-skeleton`) | PASS | PASS | PASS | N/A (PASS) |
| 030 server persistence (`030-server-persistence`) | PASS | PASS | PASS | N/A (PASS) |
| 031 login (`031-login`) | PASS | PASS | PASS | N/A (PASS) |
| 032 canvas engine (`032-canvas-display-list`) | PASS | PASS | PASS | PASS |
| 033 client core and shell (`033-client-core-and-slint-shell`) | PASS | PASS | PASS | PASS |
| 034 license tokens (`034-license-tokens`) | PASS | PASS | PASS | N/A (PASS) |
| 035 live observers (`035-live-observers`) | PASS | PASS | PASS | PASS |
| 036 release scaffolding (`036-release-scaffolding`) | PASS | PASS | PASS | N/A (PASS) |
| 037 Postgres TLS (`037-postgres-tls`) | PASS | PASS | PASS | N/A (PASS) |
| 038 HTTPS client (`038-https-client`) | PASS | PASS | PASS | N/A (PASS) |
| 039 seat ledger persistence (`039-seat-ledger-persistence`) | PASS | PASS | PASS | N/A (PASS) |
| 040 password login and gRPC-Web (`040-password-login-and-grpc-web`) | PASS | PASS | PASS | PASS |
| 041 TARA pages (`041-tara-pages`) | PASS | PASS | PASS | PASS |
| 045 password login switch (`045-password-login-switch`) | PASS | PASS | PASS | PASS |
| 046 server image in CI (`046-image-ci`) | PASS | PASS | PASS | N/A (PASS) |
| 047 numbers checked mark (`047-numbers-check-mark`) | PASS | PASS | PASS | PASS |
| 048 update manifest fetch (`048-update-manifest-fetch`) | PASS | PASS | PASS | PASS |
| 049 admin events and license (`049-admin-events-and-license`) | PASS | PASS | PASS | PASS |
| 050 PDF report (`050-pdf-report`) | PASS | PASS | PASS | PASS |
| 051 screen reader path (`051-screen-reader-path`) | PASS | PASS | PASS | PASS |
| 052 event log everywhere (`052-event-log-everywhere`) | PASS | PASS | PASS | N/A (PASS) |
| 053 PDF tree pictures (`053-pdf-tree-figures`) | PASS | PASS | PASS | PASS |
| 054 installer download (`054-installer-download`) | PASS | PASS | PASS | PASS |
| 055 session admin (`055-session-admin`) | PASS | PASS | PASS | PASS |
| 056 UI fixes and monkey tests (`056-ui-fixes-and-monkey-tests`) | PASS | PASS | PASS | PASS |
| 107 standalone mode (`107-standalone-mode`) | PASS | PASS | PASS | PASS |
| 108 remove live view (`108-remove-live-view`) | PASS | PASS | PASS | PASS |
| 110 deny dev policy (`110-deny-dev-policy`) | PASS | PASS | N/A (PASS) | N/A (PASS) |
| 064 element descriptions (`064-element-descriptions`) | PASS | PASS | PASS | PASS |
| 065 tree node ops (`065-tree-node-ops`) | PASS | PASS | PASS | PASS |
| 057 attack tree library (`057-attack-tree-library`) | PASS | PASS | PASS | PASS |
| 109 pack names and likelihood (`109-pack-names-and-likelihood`) | PASS | PASS | PASS | PASS |
| 111 EU CRA pack (`111-cra-pack`) | PASS | PASS | PASS | PASS |
| 060 login hardening (`060-login-hardening`) | PASS | PASS | PASS | N/A (PASS) |
| 061 database hardening (`061-database-hardening`) | PASS | PASS | PASS | N/A (PASS) |
| 062 event log integrity (`062-event-log-integrity`) | PASS | PASS | PASS | N/A (PASS) |
| 063 small hardening (`063-small-hardening`) | PASS | PASS | PASS | N/A (PASS) |
| 089 suggestion branches (`089-suggestion-branches`) | PASS | PASS | PASS | PASS |
| 066 i18n foundation (`066-i18n-foundation`) | PASS | PASS | PASS | PASS |
| 067 CJK fonts and formats (`067-cjk-fonts-and-formats`) | PASS | PASS | PASS | PASS |
| 068 Japanese translation (`068-japanese-translation`) | PASS | PASS | PASS | PASS |
| 070 report document model (`070-report-doc-model`) | PASS | PASS | PASS | PASS |
| 071 report builder (`071-report-builder`) | PASS | PASS | PASS | PASS |
| 072 XLSX export (`072-xlsx-export`) | PASS | PASS | PASS | PASS |
| 073 figures (`073-figures`) | PASS | PASS | PASS | PASS |
| 074 item definition (`074-item-definition`) | PASS | PASS | PASS | PASS |
| 080 Excel TARA import (`080-excel-import`) | PASS | PASS | PASS | PASS |
| 058 project access (`058-project-access`) | PASS | PASS | pending | pending |
| 112 catalog packs (`112-catalog-packs`) | PASS | PASS | N/A (pending) | N/A (pending) |
| 069 content language (`069-content-language`) | pending | pending | pending | pending |
| 081 command palette (`081-command-palette`) | pending | pending | pending | pending |

- **Branch:** `claude/inspiring-clarke-phmv4u`
- **Base for the diff:** `master` (`git diff master...claude/inspiring-clarke-phmv4u`)
- **Jobs in scope:** 027 to 041, and 045 to 051 (`factory/backlog.md`; the added jobs are listed in section 7)
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
- [ ] **Dialogs and undo (UI-10, UI-12):** a removal or unlink happens at once
      and shows a toast with Undo; Undo and Redo are icon buttons at the left of
      the top bar whose tooltip names the action and the shortcut ("Undo remove
      asset 'Brake ECU' (⌘Z)"); a confirmation dialog exists only for what Undo
      cannot reverse (delete project, break another user's lock, reset or change
      a password, end sessions, import that replaces data); a dialog names the
      thing in its title, its main button says the action (not "Yes"), a
      destructive main button is red and not the Enter default, Escape cancels;
      errors never open a dialog (field error inline, action error as a sticky
      toast). A new dialog is added to `crates/aegis-shell/tests/dialogs.rs`
      with a reason.

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

---

## 7. Jobs 045 to 051 (built after the first list)

What each added job needs from each review, besides the lists above.

### Code

- [ ] 045: `Accounts::set_password_login` stores before it flips the flag; a
      refused login costs no hashing; `HybridAuthn` routes on the `ats_`
      prefix only. `AEGIS_PASSWORD_LOGIN` is read once at start.
- [ ] 047: `compare_server_risks` runs on both triggers (server numbers and
      confirmed batch); `NumbersCheck::Checked` needs the confirmed change.
- [ ] 048: `fetch_manifest` limits (https only, 32 KiB, no redirects).
- [ ] 049: `ListEvents` cap and admin check; `audit.rs` covers every
      `AuditKind`.
- [ ] 050: `pdf.rs` layout loops end (a cell taller than a page, a word wider
      than a column, `MAX_PAGES`); output is deterministic.
- [ ] 051: `access::visible` stays O(visible); `describe` handles an invalid
      node.

### Security

- [ ] 045: only an admin can change the switch; turning it off ends every
      password session; no lock-out without a company login.
- [ ] 048: transport is not trust (a wrong signature is refused over valid
      TLS); `with_tls_config` is not used with weaker settings anywhere.
- [ ] 049: the event log holds no password or token; a non-admin is refused.
- [ ] 050: the PDF contains only what the project holds (names, text), and
      the font file license is in the repository.
- [ ] 046: the customer image refuses to start without a license
      (`deploy/verify-image.sh`; its docker lines were first run by CI).

### UX

- [ ] 045: the login screen with and without the password form; the message
      after an admin turns it off while someone is on the form.
- [ ] 047: the wording of the four states of the numbers mark; very large
      projects stay on "Checking".
- [ ] 049: the Administration page is taller than 760 px on the users tab.
- [ ] 051: **hands-on with a screen reader** (Orca on Linux, NVDA on Windows,
      VoiceOver on macOS): reach the canvas with Tab, walk the tree with the
      arrow keys, hear the sentence, press Enter on an entry. The tests do
      not run an accessibility backend (ADR 0045).

### UI

- [ ] 047: the numbers mark in the project header at 1100 px width.
- [ ] 049: the event table columns and the tab buttons.
- [ ] 050: open `target/report-samples/tara-report.pdf` (made by
      `cargo test -p aegis-client --test pages`) and check page 1 to the end.

---

## 8. Self-review notes (2026-10-26)

Not one of the four formal reviews. A first pass by the builder over the
back-fixes from the other session, with what was run.

| Check | Result |
|---|---|
| `scripts/ci.sh` on all eight crates (fmt, clippy `-D warnings`, tests), rustc 1.99.1, PostgreSQL 16 | passed |
| Plain builds (no dev-dependencies) with clippy for license, model, proto, auth, canvas, client, server | passed (the `aegis-license` fix from `9886fd4` is sound) |
| `scripts/deny.sh`, `tools/deny-fixtures/run.sh` (9 cases) | passed |
| 048 `fetch_manifest`: https only, 1 MiB body cap and no redirects in `aegis-auth` `HyperClient`, no trust from transport | as documented |
| 045 switch: only an admin; sessions end; `authenticate` checks the switch on every request | as documented, **one finding** below |
| 054 download in a shared temp folder: a planted symlink or file could be written through | fixed (`create_new` part file, a private 0700 folder per download); the tests fail on the old code, ADR 0048 section 7 |

Finding fixed (job 052 commit): a login that was already hashing when an admin
turned password login off could still store a session. It was unusable while
the switch was off but became valid again when it was turned on. `log_in`
now checks the switch under the same lock the switch uses to clear sessions;
test `a_login_racing_the_switch_leaves_no_session` fails without the fix.

Not run here: the formal code, security, UX and UI reviews (G8), the docker
lines of `deploy/verify-image.sh` (Docker Hub refused the pull with 429; they
run in CI), and any screen reader pass (G14).

---

## 9. UX and UI self-review (2026-10-26, job 056)

A first pass by the builder, not the formal factory reviews (those stay
`pending` in the table at the top). Method: every page and login screen from
`target/shell-screenshots/` at 1360 x 860, then the main pages at the
smallest window the app allows (980 x 600) and at 1100 x 760, then a random
"monkey" run (`crates/aegis-shell/tests/monkey.rs`) that fires every window
callback with hostile values while the window changes size.

### Fixed in this job

| # | Where | Problem | Fix |
|---|---|---|---|
| F1 | Administration, company login servers | The three tab buttons grew into huge blocks and the Login panel sank to the bottom | Tab row has a fixed height; a filler takes the spare room |
| F2 | Every table in a narrow panel | The style gives a table a 400 px minimum width, so the attack tree list lost its Rating column behind the canvas | `min-width: 0` in the shared `Table`; columns share the width they get |
| F3 | Reports | Text still said "open it in a browser and print to PDF" next to a PDF button | Reworded |
| F4 | Administration, users table | Six columns in about 430 px: headers and cells showed "..." | Fixed widths for the short columns, side panel 300 px |
| F5 | Threat scenarios at 1100 and 980 | The detail panel ran off the right edge (the add form needed 620 px) | Add form in two rows; side panels follow the window width |
| F6 | Attack trees at 1100 and 980 | The canvas shrank to 90 to 200 px between two 340 px panels | Panels follow the window width (list 240 to 340, inspector 260 to 340); the list shows Tree and Rating |
| F7 | Threat panel | Catalog reference "Add" button clipped; a long field label cut; "Risk before" and "Risk after" headers shown as "Ris..." | Row split in two, labels wrap, headers "Before" and "After" |

### Open, for a design pass (none blocks use)

| # | Where | Observation |
|---|---|---|
| O1 | Every table | The style paints every second row's text in its "secondary" grey (about 5.6 to 1 on the sheet, so it passes AA) which makes those rows look disabled. A custom table widget would fix it, with keyboard and screen reader behaviour to keep |
| O2 | Undo and Redo | A disabled button has a heavy outline while an enabled one has none, so a disabled Redo looks focused |
| O3 | Sidebar at 600 px high | "Administration" and "Reports" are below the fold; a thin scroll bar is the only hint |
| O4 | Goals and claims | The goals and claims tables share the height 50 / 50 and leave a gap between them |
| O5 | Risks at 980 | Seven columns need 694 px, so the table scrolls sideways |
| O6 | Notices | Info notices stack at the bottom right and cover content (the last History row) until dismissed; consider timing them out |
| O7 | Whole app | No hands-on pass with a screen reader and no keyboard-only walk through every page yet (G14) |

### Does it crash?

`tests/monkey.rs` fires the window's callbacks (all but log out, change
server and the four exports, which the page tests cover; retry, the login
buttons, ending the edit session and refilling the sample are fired rarely or
in a controlled way, see the test) with random values:
empty and 10 000 character text, control and right to left characters,
outline text with errors, indexes that do not exist (negative, huge), floats
that are NaN or infinite, pages and window sizes changed at random (240 x 160
to 3000 x 1400), over a real server. A "persistent user" step re-opens a
project, retakes the edit lock and refills an empty project, so the run keeps
reaching the model: **123 to 377 of the storm's own edits per 4 000 steps got
through and were saved**, and the test fails if none do. The editor test also
checks that the app still answers and draws after the storm. In the first
version of the test the monkey deleted its own project within 40 steps and
tested an empty app; the trace and the "edits reached the server" check found
that. The default run is 300 steps and 2 seeds; use `AEGIS_MONKEY_STEPS` and
`AEGIS_MONKEY_SEEDS` for a soak.

`tests/small_windows.rs` draws eight pages at 980 x 600, 1100 x 760 and
(below the supported minimum) 800 x 600 and 640 x 480 and keeps the pictures.
