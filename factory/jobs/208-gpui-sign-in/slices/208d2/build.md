# Build 208d2

Commit: 998e06b Open the window on the start screen and add Back to Start (208d2)

Files:
- crates/aegis-workbench/src/app/shell.rs (new: `Screen`, `Shell`)
- crates/aegis-workbench/src/app/mod.rs
- crates/aegis-workbench/src/app/files.rs
- crates/aegis-workbench/src/app/workbench.rs
- crates/aegis-workbench/src/app/commands.rs
- crates/aegis-workbench/tests/start_local.rs
- crates/aegis-client/src/command_table.rs
- crates/aegis-client/tests/command_table.rs
- crates/aegis-client/locales/en/main.ftl, ja/main.ftl, pseudo/main.ftl

All taken unchanged with `git checkout factory/208d-full -- <the 11 files of step 1>`. No hand merge was needed:
before the checkout, `git diff HEAD factory/208d-full --stat` listed only these 11 files, so the 208d commit
had no review changes outside them (start.rs, Cargo.toml, Cargo.lock were already identical).

Done when:
1. `cargo test --locked -p aegis-workbench --test start_local`: ok, 10 passed.
2. `cargo test --locked -p aegis-workbench --test folder_view`: ok, 14 passed.
3. `cargo test --locked -p aegis-workbench --test command_wiring`: ok, 2 passed; `cargo test --locked -p aegis-client --test command_table`: ok, 5 passed.
4. `git diff factory/208d-full HEAD`: prints nothing (0 lines).
5. Owner check (L11): not run by the builder. It needs a person looking at a GUI window; the builder cannot see it.
   To check: `cargo run -p aegis-workbench` should open the start screen (set `appearance` to light, then dark, in
   the settings file); `cargo run -p aegis-workbench -- <folder>` should put the folder in the field and open it
   under the remembered license file (`standalone_license`).

Before you commit: `cargo fmt --all` (no changes), `scripts/ci.sh crates/aegis-client crates/aegis-workbench`
(all steps passed in 2 crates), and the four test commands above all passed.
`tests/palette.rs`, `tests/palette_view.rs`, `tests/command_wiring.rs` were not touched.

Source lines (without tests and catalogs, `git diff --numstat HEAD~1`): shell.rs +179, mod.rs +16/-10,
workbench.rs +13/-2, files.rs +4/-1, command_table.rs +2/-1, commands.rs +1.

Notes (carried from the earlier full build at 3f13126):
- Each `Workbench::new` replaces the window's should-close callback. After Back to Start the old callback's
  weak entity is gone, so the start screen closes at once (no stale unsaved-changes prompt).
- Workbench commands act only while the workbench is shown.
- The shell reads the license path with `StartScreen::license_file(cx)` to remember `standalone_license`.
- `run` builds `StartConfig { keys: TrustedKeys::vendor(), .. }`; the command-line folder goes through
  `Shell::open_path`, i.e. the same license check as the start screen.
- "Back to Start" goes through `Leave::Start`, so unsaved changes are asked about first (164eb behaviour);
  `finish` then emits `WorkbenchEvent::BackToStart`, which the shell handles.

NEXT: reviewer
