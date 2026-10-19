# Factory backlog

`/factory next` runs the first unchecked item. Add one feature per line.

- [x] Create the `aegis-model` crate: arena-based attack tree with typed `NodeIdx(u32)` IDs and a UUIDv7-to-index map (standards D1–D3) (001-aegis-model)
- [x] Add cycle detection to `aegis-model` with Kahn's algorithm, reporting the cycle path as an error (A3) (002-cycle-detection)
- [x] Add a `cargo-deny` config that enforces the license policy from decision D5 (003-cargo-deny)
- [ ] Benchmark storage for linked parents in `aegis-model`: `Vec<NodeIdx>` vs `Box<[NodeIdx]>` vs a sparse `BTreeMap<NodeIdx, Vec<NodeIdx>>` for shared nodes only, at the large and stress reference sizes (memory with `dhat`, `link`/`parents()` time with `criterion` or `divan`); keep all three as a lesson benchmark (P2, L2) and record the result in an ADR
