# Factory backlog

`/factory next` runs the first unchecked item. Add one feature per line.

- [ ] Create the `aegis-model` crate: arena-based attack tree with typed `NodeIdx(u32)` IDs and a UUIDv7-to-index map (standards D1–D3)
- [ ] Add cycle detection to `aegis-model` with Kahn's algorithm, reporting the cycle path as an error (A3)
- [ ] Add a `cargo-deny` config that enforces the license policy from decision D5
