VERDICT: CHANGES

# Code Review - 113-platform-fidelity (Round 2)

The spec has no `## Checks` section, so I ran the standard set from the worktree (rework.md and the engineering checklist).

## Check results
- `cargo fmt --all -- --check` -> pass
- `cargo clippy -p aegis-shell -p aegis-client --all-targets -- -D warnings` -> pass
- `cargo test --workspace --locked` -> pass (1596 passed, 0 failed)
- `scripts/deny.sh` -> pass on rerun (exit 0, all 9 crates ok). The first run printed "1 of 9 crate(s) failed" while a cargo test run was going; two reruns, and `scripts/deny.sh licenses bans sources`, were clean. Likely the advisory-db fetch, but I could not reproduce it.
- Spike, Windows build, and live OS-appearance following: not runnable here (owner-run, hardware-gated). See "Acceptance criteria still open".

## Findings (must fix)

1. `crates/aegis-shell/src/shortcuts.rs:~212` (`install`) - The description editor hint is now `format!("Plain text. {} saves, Escape cancels.", ...)`, a Rust string that bypasses gettext. The old `@tr("Plain text. Ctrl+Enter saves, Escape cancels.")` is gone from `app.slint`, but its msgid is still in `translations/{ja,pseudo}/.../aegis-shell.po:597`. The ja and pseudo locales now show an English hint, and the catalog entry is orphaned. That is an L10N-01/02 regression (AC10: only fonts, chrome and modifier labels may differ). Fix: keep the sentence translatable with the key as an argument, for example `@tr("Plain text. {} saves, Escape cancels.", root.save-description-key-label)` in `app.slint` with a string property from `keys_for_label`. Update the msgids in both catalogs, and add a ja assertion to `tests/i18n.rs`.

2. `crates/aegis-shell/ui/app.slint` Edit menu (Redo) / `shortcuts.rs` table - The Redo accelerator was `@keys(Control + Y)` on master. It is now `Control+Shift+Z` on every OS, because the table row is `["Control","Shift","Z"]`. Ctrl+Y no longer redoes on Windows/Linux. AC10 says keyboard behaviour must not change. The table on master already advertised Ctrl+Shift+Z while the menu bound Y, so this fixes a mismatch but removes a working key. Fix: record the change as a deliberate decision in the ADR with the owner's consent, or keep Ctrl+Y working on non-mac targets. Cover it with a test either way.

## Findings (should fix, simplicity)

3. `shortcuts.rs:~196` and `app.slint:224` - `mac-shortcuts` is declared, set in `install`, and never read by any `.slint` file. This is dead code the spec did not ask for. Fix: delete the property and the `set_mac_shortcuts` call.

4. `crates/aegis-shell/src/platform.rs` (`JAPANESE_FALLBACK`) - Only a test reads it. The build does not apply Hiragino Sans or Yu Gothic UI anywhere, so AC8's fallback rests entirely on Slint's OS font lookup, and nothing proves it. The `.SF NS` / `.SF NS Mono` family names are also unverified on a real macOS window (build.md admits no macOS Japanese or pseudo screenshots). If Slint cannot resolve a hidden `.SF NS` family, it falls back silently to the default font. Fix: either remove the constant and state "OS fallback" in the ADR, or apply it. Capture the macOS screenshot and record which font was actually drawn.

5. `shortcuts.rs` (`MAC_MODIFIERS` / `OTHER_MODIFIERS`, `"Meta"` / `"Super"` / `⌃`) - No table row uses Meta. Remove it, or keep it only if a row needs it (YAGNI).

6. `main.rs:149-181` (`reexec_on_software`) - The retry spawns a child while the parent still holds the tokio runtime, the launcher and any open standalone project (`AEGIS_STANDALONE`). A file lock on the project folder would make the child fail. The child also reuses a login that the parent may already have started (dev login). It is acceptable for the main case (the window fails before anything is opened), but drop the launcher and runtime before spawning, or document the limitation. The child's exit status is mapped to `Err(...)` with the parent's error text, which hides the child's real failure.

## Standards
- D1/D2/D7/P4: not touched by this job. Shortcut and palette output is table-ordered, so it is deterministic.
- A8/A9: `format_label` and `keys_for` do linear scans of 17 rows, called at install time only (not an interactive path). The doc comments carry no big-O line. Add one-liners if the checklist requires them for new `pub fn`.
- R1: `keys_for_label`/`keys_for` re-scan the table per id (17 calls). That is fine, but one loop over `table()` would be simpler.
- `slint_keys` has a `# Panics` section (a library `panic!`). The table test (`every_id_is_unique_and_every_binding_parses`) covers it. The panic is sound but unneeded; `unwrap_or_default` as in `keys_for` is enough.
- Renderer: `renderer::parse` is total and tested. `AEGIS_RENDERER=software` skips Skia selection with no fallback log line. An unknown value logs one warning. A Skia selection failure logs one line and retries once. The post-show failure path is a real retry, not a silent downgrade.
- `build.rs` uses `CARGO_CFG_TARGET_OS` and is cfg-tested via `platform::widget_style`.
- Appearance: `Appearance` enum and settings round-trip are tested. The `tests/appearance.rs` Light/Dark repaint and headless-software tests are real. Theme tokens now read the palette, and the splash/About obsidian tokens are unchanged.

## Acceptance criteria still open
- AC1 (spike on a Retina Mac and a Windows laptop; ADR 0063 stays `proposed`; the evidence table stays `TBD`). rework.md says this is still required. It is owner-run and gates merge.
- AC4: offline build is proven on macOS aarch64 only (ADR 0063 records the checksum and measured times, with nothing invented). The Windows build and the archive licence notice are open.
- AC6: no test exercises live OS following; check it by hand. The `System` expectation in `tests/appearance.rs` only asserts the headless platform picks one of the two palettes.
- AC8/AC10: macOS Japanese and pseudo screenshots, and the UI-07.8 menu bar gap (record in the ADR if unsupported), are not yet shown.

## Verdict reason
Findings 1 and 2 are behaviour regressions against AC10 (untranslated hint; lost Ctrl+Y). Fix them, then rerun the checks. The remaining open criteria are owner-gated (hardware spike, Windows build) and must be resolved before merge, but they are not code defects.
