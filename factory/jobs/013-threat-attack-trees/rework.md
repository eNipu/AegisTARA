## Answers

The owner answered the spec's open questions on 2026-10-22:

1. **Attack tree identity and sharing:** confirmed. Attack trees are first-class project entities in their own `Project` arena (`TreeId`, `TreeIdx`), referenced by `ThreatScenario` via `attack_tree: Option<TreeIdx>` with reference counting (`RefCounts.trees`) and `ModelError::AttackTreeInUse` on removal while any live threat references the tree.
2. **Step feasibility rating in v0:** confirmed. Each methodology declares an ordered `feasibility_levels: Vec<LevelDef>` scale (`2..=16` levels, lowest feasibility first, indexed by `FeasibilityLevel(u8)`), each leaf step holds an optional `FeasibilityLevel`, and gates combine child levels with `MaxMin<FeasibilityLevel>` (`OR` = max = most feasible child, `AND` = `SAND` = min = least feasible child). Multi-factor scoring tables belong to a later job.
3. **Operations and persistence scope:** confirmed. Add `AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, and `RemoveAttackTree`; update the methodology pack format for `feasibility_levels` now; defer snapshot/op-log wire encoding of attack trees to a follow-up store job, refusing `Project::save` when `attack_tree_count() > 0` and `OpLogWriter::append` on the four new ops with `StoreError::UnencodableOp` so nothing is silently dropped and golden v1 project/oplog fixtures stay unchanged.
