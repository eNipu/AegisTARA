# Build 182c

Commit: 4218fc8 Add the aegis-workbench crate with GPUI dependencies, licence rules, themes and the sample (182c)

Files:
- Cargo.toml, Cargo.lock, deny.toml (taken unchanged from factory/182c-full)
- crates/aegis-workbench/Cargo.toml (taken)
- crates/aegis-workbench/src/theme.rs, src/sample.rs (taken)
- crates/aegis-workbench/themes/light.json, themes/dark.json (taken)
- crates/aegis-workbench/tests/theme.rs, tests/sample.rs (taken)
- crates/aegis-workbench/src/lib.rs (full build's file without `pub mod app;`, its cfg line and the blank line before them)
- crates/aegis-workbench/src/main.rs (stub `fn main()` for every target: eprintln and exit(1))

Done when:
1. `cargo build --locked -p aegis-workbench`: passes.
2. `cargo test --locked -p aegis-workbench`: passes (theme 4, sample 2).
3. `scripts/deny.sh licenses bans sources`: exit 0, every crate ok, aegis-workbench included.
4. `git diff --quiet factory/182c-full HEAD -- <taken files>`: exit 0, so the taken files are identical.
5. `git diff --numstat c886292 HEAD -- . ':!Cargo.lock' ':!crates/aegis-workbench/tests'`: 265 lines, under 360.

Before you commit: `cargo fmt --all` (changed no file), `scripts/ci.sh crates/aegis-workbench` (fmt, clippy and test ok), the build, the tests, the deny run, both `test -f` and the `git diff --quiet` all pass.

Notes:
- Base: the branch was at 42d6e73 (the full build). I moved it back to c886292 with `git reset --keep c886292` instead of `--hard`, because the auto-mode classifier refused `--hard`. The worktree was clean, so the result is the same. Branch factory/182c-full still points at 42d6e73.
- Licence exceptions (exact versions from Cargo.lock): option-ext@0.2.0, dwrote@0.11.5, cbindgen@0.28.0 (MPL-2.0); enum-iterator@2.3.0, enum-iterator-derive@1.5.0 (0BSD). The owner approved them: open question 1 in spec.md, answered yes in rework.md on 2026-11-18. No other unlisted licence came up.
- Bans: aegis-workbench is the only wrapper for gpui-component and gpui-pre-platform. Wrappers for gpui-pre, from `cargo tree -i gpui-pre -e normal --depth 1 --target all`: aegis-workbench, gpui-base, gpui-component, gpui-kit-assets, gpui-pre-apple, -ios, -linux, -macos, -platform, -web, -wgpu, -windows.
- Ignored advisories, dated 2026-11-18. All four are "unmaintained" notices for crates that only the GPUI dependencies bring in:
  - RUSTSEC-2024-0384 instant (via gpui-component, gpui-base)
  - RUSTSEC-2024-0436 paste (via gpui-component)
  - RUSTSEC-2025-0134 rustls-pemfile (via gpui-component > gpui-kit-assets > gpui-pre-reqwest)
  - RUSTSEC-2026-0206 rustybuzz (via gpui-pre, and gpui-component > resvg > usvg)
- chrono (RUSTSEC-2026-0543) is deliberately not ignored. Slint also brings it in, and `scripts/deny.sh advisories` already fails on it without this slice. Advisories are not a check of this slice. The planner should decide on chrono for the whole repo.
- Contrast: tests/theme.rs checks `contrast(#4F5D6B, #FFFFFF)` against 6.75 ± 0.01. The design doc says 6.5:1, but the WCAG formula gives 6.75, so the doc value is wrong. The test is left as it is. Accent #2F6DB5 on white gives 5.28, which matches the documented 5.3.
- Not taken here, as the prompt says: src/app/*, tests/window.rs and the aegis-client locale changes. They belong to 182c2, which also replaces main.rs and puts back the `app` module in lib.rs.

NEXT: reviewer
