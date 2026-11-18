# Spec — 182-workbench-skeleton

## Open questions

1. **Licensing: may the GPUI dependency set ship with two review-first licenses?** Adding `gpui-pre` 0.3.12, `gpui-pre-platform` 0.3.12 and `gpui-component` 0.9.1 to the product workspace brings in crates that the D5 allow list in `deny.toml` does not cover: **MPL-2.0** in `option-ext` 0.2.0 (all platforms), `dwrote` 0.11.x (Windows fonts) and `cbindgen` 0.28.0 (build time only), and **0BSD** in `enum-iterator` 2.3.0 and `enum-iterator-derive` 1.5.0. D5 lists MPL-2.0 as "review first". ADR 0066 checked only for GPL-only licenses and missed these.
   - **Recommended: yes, as one `exceptions` entry per crate and version in `deny.toml`, each with a comment "approved for the GPUI workbench, job 182".** MPL-2.0 is file-level copyleft: we ship these files unmodified, so the only duty is to say where their source is (crates.io) in the third-party notices. 0BSD is more permissive than MIT. Per-crate entries (not the allow list) keep any other MPL crate from slipping in later.
   - If no: GPUI cannot ship, slice 182c cannot be built, and the job stops after 182b (both client-only slices are still useful to any toolkit).

## Title
Workbench skeleton: the `aegis-workbench` crate with docks, the project explorer, the command palette, one command table, light and dark themes and keyboard paths (WBX-02, WBX-03, WBX-04, WBX-10, WBX-11)

## Design Reference
- Backlog line: `factory/backlog.md` line 15.
- Requirements: `docs/design/workbench-breakdown.md` group 5 (WBX-02, WBX-03, WBX-04, WBX-10, WBX-11), job row 182.
- Explorer: `docs/design/project-explorer.md` sections 2 (behaviour), 3 (tree), 8 (palette), 9a (window frame).
- GPUI plan: `factory/jobs/207-gpui-spec/spec.md` sections 2 and 4.
- Toolkit: `docs/decisions.md` D37, D40, D43; `docs/adr/0066-workbench-shell-toolkit.md`; spike `spikes/gpui-shell/` (pinned versions, dock and window code to learn from, not to copy wholesale).
- Licenses: `docs/decisions.md` D5; `deny.toml`.
- Code touched:
  - `crates/aegis-client/src/{command_table.rs,palette.rs,explorer.rs}` (new), `crates/aegis-client/src/{lib.rs,settings.rs}`, `crates/aegis-client/locales/{en,ja,pseudo}/main.ftl`, `crates/aegis-client/tests/{command_table.rs,palette.rs,explorer.rs}` (new)
  - `crates/aegis-workbench/` (new crate), root `Cargo.toml`, `Cargo.lock`, `deny.toml`
- Not touched: `aegis-shell` and every `.slint` file (no new Slint work), `aegis-model`, `aegis-server`, the snapshot and workspace file formats.

## Problem & Goal
Every Slint screen is being replaced by GPUI (owner, 2026-11-18). Nothing GPUI exists in the product yet; the spike was throwaway. This job builds the frame every later screen job (187, 188, 189, 191, 195, 203, 208, 209) plugs into: one window with docks, a project explorer that shows the whole project as one pgAdmin-style tree, a palette that reaches any element, one command table every menu, toolbar and shortcut reads, and the new palette as light and dark themes.

Business value: a consultant can open a project and get to any asset, threat or tree in two keystrokes, in a calm window that looks like an engineering tool. Each later screen job only adds a tab, so the GPUI app grows by small, reviewable jobs, and Slint can be retired (192). Logic stays in `aegis-client` (207 rule: a GPUI view holds no logic), so it is tested without a window and survives any later toolkit change.

## Acceptance Criteria

### Command table (WBX-11), slice 182a
1. `aegis_client::command_table::commands()` returns one static table; each entry has an id, a group, an icon key, an optional context, optional keys (macOS spelling, optional other-platform spelling) and a palette flag. Label and tooltip are Fluent ids derived from the id (`cmd-<id>`, `cmd-<id>-tip`).
2. A test fails when a command has an empty icon key, a label or tooltip id missing from any of the en, ja and pseudo catalogs, a duplicate id, or two commands with the same keys in the same context on the same platform.
3. `shortcut_label(keys, mac)` gives `⌘K`, `⇧⌘Z`, `⌥⌘B` on macOS and `Ctrl+K`, `Ctrl+Shift+Z`, `Ctrl+Alt+B` elsewhere; checked against hand-written strings.

