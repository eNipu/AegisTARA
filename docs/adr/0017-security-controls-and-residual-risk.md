# 0017. Security controls, risk treatment, and residual risk evaluation

Date: 2026-04-20
Status: Accepted

## Context

In ISO/SAE 21434 Clause 15.9, after inherent risk is determined for each threat scenario, the analyst makes a risk treatment decision (`avoid`, `reduce`, `share`, or `retain`). When reducing risk, the analyst defines security controls (`SecurityControl`) and attaches them to attack steps in the attack tree that realizes a threat scenario.

A security control only reduces overall threat risk if it covers the critical attack paths:
- On an `Or` gate (attacker chooses the easiest branch), mitigating only one branch leaves alternative unmitigated branches open, so the control is **circumvented** and the gate's residual feasibility shifts to the easiest unmitigated branch (`why_path` automatically pivots to the circumventing branch).
- On an `And` or `SequentialAnd` gate (attacker must complete every child step), mitigating any single required step lowers the gate's residual feasibility to that bottleneck (`MaxMin` minimum across children).

Designing this capability requires deciding:

1. How `RiskTreatment`, `SecurityControl`, and per-step control mitigations (`StepMitigation`) are represented in `aegis-model`.
2. How multiple controls mitigating the same attack step combine with the step's inherent feasibility to form its residual feasibility.
3. How inherent and residual attack-tree and risk evaluations coexist without duplicating tree topology or mutating inherent ratings.
4. How referential integrity, single-op undo (`Op` inverses), and decode-time caps (`Limit`) are enforced on snapshots and op logs.

## Decision

1. **`RiskTreatment` on `ThreatScenario` (`Op::SetRiskTreatment`).**
   - `RiskTreatment` is a 1-byte enum with variants `Avoid`, `Reduce`, `Share`, and `Retain` (serialized in `snake_case`).
   - `ThreatScenario` stores `treatment: Option<RiskTreatment>`, mutated via `Op::SetRiskTreatment { threat, treatment, provenance }`.
   - `Op::RemoveThreatScenario` requires `treatment == None` (`ModelError::ThreatHasRiskTreatment { threat, treatment }`) so `Op::AddThreatScenario` remains its exact single-op inverse.

2. **`SecurityControl` arena (`ControlId`, `ControlIdx`).**
   - `ControlIdx` and `ControlId` are generated via `entity_ids!`.
   - `SecurityControl` holds `(id: ControlId, name: Arc<str>, provenance: Provenance)` in a `Project` arena (`controls: Arena<ControlId, ControlIdx, SecurityControl>`) with `RefCounts::controls: Vec<u32>` counting step-mitigation attachments across all attack trees.
   - `Op::AddSecurityControl`, `Op::RenameSecurityControl`, and `Op::RemoveSecurityControl` manage lifecycle; `RemoveSecurityControl` rejects controls with `refs > 0` via `ModelError::SecurityControlInUse { control, steps }`.

3. **Per-step control mitigations (`StepMitigation`, `MAX_STEP_MITIGATIONS = 16`, `Op::SetStepMitigation`).**
   - `StepMitigation` is an 8-byte `Copy` pair `(ControlIdx, FeasibilityLevel)`.
   - Each `TreeEntry` stores `step_mitigations: Vec<Box<[StepMitigation]>>` indexed by `NodeIdx::index()`, sorted by `ControlIdx` (and sorted by `ControlId` on the wire). At most `MAX_STEP_MITIGATIONS = 16` controls may mitigate a single step (`ModelError::TooManyStepMitigations`, `Limit::StepMitigations { max: 16 }`).
   - `Op::SetStepMitigation { tree, node, control, feasibility, provenance }` sets, updates, or clears (`feasibility: None`) a control's mitigated feasibility ceiling on a step node, updating `RefCounts::controls` and the tree's provenance, and returning the prior `Op::SetStepMitigation` as its exact inverse.
   - `Op::RemoveAttackTree` requires a tree to have zero step mitigations (`ModelError::AttackTreeHasMitigations { tree, count }`) so `Op::AddAttackTree` remains its exact single-op inverse.

4. **Residual step feasibility and dual inherent/residual evaluation.**
   - Each control mitigation on a step acts as an upper bound (ceiling) on the attacker's feasibility for that step:
     $$\text{residual}(s) = \min\!\left(\text{inherent}(s),\; \min_{m \in \text{mitigations}(s)} m.\text{feasibility}\right)$$
   - When a step has no mitigations (or only mitigations whose ceiling exceeds the step's inherent feasibility), its residual feasibility equals its inherent feasibility.
   - `Project::residual_step_feasibility(tree, node)` computes this in $O(c)$ time ($c \le 16$).
   - `Project::evaluate_residual_tree(tree)`, `Project::evaluate_residual_threat(threat)`, and `Project::evaluate_residual_risk(threat)` evaluate the same `AttackTree` topology and `MaxMin` domain using `residual_step_feasibility` at leaf steps (or falling back to `threat.direct_feasibility()` when no attack tree is linked).

5. **Snapshot and op-log persistence.**
   - `aegis-project` snapshots serialize `security_controls` (sorted by `ControlId`, omitted when empty), `treatment` on threats (omitted when `None`), and `step_mitigations: { "<node_id>": { "<control_id>": "<feasibility_key>" } }` on attack trees (omitted when empty; empty outer or inner maps are rejected at decode time).
   - Decode-time caps enforce `Limit::StepMitigations { max: 16 }` on each step's control map and `Limit::AttackTreeNodes { max: 32 }` on the outer node map before `validate` runs.

## Consequences

- Analysts can compare `Project::evaluate_risk(threat)` (inherent) and `Project::evaluate_residual_risk(threat)` (residual) side by side, together with their `Evaluation::why_path` traces, immediately seeing whether a security control reduces risk or is circumvented by an unmitigated `Or` branch.
- Memory per step with no mitigations is one empty `Box<[]>` header (16 bytes on 64-bit), keeping all 19 `benches/store_memory.rs` gates passing.
- All v1 snapshot and op-log fixtures remain byte-for-byte compatible.
