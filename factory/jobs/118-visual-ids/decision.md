# Decision — 118-visual-ids

## Decision
APPROVE

Ship human-readable visual IDs on every project element (FND-06, FND-07).

## Key choices
- Prefix scheme `A / D / T / AT / SC / G / CL / C / IF / DF / TB`, zero-padded
  to at least 2 digits and growing past `99`.
- Monotonic per-type assignment with no reuse after deletion; undo restores the
  removed element's original number, and `Project::eq` compares visual numbers.
- Persistence stores only non-default visual IDs (omitting `num == pos`) so v1
  golden fixtures stay byte-for-byte identical; the per-type high-water counter
  is rebuilt on load and duplicate numbers are rejected.
- The command palette matches element visual IDs by prefix (and names by
  substring), so `T00` and `AT00` never collide in search.

## Checks
- `cargo fmt --all -- --check`, `scripts/ci.sh` for `aegis-model`,
  `aegis-client`, `aegis-shell`, `cargo test --workspace --locked`, and
  `scripts/deny.sh` all pass.
