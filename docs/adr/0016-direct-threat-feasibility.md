# 0016. Direct threat feasibility and attack-tree override

Date: 2026-04-20
Status: Accepted

## Context

In early-stage (concept-phase) threat analysis and risk assessment (ISO/SAE 21434 Clause 15.6.2), analysts often assign an attack feasibility rating directly to a threat scenario before decomposing it into a multi-step attack tree. Later in product development, high-risk threats are refined with an `AttackTree` whose bottom-up evaluation derives feasibility from individual attack steps.

Supporting both workflows requires deciding:

1. How direct threat feasibility is stored and mutated on `ThreatScenario`.
2. Which source takes precedence when a `ThreatScenario` has both a `direct_feasibility` rating and a linked `AttackTree`.
3. How `Op::RemoveThreatScenario` preserves exact single-op undo (`Op::AddThreatScenario` inverse) when a threat has a `direct_feasibility` rating.
4. How `direct_feasibility` is persisted in project snapshots and op logs without altering existing v1 fixtures.

## Decision

1. **`ThreatScenario::direct_feasibility` and `Op::SetThreatFeasibility`.**
   - `ThreatScenario` stores `direct_feasibility: Option<FeasibilityLevel>` (1 byte in memory), exposed via `ThreatScenario::direct_feasibility(&self) -> Option<FeasibilityLevel>`.
   - `Op::SetThreatFeasibility { threat: ThreatId, level: Option<FeasibilityLevel>, provenance: Provenance }` sets or clears the direct feasibility rating on a threat scenario, validates `level` against the project's methodology (`ModelError::UnknownFeasibilityLevel`), updates the threat's provenance, and returns the exact prior `Op::SetThreatFeasibility` as its inverse in $O(1)$ time and $O(1)$ space.

2. **Precedence in `Project::evaluate_risk`: linked attack tree overrides direct feasibility.**
   - When `threat.attack_tree()` is `Some(tree_idx)`, `Project::evaluate_risk` evaluates the linked attack tree (`Project::evaluate_threat`) and uses its root `FeasibilityLevel`, populating `ThreatRisk::tree_evaluation` with `Some(eval)`.
   - When `threat.attack_tree()` is `None`, `Project::evaluate_risk` falls back to `threat.direct_feasibility()`, setting `ThreatRisk::tree_evaluation` to `None`.
   - When both `threat.attack_tree()` and `threat.direct_feasibility()` are `None`, `Project::evaluate_risk` returns `Err(ModelError::NoAttackTree(id))`.
   - Linking an attack tree does not erase `direct_feasibility`: if the tree is later unlinked (`Op::SetThreatAttackTree { tree: None, .. }`), the threat scenario cleanly falls back to its stored `direct_feasibility` rating.
   - `Project::evaluate_threat` remains dedicated to evaluating a threat's linked attack tree (`Result<Evaluation<FeasibilityLevel>, ModelError>`).

3. **Referential integrity on `Op::RemoveThreatScenario`.**
   - Just as `Op::RemoveThreatScenario` rejects removing a threat while `attack_tree` is `Some(tree)` (`ModelError::ThreatHasAttackTree`), it rejects removing a threat while `direct_feasibility` is `Some(feasibility)` with `ModelError::ThreatHasDirectFeasibility { threat, feasibility }`.
   - This keeps `Op::AddThreatScenario` as the exact single-op inverse of `Op::RemoveThreatScenario` without adding optional fields to `Op::AddThreatScenario` or breaking v1 op-log compatibility.

4. **Snapshot and op-log persistence.**
   - In `aegis-project` snapshots, `ThreatScenario` serializes `direct_feasibility: "<key>"` (after `damages` and before `provenance`) only when `Some`, omitting the field when `None` so v1 golden snapshots remain byte-for-byte identical. On `Project::load`, `direct_feasibility` is replayed via `Op::SetThreatFeasibility` before applying the threat's final stored provenance.
   - In `aegis-oplog`, `Op::SetThreatFeasibility` encodes as `{"op":"set_threat_feasibility","threat":"...","level":"..."|null,"provenance":...}` (omitting default provenance).

## Consequences

- Concept-phase TARAs can rate threat feasibility and compute `ThreatRisk` immediately without constructing 1-node placeholder attack trees.
- Refining a threat with an `AttackTree` seamlessly overrides its concept-phase `direct_feasibility` with step-level derivations and `why_path` explanations, while unlinking the tree restores the concept-phase rating.
- All v1 snapshot and op-log fixtures remain byte-for-byte compatible.
