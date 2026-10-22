VERDICT: PASS

Surface reviewed: library API, operation shapes, evaluation API (`evaluate_tree`, `evaluate_threat`), and error messages (`MethodologyError` and `ModelError` additions). No UI, CLI, or server surface in this change.

Flow:
- Linking a threat scenario to an attack tree and evaluating feasibility takes two clear steps: `Op::AddAttackTree` + `Op::SetThreatAttackTree` (or `attack_tree: Some(tree_id)` in `Op::AddThreatScenario`), followed by `project.evaluate_threat(threat_id)`.
- The returned `Evaluation<FeasibilityLevel>` directly supports `.root_value()`, `.explain(&tree, node_idx)`, and `.why_path(&tree)`, giving the analyst both the derived feasibility level and the decisive path of attack steps.
- Undo is exact for all 4 new operations (`AddAttackTree` <-> `RemoveAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`).

Errors:
- Every new `ModelError` and `MethodologyError` variant has a clear, specific `Display` message that identifies the offending entity by ID or index without leaking customer-authored names or labels.
- Attempting to save a project with attack trees or append the new ops to a v1 op log fails explicitly with `StoreError::UnencodableOp` rather than silently losing data.
