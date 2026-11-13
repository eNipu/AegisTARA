# Build 122da

Commit: 5c0433a Read and write through and type in the attack tree outline (122da)
Files:
- crates/aegis-client/src/outline.rs
- crates/aegis-client/tests/outline_notes.rs (new)

Done when:
1. `cargo test --locked -p aegis-client --test outline_notes`: 9 passed.
2. `cargo test --locked -p aegis-client --lib outline`: 6 passed; `cargo test --locked -p aegis-client --test tree_edit --test library_insert`: 8 + 7 passed.
3. `git diff --numstat master -- crates/aegis-client/src`: 228 added, 13 removed (241 lines, under 300).

Before you commit: `cargo fmt --all` clean; `scripts/ci.sh crates/aegis-client` reports fmt, clippy and test ok; `cargo check --workspace --all-targets` builds.

Notes:
- `read_tail` checks for a key that was already taken before it checks for an empty value. So `A | type: a | type:` gives `DuplicateNote`, not `UnknownNote`. The prompt does not say which comes first.
- `NoteOnGate` is checked line by line inside the parse loop, so the error reported is still the one on the earliest line.
- The writer works out a node's tail once, on its first line, and reuses it on later lines. So `note` is called once per step, and every line of a shared step gets the same tail.
- The writer does not check the values themselves. A value of `-`, an empty value, or one that contains ` | ` would not read back the same. Values are IDs and keys, so this does not come up today. The next slice may want to refuse such values when it applies them.
- The existing unit tests in `outline.rs` are unchanged and pass.

NEXT: reviewer
