Commit: 76d3b8a Open, create and save a workspace folder on disk (164a)
Files:
- crates/aegis-client/src/folder.rs (new, 467 lines including docs)
- crates/aegis-client/src/lib.rs (+1: `pub mod folder;`)
- crates/aegis-client/tests/folder.rs (new, 14 tests)

Done when:
1. `cargo test --locked -p aegis-client --test folder`: 14 passed, 0 failed.
2. `scripts/ci.sh crates/aegis-client`: fmt ok, clippy ok, test ok ("all steps passed in 1 crate(s)").
Also ran `cargo fmt --all` (clean). Source lines changed: 468 (cap 480; the count includes doc comments).

Notes:
- `open` checks `aegis.json` before it takes the lock, not after as step 9 lists. Taking the lock first creates
  `.aegis.lock` in any folder the user picks by mistake; checking first leaves a non-workspace untouched.
  Result and error variants are the same.
- The three caps are checked while `collect` walks (in a small `Found` helper), so a folder with millions of
  entries is refused once the count passes `MAX_WORKSPACE_FILES` instead of after listing all of them. No file
  is read before `collect` returns. `open` also re-sums the bytes actually read against
  `MAX_PROJECT_FILE_BYTES` (files may grow after `collect`); `read_capped` handles the per-file cap.
- Root-level entries that are neither `*.json` (minus `manifest.json`) nor `shared`/`systems` are ignored even
  when they are symbolic links; a root `*.json` link, a `shared`/`systems` link, or any link below them →
  `Symlink`. Non-regular files (fifos, sockets) under the walked folders are skipped.
- `convert_beside` refuses when `aegis.json` exists in any form (`symlink_metadata`), and gives `Io` when
  `root` does not exist.
- `create` and `convert_beside` share a private `start` (lock, empty `written`, `save`).
- Known gap, not in scope: a file could be swapped for a symlink between `collect` and `File::open`
  (no `O_NOFOLLOW` without a new dependency or `libc`).
- Refusal tests each plant one bad input in an otherwise valid workspace and assert the exact variant (L09);
  TooDeep and Symlink also assert the reported path.

NEXT: reviewer
