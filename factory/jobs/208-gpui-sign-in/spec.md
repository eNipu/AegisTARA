## Title

208 Sign-in and project list in GPUI

## Design Reference

- Backlog line: `factory/backlog.md` line 17 ("Sign-in and project list in GPUI ... server choice, company sign-in, user name and password, standalone folder and license, the five connection states, the project list and new workspace").
- `factory/jobs/207-gpui-spec/spec.md` section 1, rows 1 and 2 (screen 1 "Sign in: company, password, on this computer", screen 2 "Projects: list, lease holder, new project"), section 2 (a GPUI view holds no logic; logic lives in `aegis-client`), section 3 (connection, password and OIDC sign-in exist in `connection.rs`, `transport.rs`; project list exists), section 4 (each screen gets headless keyboard tests).
- The flow being replaced: `crates/aegis-shell/src/launcher.rs` (Slint launcher, 795 lines: server choice, `GetLoginOptions`, password / company / development login, keychain restore, standalone start), `crates/aegis-shell/src/provider.rs` (credential providers), `crates/aegis-shell/src/standalone.rs` lines 36-80 (license check before a standalone project opens), `crates/aegis-shell/src/controller.rs` lines 3165-3183 (the five connection codes: 0 Connecting, 1 Online, 2 Offline, 3 Login required, 4 Choose server).
- Data: `crates/aegis-client/src/state.rs` (`ConnectionView`, `ProjectEntry`, `ProjectView`), `crates/aegis-client/src/session.rs` (`Session`, `on_opened` lines 1509-1580), `crates/aegis-license/src/license.rs` (`License::verify`, `LicenseState`).
- GPUI window: `crates/aegis-workbench/src/app/` (jobs 182, 164, 164e).
- Decisions: D2 (company login through the system browser), D22 (standalone app), D23 (read-only users see a saved version), D42 (methodology chosen once).

## Problem & Goal

The GPUI workbench opens only the in-memory sample or a folder given on the command line. It has no way to sign in to a company server, no license check, no project list and no way to start a new workspace, so the Slint app is still needed for every real start. This job gives the GPUI app its first screen: "On this computer" (a workspace folder plus the signed license, or a new workspace, or the sample) and "Company server" (address, company login, user name and password, the five connection states, the project list, new project, open).

Business value (P7): this is the door into the product. A consultant opens a customer's workspace with the customer's license; a company user signs in with company credentials and sees which projects exist. Without it, no customer can be moved off Slint (job 192). The license check protects revenue: a workspace folder can only be opened in GPUI with a valid signed license.

## Acceptance Criteria

1. The credential providers (`AppProvider`, `OidcProvider`, `PasswordProvider`, `secrets()`) live in `aegis_client::provider`; the Slint app uses them from there and still builds and passes its tests. Check: `cargo build --locked -p aegis-shell` and `scripts/ci.sh crates/aegis-shell`.
2. A toolkit-free sign-in view model maps every `LoginOptions` to a login form, every `ConnectionView` to one of the five stages, and every sign-in error code to a catalog text. Check: `cargo test --locked -p aegis-client --test signin_view`.
3. A toolkit-free `SignIn` controller, against an in-process password server, goes: no remembered address → Choose server; connect → Login required (password form); wrong password → the wrong-password text; right password → Online with the project list, and the address and user name are remembered in the settings file; Change server → Choose server; a development server → the development form → Online; a bad address → Choose server with an error. Check: `cargo test --locked -p aegis-client --test signin`.
4. `check_license` accepts a valid signed license (editing allowed), a license in grace (editing allowed, days left), an expired one (editing not allowed), and refuses a wrong signature, a not-yet-valid license and an empty file, each with its own text. Check: `cargo test --locked -p aegis-client --test license_file`.
5. An open server project exposes the version it was downloaded as (`ProjectView::snapshot`), which does not include local edits until a reload. Check: `cargo test --locked -p aegis-client --test session snapshot`.
6. A read-only workbench refuses undo, redo, drag and drop, Save and Save As with its reason in the status bar, and the project stays byte-equal; the Open Folder command is refused without a checked license. Check: `cargo test --locked -p aegis-workbench --test read_only`.
7. The start screen opens a folder with a valid license into the workbench, opens it read-only with an expired license, stays on the start screen with the error for a bad license, creates a new workspace folder, opens the sample without a license, remembers folder and license in the settings file, and "Back to start" returns to it (asking first when there are unsaved changes). Typing a folder path and Enter opens it. Check: `cargo test --locked -p aegis-workbench --test start_local`.
8. The start screen, against an in-process password server: address and Enter connect, the password form logs in, the project list shows the server's projects, New project creates one and the workbench shows it read-only with the project name, Back to start closes it and shows the list again, Sign out returns to the login form. Check: `cargo test --locked -p aegis-workbench --test start_server`.
9. Every new text is in `crates/aegis-client/locales/{en,ja,pseudo}/main.ftl` with a hyphenated id. Check: `cargo test --locked -p aegis-client i18n`.
10. The whole gate passes and no third-party crate is new to `Cargo.lock`. Check: `scripts/ci.sh` and `git diff master -- Cargo.lock` shows only workspace crates' dependency lists.

Owner checks (cannot be run by an agent, lesson L11): the start screen in light and dark at 1440 x 900; a real company login through the browser against the test server on Railway.

## Decisions

- **GPUI standalone opens workspace folders directly** (the job 164 path) after a local license check; no embedded loopback server as in Slint. Reason: 164 already reads and writes the folder without a server; a server adds nothing for one user.
- **Expired license opens read-only; a bad, unsigned, not-yet-valid or unreadable license is refused.** Reason: `LicenseState::Expired` documents "reading stays possible", and a customer must never be locked out of their own files.
- **A server project opens read-only in the GPUI workbench**, from the version the session downloaded (D23 saved version); its reason names the server and the lease holder if someone edits. Editing a server project in GPUI (lease, edits and undo through the `Session`) is a follow-up job, to be added to the backlog after this one. Reason: the workbench edits a local `Workspace`; routing edits through the session is its own design and would double this job.
- **The lease holder shows after opening** (in the read-only reason), not in the list. Reason: `ProjectSummary` carries no holder; adding one is a protocol change.
- **`SignIn` is a new toolkit-free port of the launcher flow in `aegis-client`; `launcher.rs` stays as is until Slint is retired (192).** Only the providers move, so the keychain and OIDC code exist once. Reason: rewiring the Slint launcher is Slint work the owner ruled out.
- **New local workspace uses the methodology the workbench carries (ISO/SAE 21434 + R155).** Reason: choosing a methodology and template is job 204. New server project offers the three built-in methodologies, as Slint does.
- **The GPUI app polls `SignIn::pump` every 50 ms** while it shows the start screen or a server project. Reason: simplest correct bridge from tokio to the GPUI thread (P3); tests call `pump` directly.
- **The sample opens without a license; the Open Folder command inside the workbench needs a checked license.** Reason: the sample is the demo; every real folder goes through the license.
- **A folder given on the command line opens through the same license check** with the remembered license, else the start screen shows it with the error.
- **Keychain access is injected (`Secrets`)** so tests never touch the OS keychain.

## Out of scope

- Editing server projects in GPUI (lease, edits, undo through the session): follow-up job.
- New project in three steps from a template (204); a methodology choice for local workspaces.
- Lease holder in the project list (protocol change).
- Administration, accounts, change password (213).
- Recent folders beyond the one remembered folder.
- Any change to `launcher.rs` logic, `standalone.rs`, `aegis-server` or the protocol.
