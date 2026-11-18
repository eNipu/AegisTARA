Commit: 05607c2 Convert an old standalone project folder into workspace files beside it (164c)
Files:
- Cargo.lock (+2: `aegis-server`, `tokio` in the `aegis-workbench` dependency list)
- crates/aegis-workbench/Cargo.toml (+2)
- crates/aegis-workbench/src/lib.rs (+1, `pub mod convert;` outside the GPUI cfg)
- crates/aegis-workbench/src/convert.rs (new, 87 lines)
- crates/aegis-workbench/tests/convert.rs (new, 140 lines)
- docs/architecture.md (+5, new "Conversion" paragraph at the end of II.4.7)

Done when:
1. `cargo test --locked -p aegis-workbench --test convert` → 3 passed, 0 failed
2. `scripts/deny.sh licenses bans sources` → ok for every crate
3. `git diff --stat HEAD~1 -- Cargo.lock` → 1 file, 2 insertions, both inside the `aegis-workbench` entry; no new package

Before you commit: `cargo fmt --all` clean; `scripts/ci.sh crates/aegis-workbench` → fmt, clippy, test ok.

Notes:
- The `block_on` runs one private `async fn read_old` (open_existing, list(1), methodology check, load); the store is dropped inside it, on every path. The runtime is dropped before `Loaded::rebuild` and `WorkspaceFolder::convert_beside`, so `.aegis.lock` is free.
- Error texts: read/rebuild failures get the prefix "cannot read the old project folder: "; "the folder holds no project"; the methodology mismatch text as specified. `convert_beside` errors are passed through as `to_string()` without the prefix, since they concern writing the new files, not reading the old folder.
- The version-refusal test hits only the new guard (L09): without it, the snapshot (saved with the real methodology) would rebuild fine and `aegis.json` would be written; the test also checks the message names `0.0-test`.
- The old-file check covers `manifest.json`, `ops.jsonl`, `ops.meta` and every file in `snapshots/` (and asserts at least one `.aegis` snapshot exists).
- Section II.2's crate graph shows no `aegis-workbench` node, so no edge was added there.
- Source lines (without tests, docs, lock): about 90.

NEXT: reviewer

## Rework 1
Commit: 7fbdd03 Cut folder values shown in the methodology refusal message (164c)
Files: crates/aegis-workbench/src/convert.rs, crates/aegis-workbench/tests/convert.rs
Fixed:
- convert.rs refusal message: `methodology_key` and `methodology_version` read from the old manifest now go through `shown()`, which keeps at most 64 chars and adds `…` when cut (`SHOWN_CHARS = 64`).
- New test `a_long_methodology_key_is_cut_in_the_message`: a folder with a 10 000-char key is refused with a message under 300 bytes that contains `…`, and no `aegis.json` is written. To build it, the test helper `old_folder` now calls `old_folder_keyed(dir, m, key, version)`. The `0.0-test` assertion still passes.
Done when:
- `cargo fmt --all`: ok
- `scripts/ci.sh crates/aegis-workbench`: all steps passed
- `cargo test --locked -p aegis-workbench --test convert`: 4 passed
- `scripts/deny.sh licenses bans sources`: ok
- Cargo.lock unchanged in this commit
NEXT: reviewer
