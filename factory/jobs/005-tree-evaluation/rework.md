## Answers

The owner answered the spec's open questions on 2026-10-20:

1. **Crate:** confirmed. The evaluator goes in `crates/aegis-model/src/eval.rs`, uses only the tree's public API, and can move to `aegis-engine` later.
2. **Empty gates:** confirmed. Return `ModelError::EmptyGate(NodeId)`. The domain trait has no neutral elements.
3. **Missing leaf values:** confirmed. Return `ModelError::MissingLeafValue(NodeId)`, never a default.
4. **Shared subtrees:** confirmed. Evaluate each node once (A2) and document the limit for sum and probability domains. Exact handling (BDD) is out of scope.
