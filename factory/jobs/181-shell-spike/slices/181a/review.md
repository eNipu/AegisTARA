VERDICT: PASS
## For the owner
What changed: a new throwaway app in `spikes/gpui-shell/` opens one GPUI window with gpui-component's dock.
The left dock is a project tree, the centre has two tabs ("Attack tree" placeholder and an "Item definition" code editor with 2000 lines), and the right dock is an inspector.
Clicking a tree row updates a shared `Selection`, and the inspector redraws from it. The README lists the licenses of all 760 packages, and none is GPL-only.
Look at:
- Done-when 4 is not proven. The builder could not click, type, drag or take a screenshot because the agent has no Accessibility or Screen Recording permission. You need to run `cargo run --release --manifest-path spikes/gpui-shell/Cargo.toml` yourself, click a tree row and check that the inspector changes, type in the editor tab, and drag a dock splitter. That is the D37 evidence this slice exists for.
- spikes/gpui-shell/src/main.rs:4 — the `dock_panel!` macro writes the four panel traits once. The builder found that every dock panel needs the same ~20 lines of boilerplate, which is a useful finding for the real workbench.
- spikes/gpui-shell/src/shell.rs:40-44 — `set_dock` takes no size, so the dock widths come from separate `set_dock_size` calls. This is easy to miss in the toolkit API.
Risk: low. This is a spike that no production crate uses (D8), and it touches nothing outside `spikes/gpui-shell/`. It adds about 760 third-party crates, but only to this spike's own lock file, not to the root workspace. The license check is recorded in the README: `r-efi` and `self_cell` mention GPL only as an `OR` alternative.
View: git -C factory/worktrees/181-shell-spike diff 60bc411ac15c17a53b734c6acc551dfa4abfdeff..HEAD
## Findings
- None that block. Notes only:
- spikes/gpui-shell/src/project_tree.rs:82,98 — the tree highlights the selected row by comparing labels, so two rows with the same label would both light up. That is fine for this fixed sample but wrong for a real tree. The workbench should compare an ID instead.
- The work is two commits (d27869d, then 2d6c844 for the size rework), but the prompt asked for one. That is acceptable for a rework.
- The unit tests check only what the prompt asked for (the depths, row 0's kind, the line count and the header). They would fail if `sample_outline` or `sample_text` were removed or reordered. The UI wiring has no automated test, which is expected for a spike.
## Lesson
[planner] A "Done when" item that needs clicks or screenshots cannot be met by an agent without macOS Accessibility and Screen Recording permission, so plan it as an owner check from the start.
NEXT: human
