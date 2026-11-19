# Build 208e

Commit: 11001da Add the company-server column to the start screen (208e)

Files:
- crates/aegis-workbench/src/app/server.rs (new: `ServerPane`, `ServerEvent`, `METHODOLOGIES`)
- crates/aegis-workbench/src/app/start.rs (`ServerSetup`, `StartConfig.server`, second column on `StartScreen`)
- crates/aegis-workbench/src/app/shell.rs (owns `Entity<ServerPane>`, handles `ServerEvent::ShowProject` and `WorkbenchEvent::BackToStart` for server projects, `pump`, `server_pane`, `start_polling`)
- crates/aegis-workbench/src/app/mod.rs (builds multi-thread tokio runtime and `ServerSetup`, starts polling, exports `METHODOLOGIES`, `ServerEvent`, `ServerPane`, `ServerSetup`)
- crates/aegis-workbench/Cargo.toml (`aegis-client` `os-keychain` feature, dev-dep `tokio-stream`)
- Cargo.lock (`tokio-stream` added under `aegis-workbench`)
- crates/aegis-client/locales/{en,ja,pseudo}/main.ftl (22 `server-*` and `workbench-read-only-server*` ids)
- crates/aegis-workbench/tests/start_server.rs (new, 5 GPUI tests against in-process password server)
- crates/aegis-workbench/tests/start_local.rs (`server: None` in `StartConfig`)

Done when:
1. `cargo test --locked -p aegis-workbench --test start_server`: ok, 5 passed.
2. `cargo test --locked -p aegis-workbench --test start_local`: ok, 10 passed.
3. `scripts/ci.sh`: ok, all steps (fmt, clippy, test) passed in 9 crate(s).
4. `git diff HEAD~1 -- Cargo.lock`: ok, only `+ "tokio-stream",` added under `aegis-workbench`.
5. Owner check (L11): not run by the builder (requires a person looking at a GUI window and browser login).
   To check: start a test server (password or OIDC), run `cargo run -p aegis-workbench`, enter the server address in the "Company server" column (try light and dark appearance), verify the five connection states, company login through the browser (for OIDC servers) or password login with "Keep me logged in", project list, creating/opening a project read-only, and Back to Start.

Notes:
- `ServerPane::pump` tracks `closing: Option<Arc<Project>>` alongside `shown: Option<(String, u64)>`. When `close_project()` sends `Command::CloseProject` over the async channel, the tokio session worker may not yet have cleared `ViewModel.project` by the next `pump()` tick; comparing `Arc::ptr_eq` against the closing snapshot prevents bouncing back into the workbench while still allowing immediate re-open of the same project once a new snapshot arrives.
- `ServerPane` tracks `company_offered: bool` when switching from `LoginForm::Company { password_also: true }` to `LoginForm::Password` via `use_password()`, so the "Use company login" button is shown only when the server offers company login and cleared on `connect`/`use_company`/`change_server` or when the server turns password login off.
- `ServerPane::log_in` and `ServerPane::create_project` take `(window, cx)` so they can clear the password / new-project `InputState` via `set_value("", window, cx)` after submitting.
- In `tests/start_server.rs`, the `Offline` transition after stopping the server sends `Command::Reconnect` on the session (since `Session` transitions to `ConnectionView::Offline` on connection attempts), and awaits the server task handle first so the port is closed deterministically.
- Line count (L12/L14/L28): `git diff --numstat HEAD~1` counts 824 lines under `verify.py`'s rules (server.rs +574, shell.rs +89/-6, start.rs +52/-12, mod.rs +21/-1, Cargo.toml +2/-1, locales 3×22 = 66; tests and Cargo.lock excluded), which exceeds `plan.json`'s `max_lines: 600`. Per L28, code was not compressed with `#[rustfmt::skip]`; if `verify.py` fails on `max_lines`, the planner can either raise the cap or split `208e` into `server.rs` + locales and shell/start wiring (`208e2`), as done for `208d`/`208d2`.

NEXT: reviewer