### Palette search (WBX-04), slice 182a
4. `aegis_client::palette::entries(ws, catalog, mac)` lists every palette command and every element (from `pages::palette_elements`) with its visual ID, name, kind and, for threat scenarios, its catalog IDs (for example `R155 4.3.2`).
5. `palette::search(entries, query, limit)` matches visual ID, name, catalog ID and command label, case-insensitively, ranked: exact visual ID, visual ID prefix, catalog ID prefix, name prefix, word prefix inside the name, substring; ties keep table order. Tested on a fixture with one element of every kind, with each expected order written by hand.
6. Any element of the fixture is found by its visual ID as the first result (a loop over every element, the expected target taken from the element itself, not from the entry list).

### Explorer model (WBX-03), slices 182b (items 7-9) and 182b2 (items 10-12)
7. `aegis_client::explorer::ExplorerIndex::build(ws, names, catalog, project_name)` computes counts and lint marks once per change; `index.rows(&state)` gives the visible rows in O(visible rows).
8. The tree has: the project root; Overview; Project definition → Parties; Shared analysis → damage scenarios, security controls, goals, claims; Systems → one node per system (in system order, colour slot 1 to 6 by position) with the same folders in the same order: Item definition (Components, Interfaces, Data flows, Trust boundaries), Assets, Damage scenarios ⇢, Threat scenarios, Attack trees, Security controls ⇢, Risk treatment, Goals and claims ⇢, Problems; plus a "Not in a system" node with the same folders, shown only when a system-level element belongs to no system. Folder names for element kinds come from the methodology's element names, so a pack's own words show.
9. Every folder shows its count, 0 included (never hidden). An element appears in a system folder when its `SystemUse` set contains that system; in "Not in a system" when its set is empty. A test with one element of every kind in two systems and one unassigned checks every count with its own hand count.
10. A folder carries the worst lint mark (error over warning) of the findings whose target lies in it; the mark also shows on every ancestor. Tested with one error and one warning.
11. Keyboard: Down and Up move the selection over visible rows; Right expands a folder or moves to its first child; Left collapses or moves to the parent; a filter string shows only matching elements (visual ID or name) with their ancestors expanded.
12. `explorer::drop_edit(project, dragged, target)` returns `Edit::MoveToSystem` for a top-level component dropped on a system or on "Not in a system", and `None` for every other pair. A test applies it, checks the component's system, undoes, and compares the saved project bytes with the bytes before.

### Workbench window (WBX-02, WBX-10), slice 182c
13. New crate `crates/aegis-workbench` (binary `aegis-workbench`), a workspace member. GPUI dependencies are declared only for macOS and Windows targets; on other targets the binary prints one line and exits 1, and its pure modules still build and test (CI runs on Ubuntu).
14. `scripts/deny.sh licenses bans sources` passes, with the exceptions of open question 1 and a `[bans]` entry that keeps `gpui-pre` and `gpui-component` out of every crate but `aegis-workbench`.
15. The window opens the workbench sample (the client sample plus two systems with one component each, built through `Edit`s) with a top bar, the explorer dock on the left, a tab area with an Overview tab, an inspector dock on the right (placeholder text), a bottom dock (placeholder text) and a status bar.
16. Themes: `themes/light.json` and `themes/dark.json` hold the tokens of `project-explorer.md` section 8 and the six system colours. A test checks every token parses, and that ink, ink-muted and accent each reach a contrast of at least 4.5:1 on surface in both themes, with the ratio computed by the WCAG formula (and the formula checked once against the documented 6.5:1 of ink-muted on white).

### Explorer, tabs, drag and layout memory (WBX-02, WBX-03), slice 182d
17. Dock open state and sizes are saved per workspace key in the app settings file (`Settings::workbench_layouts`, at most 16 remembered, oldest dropped) and restored on the next start. Tested without a window.
18. The explorer dock renders `ExplorerIndex::rows` with icons, counts (grey at 0), marks, `⇢` for linked folders and the system colour square. Enter or a click opens a tab: a folder opens a list tab (visual ID and name of its elements, read-only), an element opens an element tab (visual ID, name, kind); opening the same node again focuses the existing tab.
19. Dragging a component onto a system or "Not in a system" applies `drop_edit` through `Workspace::apply`; `⌘X` on a component then `⌘V` on a system does the same from the keyboard; `⌘Z` and `⇧⌘Z` undo and redo it. The explorer counts update after each.

