# Build 182c2

Commit: 04fbe75 Add the workbench window with docks, panels, theme switching and the window test (182c2)

Files (all taken unchanged with `git checkout factory/182c-full -- ...`):
- crates/aegis-workbench/src/lib.rs
- crates/aegis-workbench/src/main.rs
- crates/aegis-workbench/src/app/mod.rs
- crates/aegis-workbench/src/app/workbench.rs
- crates/aegis-workbench/src/app/panels.rs
- crates/aegis-workbench/tests/window.rs
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl

Precheck: HEAD before the slice was 4218fc8 `... (182c)` on top of c886292; `git diff HEAD factory/182c-full --stat` named exactly the 9 allowed files.

Done when:
1. `cargo build --locked -p aegis-workbench`: pass.
2. `cargo test --locked -p aegis-workbench`: pass (sample 2, theme 4, window 1).
3. `git diff --quiet factory/182c-full HEAD`: exit 0 (branch equals the full build).
4. `git diff --numstat HEAD~1 HEAD -- . ':!crates/aegis-workbench/tests'`: 435 added, 1 removed (under 580).
5. Owner check: `cargo run -p aegis-workbench` showing the window with the four docks in light and dark cannot be done by an agent (L11); left for the owner.

Before you commit: `cargo fmt --all` changed nothing in the taken files; `scripts/ci.sh crates/aegis-workbench crates/aegis-client` all steps passed (fmt, clippy, test for both crates).

Notes:
- `lib.rs` adds `#[cfg(any(target_os = "macos", target_os = "windows"))] pub mod app;`; `main.rs` calls `aegis_workbench::app::run()` on macOS and Windows, and elsewhere prints `aegis-workbench runs on macOS and Windows` and exits 1.
- `app/mod.rs`: `run()` opens one 1400 x 900 window titled "AegisTARA" with a `Root` over `Workbench`; `apply_appearance` calls `Theme::change` or `Theme::sync_system_appearance`, then `apply_tokens` through `Theme::update`; with the System setting the window follows OS appearance changes via `observe_window_appearance`.
- `app/workbench.rs`, `app/panels.rs`: `ExplorerPanel` (rows of `index.rows(&state)` indented by depth with counts; 182d replaces it), `OverviewTab` (project name and `Project::*_count()` with the pack's plural names), `InspectorPanel` and `BottomPanel` from a small `text_panel!` macro. Docks: left 280 px, right 320, bottom 200. Status bar shows `Methodology::label()` and `workbench-status-unsaved`.
- Fluent ids under `# Workbench window (job 182)` in en, ja, pseudo: `workbench-overview`, `workbench-inspector-empty`, `workbench-bottom-empty`, `workbench-status-unsaved`, `workbench-explorer` "Explorer", `workbench-inspector` "Inspector", `workbench-bottom` "Output".
- The GPUI test context works: `tests/window.rs` hosts the full `Workbench` (gpui-component `DockArea` and panels) through `TestAppContext::add_window_view` after `gpui_component::init`, draws the window, and checks the explorer shows the same rows as `index.rows(state)` (more than 3) and the sample has 3 components. It passed (0.04 s). Slices 182d and 182e can rely on it.
- No criterion unmet apart from the owner check.

NEXT: reviewer
