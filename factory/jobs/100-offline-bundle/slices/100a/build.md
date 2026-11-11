# Build 100a

Commit: 7251c9a Sign and verify an offline bundle folder in aegis-license (100a)

Files:
- crates/aegis-license/src/bundle.rs (new)
- crates/aegis-license/src/lib.rs
- crates/aegis-license/src/keys.rs
- crates/aegis-license/src/issuer.rs
- crates/aegis-license/tests/bundle.rs (new)

Done when:
1. `cargo test --locked -p aegis-license --test bundle`: ok, 13 passed (11 tests plus 3 proptests; the two never-panics proptests are separate).
2. `scripts/ci.sh crates/aegis-license`: fmt ok, clippy ok, test ok.
3. `cargo test --locked -p aegis-license`: every test binary ok (17 unit, 13 bundle, 11, 7).
4. `git diff --numstat master -- crates/aegis-license/src`: 370 + 18 + 5 + 4 = 397 lines (cap 400).

Also ran `cargo fmt --all` and `cargo check --locked --workspace` (L03): both clean.

Notes:
- `sha256_file` takes `(path: &Path, rel: &str)`, not just `path`. The extra argument is
  so an I/O error carries the bundle-relative path (`Io(rel)`), not the absolute path.
- Names from disk are checked with `valid_path` before anything else. A name that fails,
  or is not UTF-8, gives `Malformed("a file name")` with no payload. So every `String` in
  an error is a valid bundle path, and the `format!("{:?}", name)` fallback is never
  needed. The only exception is a `read_dir` failure: it gives `Io("./")` or
  `Io("./installers/")`.
- `parse_sums` uses one message for "bad hash / bad path / lists SHA256SUMS or .sig",
  and one for "empty / no final newline". All of these are still `Malformed`; the
  messages are shorter to stay under the line cap.
- The `MAX_BUNDLE_FILES + 2` entry cap counts every directory entry seen, including
  folders and the two skipped top-level files.
- `verify_bundle_dir` reads `SHA256SUMS.sig` with a 64-byte cap; a longer or shorter
  file gives `BadSignature`. `read_regular` checks the length before it reads and reads
  through `take(cap + 1)`, so a file that grows during the read is still refused.
- The Missing/Unlisted check uses `Vec::contains` (O(n^2), with n at most 64).
- To meet the 400-line cap, the build was squashed into a single commit before review.
  The first version was 469 lines, and the trimmed version was amended into that commit.
  No reviewer had seen it.
- `Manifest::release()` uses `expect`, but that is existing code in manifest.rs. It is
  only called after `Manifest::verify`.

NEXT: reviewer