### Palette, menus and keys (WBX-04, WBX-10, WBX-11), slice 182e
20. `⌘K` (Ctrl+K) opens the palette over the window; typing filters with `palette::search`; Up, Down, Enter pick; Escape closes. Picking an element selects it in the explorer and opens its tab; picking a command runs it.
21. Every key binding, every menu item (with its icon before the label) and every toolbar button (with its tooltip) is built from `command_table::commands()`; nothing else spells a shortcut. A test maps every command's icon key to a GPUI icon and every command id to a handler, and fails on a command with neither.
22. A shortcut list (command `show-shortcuts`) lists every command with its platform label.
23. Headless GPUI tests (macOS and Windows only) drive with the keyboard alone: open the palette, type a visual ID, Enter, and check the explorer selection and the open tab; and move a component with `⌘X`/`⌘V`, then undo.

### Owner checks after the merge (L11: an agent cannot click or capture the screen)
24. The window opens, docks collapse and resize, the layout comes back after a restart, light and dark look right, drag and drop of a component works, the palette feels instant. Recorded by the owner in the job report.

## Decisions
- **182b split in two.** The first build of 182b (commit 88ef378) was 730 lines against the 600 cap; 182b keeps the tree, counts and rows, 182b2 adds marks, filter, keyboard moves, reveal and the drop rule from the same commit.
- **Workspace member, target-gated GPUI.** The crate lives in `crates/` so `scripts/ci.sh` and `scripts/deny.sh` cover it; GPUI dependencies are under `[target.'cfg(any(target_os = "macos", target_os = "windows"))'.dependencies]` because the Ubuntu CI job has no window libraries and the desktop app ships for macOS and Windows. Checked: the GPUI set resolves next to Slint in one lock file (1,047 packages, offline).
- **Binary `aegis-workbench`, not `aegis`.** `aegis` would collide with today's shipped app until 192 retires Slint; 192 renames it.
- **Opens the built-in sample, not a folder.** Opening and saving a workspace folder is job 164, not done yet. The skeleton keeps the project in memory; edits go through `Edit::to_ops` and `Workspace::apply` (D43), and each edit is confirmed locally at once (`next_submission` then `on_accepted`), as there is no server.
- **Only today's element kinds get folders.** Configurations, Base library, Misuse cases, Circumvent trees, Traceability, Reports and Versions arrive with their jobs (203, 209, 210, BLK); a folder with no model behind it would be a promise the app cannot keep.
- **Folder marks are lint marks.** Risk colours stay as labelled badges in grids (189); a coloured dot in the tree would be read as a risk.
- **Layout = docks only.** Open tabs are not restored; the Overview tab opens on start (explorer doc 3.1). Tabs come back once tabs have real editors (189).
- **Drag and drop = components between systems.** Dropping a base tree on a system needs the base library (new job) and dropping a leaf on a node needs the tree tab (187).
- **No rename or delete in the explorer yet.** F2 and Delete need the editors and the undo toast; they come with 189.
- **The controller audit in 207 section 3 is not part of this job.** The skeleton needs nothing from `aegis-shell/src/controller.rs`: palette elements, counts, problems and edits already live in `aegis-client`. Each screen job moves what it needs.
- **Screen readers:** accepted as weaker for now (D37); no accessibility work here.
- **Performance budgets on `large` are measured in 187 and 189**, where the canvas and grids exist; the explorer index is O(project) per change and the visible rows O(visible).
- **Advisories:** an "unmaintained" or "unsound" notice that enters only through the GPUI set may be ignored in `deny.toml` with a dated comment naming the crate and the path, as was done for Slint; a vulnerability advisory stops the slice.

## Out of scope
- Opening and saving workspace folders (164), sign-in and project list (208), grids and cell editing (189), the attack tree canvas (187), the inspector content (188), the bottom dock content (191), versions (203), settings windows (209, 213).
- Removing or changing anything in `aegis-shell` or the `.slint` files.
- Screenshots in five variants and the performance bench (later screen jobs).
- Windows and RDP measurement (its own backlog line).
