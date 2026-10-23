# 0018. Project completeness and consistency linter

Date: 2026-10-23
Status: Accepted

## Context

In ISO/SAE 21434 Clause 15 and UN R155 Annex 5 reviews, a TARA work product is built incrementally across concept and product-development phases. During editing, a project is valid under `Project::apply` (referential integrity holds at every operation), yet it may still be incomplete or internally inconsistent from an assessment standpoint:

- Assets or declared security properties may not be covered by any threat scenario.
- Damage scenarios may have no impact ratings or may not be linked to any threat scenario.
- Threat scenarios may lack linked damage scenarios, lack a feasibility source (neither an attack tree nor a direct feasibility rating), carry a direct feasibility rating that is shadowed by a linked attack tree, lack a risk treatment decision, or declare `RiskTreatment::Reduce` without any security control mitigations on their attack tree.
- Attack trees may be unlinked, contain empty gates, or contain unrated leaf steps.
- Security controls may not be attached to any attack step.

Analysts, CI checks, and audit exports need a deterministic, machine-readable completeness and consistency check (`Project::lint`) that:

1. Runs in linear time in project size using the existing `RefCounts` tables without rescanning the threat arena per entity.
2. Produces a canonical finding order independent of operation or edit history (rule P4).
3. Never leaks user-supplied entity names, labels, or provenance actor strings in `Display` or `Debug` output (rule R6).
4. Guarantees a soundness property: when `Project::lint` reports zero `LintSeverity::Error` findings, every tree and threat risk evaluation on the project succeeds without error.

## Decision

1. **`LintSeverity`, `LintCode`, and `LintFinding` (`lint.rs`).**
   - `LintSeverity` has two ordered levels: `Warning < Error`.
   - `LintCode` is a `Copy` enum (`<= 40` bytes) with 13 variants referencing entities strictly by stable public IDs (`AssetId`, `DamageId`, `ThreatId`, `TreeId`, `NodeId`, `ControlId`) and `Property` vocabulary index:
     - Warnings (`LintSeverity::Warning`):
       - `AssetWithoutThreats { asset }`
       - `AssetPropertyUntested { asset, property }`
       - `DamageScenarioWithoutImpacts { damage }`
       - `DamageScenarioUnlinked { damage }`
       - `ThreatWithoutDamages { threat }`
       - `RedundantDirectFeasibility { threat }`
       - `ThreatWithoutTreatment { threat }`
       - `AttackTreeUnlinked { tree }`
       - `SecurityControlUnused { control }`
     - Errors (`LintSeverity::Error`):
       - `ThreatWithoutFeasibility { threat }`
       - `ReduceTreatmentWithoutControls { threat }`
       - `AttackTreeGateEmpty { tree, node }`
       - `AttackTreeStepUnrated { tree, node }`
   - `LintFinding` wraps `LintCode` and exposes `code()`, `severity()`, `is_error()`, and `is_warning()`.

2. **Single-pass `RefCounts`-backed `Project::lint` (`project.rs`).**
   - `Project::lint(&self) -> Vec<LintFinding>` scans `self.assets`, `self.damages`, `self.threats`, `self.trees`, and `self.controls` once in canonical ID order (`Arena::iter`), consulting `self.refs` (`assets`, `asset_properties`, `damages`, `trees`, `controls`) in $O(1)$ / $O(\log P)$ time per entity, then sorts findings by `LintFinding: Ord`.
   - Total time is $O(A P + D + T + K V + C + F \log F)$ and space is $O(F)$ for $F$ findings, allocating zero temporary hash maps or reference tables.
   - `AssetPropertyUntested` is emitted only when an asset has at least one targeting threat (`refs.assets[idx] > 0`), avoiding redundant noise alongside `AssetWithoutThreats`.

## Consequences

- `Project::lint` is 100% deterministic and history-independent (rule P4), verified by unit, integration, and property tests over both methodologies.
- Zero `LintSeverity::Error` findings guarantees that `evaluate_tree`, `evaluate_residual_tree`, `evaluate_risk`, and `evaluate_residual_risk` succeed for every tree and threat scenario in the project.
- `LintFinding` and `LintCode` never contain entity names or actor strings, satisfying rule R6.
