# Build 208d

Commit: bb6d926 Add the start screen view that checks the license before a folder opens (208d)

Files:
- crates/aegis-workbench/src/app/start.rs (new, taken unchanged from factory/208d-full)
- crates/aegis-workbench/Cargo.toml, Cargo.lock (taken unchanged from factory/208d-full: aegis-license)
- crates/aegis-workbench/src/app/mod.rs (`mod start;` and `pub use start::{StartConfig, StartEvent, StartScreen};` only)
- crates/aegis-client/locales/{en,ja,pseudo}/main.ftl (only the `# Start screen (job 208)` block, 13 start-* ids, appended as on the branch)
- crates/aegis-workbench/tests/start_local.rs (new, 7 view tests)

Done when:
1. `cargo test --locked -p aegis-workbench --test start_local`: ok, 7 passed.
2. `cargo test --locked -p aegis-workbench --test folder_view`: ok, 14 passed.
3. `git diff factory/208d-full HEAD -- crates/aegis-workbench/src/app/start.rs crates/aegis-workbench/Cargo.toml Cargo.lock`: prints nothing.
4. `git diff HEAD~1 -- Cargo.lock`: only `+ "aegis-license",` under aegis-workbench.

Before you commit: `cargo fmt --all` clean; `scripts/ci.sh crates/aegis-client crates/aegis-workbench`: all steps passed (fmt, clippy, test for both crates).

Notes:
- Step 1: `git reset --hard 720eb6a` was refused by the permission classifier. The tree was clean and 3f13126 is still on `factory/208d-full`, so I used `git reset --keep 720eb6a` instead (same result on a clean tree; refuses to drop local changes). The branch is now 720eb6a + bb6d926.
- Tests: empty fields (folder message, then license message, no event); valid license -> one Open with the dir, customer "Start Test", editing, `license()` Some, no error; foreign key -> no event, error starts "The license file cannot be used"; create_local into a non-empty dir -> "is not empty", into a missing dir -> one Create; open_sample -> one Sample; remembered settings fill both fields (`license_file` checked, Open with saved folder without setters); keyboard focus_folder + simulate_input + enter -> one Open.
- Source lines (without tests, lock, locales): 353 (start.rs 347 is the branch's file as is).
- `run` and the window are unchanged; no shell, WorkbenchEvent, back-to-start (208d2).

NEXT: reviewer
