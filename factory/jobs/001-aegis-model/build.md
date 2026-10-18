## Round 1
Commits: d3266e1 Add arena-based attack tree model with typed IDs
Files:
- crates/aegis-model/Cargo.toml
- crates/aegis-model/Cargo.lock
- crates/aegis-model/src/lib.rs
- crates/aegis-model/src/id.rs
- crates/aegis-model/src/tree.rs
- crates/aegis-model/src/error.rs
- crates/aegis-model/tests/properties.rs
- docs/adr/0001-arena-and-typed-ids.md

Acceptance criteria:
- [x] AC1 — `cargo build` ok; `grep -n 'forbid(unsafe_code)' src/lib.rs` → line 29; `cargo tree --depth 1 -e normal` → only `thiserror v2.0.21`, `uuid v1.26.1`. Edition 2024, `rust-version = "1.85"` (matches uuid 1.26.1's own MSRV; thiserror 1.77).
- [x] AC2 — `cargo test --doc` → `src/id.rs - id::NodeIdx (line 19) - compile fail ... ok` (passes a `NodeId` to `AttackTree::get`). A companion doctest with `tree.root()` compiles, so the failure is the type mismatch.
- [x] AC3 — `cargo test id::` → 13 passed (v7 accepted, v4 → NotVersion7, nil/max/wrong-variant → NotVersion7, malformed → InvalidUuid, Display lowercase hyphenated, round trip).
- [x] AC4 — `cargo test tree::small_tree` → 2 passed (13 nodes, parent/children for every node, labels, `idx_of(id_of(i)) == Some(i)`).
- [x] AC5 — `cargo test tree::errors` → 5 passed (DuplicateId for step and root IDs, NotAGate, UnknownNode from a larger tree, ChildrenNotEmpty; snapshot of iter/parents/lookups equal before and after, plus internal invariant check).
- [x] AC6 — `cargo test id::capacity` → 3 passed (`Some` at `u32::MAX as usize`, `None` at `+1` and `usize::MAX`, 64-bit only via `cfg(target_pointer_width = "64")`).
- [x] AC7 — `cargo test --test properties` → 2 passed (32 cases, each up to 2,000 ops, ~10% reused IDs, random parents incl. steps; invariants and insertion-order `iter()` checked after every call; failed calls leave `len` unchanged). ~7 s in debug.
- [x] AC8 — `cargo test --release tree::large_tree` → 1 passed (100,000 nodes, every ID resolves to its index), 0.11 s.
- [x] AC9 — `grep -nE 'unwrap\(|expect\(|panic!' src/*.rs` → hits only in `#[cfg(test)]` modules (id.rs ≥ line 167, tree.rs ≥ line 497). Library code has none. The one `// INVARIANT:` comment is in `AttackTree::iter` (a `filter_map` that cannot drop items), no unwrap there.
- [x] AC10 — `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` → no warnings; every `AttackTree` pub fn states time cost.
- [x] AC11 — `test -f docs/adr/0001-arena-and-typed-ids.md` → exists; covers context, options (arena vs Rc<RefCell>/nested; HashMap vs BTreeMap vs FxHashMap; u32 vs usize), decision, consequences.
- [x] AC12 — `git diff --stat master -- spikes/` → empty.

Checks (from crates/aegis-model):
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass (with `#![warn(clippy::pedantic)]`)
- `cargo test` → pass (28 unit, 2 property, 27 doctests)
- `cargo test --release tree::large_tree` → pass
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` → pass

Notes:
- Open questions answered by assumption: crate at `crates/aegis-model/` (standalone, no workspace); no `link`; steps carry only a label.
- Test modules are named `id::parse`, `id::capacity`, `tree::unit`, `tree::small_tree`, `tree::errors`, `tree::large_tree` so the spec's filter strings match.
- Added small helpers beyond the sketch: `Node::gate`, `Node::step`, `Node::label`, `Node::gate_kind`, `Node::children` (all documented and tested).
- `NodeId::from_uuid` also requires the RFC 9562 variant (not just version nibble 7); other variants give `NotVersion7`.
- `FromStr` accepts any form `Uuid::parse_str` accepts (hyphenated, simple, braced, URN, any case); `Display` is always hyphenated lowercase.
- `add_child` error order: UnknownNode, NotAGate, ChildrenNotEmpty, DuplicateId, CapacityExceeded. All checks run before any mutation.
- Capacity: indices go up to `u32::MAX`, so the arena can hold `u32::MAX + 1` nodes before `CapacityExceeded` (one more than the spec's wording "more than u32::MAX nodes"). It follows directly from AC6's `try_from_usize` contract.
- Invariant `by_id.len()` equality is checked by a `#[cfg(test)] check_invariants` used in unit tests; the integration proptest checks the same invariants via the public API.
- `Cargo.lock` is committed for reproducible builds (P4).
- ADR says no criterion benchmark exists yet; HashMap vs BTreeMap benchmark (L2) is follow-up.
