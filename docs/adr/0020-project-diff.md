# 0020. Deterministic Project Version Diff and Change Impact Analysis

Date: 2026-04-21
Status: Accepted

## Context

Rule A7 requires deterministic version diff and change impact analysis between two TARA project snapshots (`base` and `curr`) over the same methodology so engineers and reviewers can audit:

1. Which assets, damage scenarios, threat scenarios, attack trees, and security controls were added, removed, or modified.
2. Which threat scenarios are impacted by those changes (either directly or indirectly through a modified asset, damage scenario, attack tree, or security control) and how each impacted threat's inherent and residual feasibility and worst-case risk (`max_risk`) changed.

All diff outputs must be deterministic, canonical, history-independent (rule P4), and free of user-supplied entity names, labels, or actor text (rule R6).

## Decision

1. **Methodology compatibility check (`ModelError::MethodologyMismatch`):**
   - `Project::diff(&self, other: &Project) -> Result<ProjectDiff, ModelError>` first verifies that `self` and `other` share an identical methodology (`Arc::ptr_eq(&self.methodology, &other.methodology) || *self.methodology == *other.methodology`), because `FeasibilityLevel`, `RiskLevel`, `Property`, `ImpactCategory`, and `ThreatCategory` indices are only comparable within the same methodology.
   - If the methodologies differ, `diff` returns `Err(ModelError::MethodologyMismatch { expected, expected_version, found, found_version })`, carrying only methodology keys and versions (rule R6).

2. **Linear sorted merge-join across entity arenas (`EntityDiff<Id>`):**
   - Because `Arena::iter` yields live entities in ascending `UUIDv7` order (`BTreeMap<Id, Idx>`), a single generic helper `diff_arena(base, curr, eq)` performs a sorted two-pointer merge-join in $O(N_{\text{base}} + N_{\text{curr}})$ time without allocating hash maps or re-sorting.
   - Cross-arena slot references inside `ThreatScenario` and `TreeEntry` are compared via `Project::same_threat` and `Project::same_tree` using public IDs so arena slot compaction or insertion history never causes false positives (rule P4).

3. **Change impact propagation and risk deltas (`ThreatRiskDelta`, `RiskDelta`, `RiskSnapshot`, `RiskChange`):**
   - A threat scenario is included in `ProjectDiff::risk_deltas()` (in ascending `ThreatId` order) whenever:
     - It was added (`before = None`) or removed (`after = None`), or
     - It was directly modified (`threats.modified()`), or
     - Its referenced `AssetId` is in `assets.modified()`, or
     - Any of its linked `DamageId`s is in `damage_scenarios.modified()`, or
     - Its linked `TreeId` is in `attack_trees.modified()`, or any `StepMitigation` in that tree references a `ControlId` in `security_controls.modified()`.
   - For each affected threat scenario, `ThreatRiskDelta` records both `inherent: RiskDelta` and `residual: RiskDelta`, where each `RiskDelta` holds `before: Option<RiskSnapshot>`, `after: Option<RiskSnapshot>`, and directional `change: RiskChange` (`Unchanged`, `Increased`, `Decreased`, `NewlyEvaluated`, `NoLongerEvaluated`).
   - `ProjectDiff::changed_risks()` filters `risk_deltas()` to threats whose inherent or residual `RiskSnapshot` (`feasibility`, `max_risk`, or evaluability) actually changed.

## Consequences

- `p1.diff(&p2)?.is_empty() == (p1 == p2)` holds identically across all projects over the same methodology, and `p.diff(&p)?.risk_deltas().is_empty()` holds reflexively.
- Indirect risk changes (such as raising a damage scenario's impact level or attaching a security control to a shared attack tree step) automatically surface on every affected threat scenario without requiring the threat scenario itself to be touched.
- Every diff type stores only public `UUIDv7` identifiers and vocabulary indices, guaranteeing rule R6 compliance.
