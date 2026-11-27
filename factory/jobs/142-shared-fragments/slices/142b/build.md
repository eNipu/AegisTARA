Commit: 40fe73d97a91132f07a951cd2f6753b74cf40d8a Prepare fragment slots and validated dependency graph (142b)
Files:
- crates/aegis-model/src/blocks.rs
- crates/aegis-model/src/error.rs
- crates/aegis-model/src/lib.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/tests/shared_fragments.rs
- crates/aegis-model/tests/common/shared_fragment_guards.rs (new)
Done when:
1. `cargo test --locked -p aegis-model --test shared_fragments --test ops_properties`: pass (11 + 11 tests).
2. `scripts/ci.sh crates/aegis-model`: pass (fmt, clippy, test all ok). The library tests include the 7 private-setup fragment unit tests (`project::shared_fragments::*`).
Also: `cargo fmt --all` clean.
Notes:
- Production lines changed vs b7b0993 (`git diff --numstat`, src only): 320 (blocks 6, error 46, lib 4, project 264), under the 440 cap.
- StepLinks gains `fragment: Vec<Option<TreeIdx>>`. Both lazy constructors, is_empty, linked_steps, insert/remove links, stable-ID `same_tree` equality and the check_step_links invariants (slot length, at most 256 populated slots, live Fragment target, no gate, no shared/own feasibility/factors/via/attack type) all handle it. Local conditions and mitigations stay legal. tree_invariants also runs `fragment_order()`, and threat_invariants refuses a threat linked to a fragment.
- Public read-only API: `step_fragment`, `step_fragment_id`, `fragment_users` (sorted StepRef), `fragment_order`. Constants `MAX_FRAGMENT_REFERENCES = 256` and `MAX_FRAGMENT_DEPTH = 8` live in blocks.rs and are exported. The seven new ModelError variants are added as specified.
- `check_fragment_graph(candidate)` is private and is meant for reuse by 142c. It builds deduplicated BTreeSet edges keyed by stable IDs, with the candidate overlaid on the source slot. It then runs Kahn with a BTreeSet ready queue (stable-ID ties), so dependencies come first. Depth is computed iteratively in Kahn order over the whole graph, which counts upstream users of the source. A cycle is extracted from the Kahn residual by walking the first remaining dependency until a tree repeats, as in ADR 0002. The walk already follows source->target, so no reversal is needed. The result is rotated to the smallest ID and closed (first == last). None of this recurses.
- Guards: `check_use_fragment` checks a live Fragment target (FragmentRole), an unshared Step source (NotAStep / NodeUsesSharedStep / NodeUsesFragment), provenance via check_provenance, no own ratings (StepIsRated), the per-tree cap of 256 populated source nodes, and then the candidate graph. `check_unuse_fragment` returns NodeNotFragment. `unshared_step_slot` (feasibility/via/attack type/factor setters) and `check_use_shared_step` refuse a fragment source. RemoveTreeNode refuses a source node. RemoveAttackTree refuses an incoming-used fragment (FragmentInUse) and a tree with outgoing references (NodeUsesFragment). SetThreatAttackTree refuses Fragment trees (FragmentRole).
- No Op variant, wire field, public setter or loader change. `set_fragment`, `check_use_fragment` and `check_unuse_fragment` are private and `#[allow(dead_code)]` until 142c wires them. Only the cfg(test) module installs slots.
- The private-setup tests sit in `tests/common/shared_fragment_guards.rs`, mounted into project.rs as `#[cfg(test)] #[path] mod shared_fragments`, so they can reach private setup while the file stays inside the allowed `tests/*`. Integration tests (`--test shared_fragments`) add public checks: exported constants and dependency-first order, the read-only accessors on unreferenced trees, and a refused fragment->threat link that leaves the Project unchanged.
- Covered cases: diamond with parallel edges (dedup, users sorted, candidate checks leave the Project unchanged), self cycle, a cross cycle with an upstream residual user where every reported edge is checked against real slots plus the candidate, depth 8 accepted and depth 9 refused when inserted below upstream users, cap 256/257 and room again after clearing one slot, wrong role/gate/rated/shared/blank-actor guards, all setter guards, incoming/outgoing removal, and stable-ID equality across different arena layouts and node slot insert/remove shifts.
NEXT: reviewer
