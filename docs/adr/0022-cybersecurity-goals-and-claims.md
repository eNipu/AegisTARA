# 0022. Cybersecurity Goals and Claims

Date: 2026-04-19
Status: Accepted

## Context

ISO/SAE 21434 Clause 15.5 requires that every threat scenario's risk treatment decision be backed by a traceable artifact:
- When a threat scenario's risk is treated by **reducing** the risk (`RiskTreatment::Reduce`), one or more **cybersecurity goals** must be specified to address the threat scenario, and security controls are selected to fulfill those cybersecurity goals.
- When a threat scenario's risk is treated by **retaining** (`RiskTreatment::Retain`) or **sharing** (`RiskTreatment::Share`) the risk, a **cybersecurity claim** must be recorded to document the rationale and assumptions justifying that decision.

Without first-class `CybersecurityGoal` and `CybersecurityClaim` entities in `aegis-model`, a TARA project can record `RiskTreatment` decisions and attach `SecurityControl` mitigations to attack steps (ADR 0017), but cannot represent the top-level cybersecurity requirements (goals) or the rationale for retained/shared risk (claims), nor lint or diff that traceability chain.

## Decision

### 1. Entities and Identifiers (`control.rs`, `id.rs`, `threat.rs`)
- Introduce two arena-allocated project entities with `u32` slot indices and `UUIDv7` public IDs:
  - [`CybersecurityGoal`] (`GoalIdx`, `GoalId`, `EntityKind::CybersecurityGoal`): holds a `name: Arc<str>`, a sorted unique list of linked security controls `controls: Vec<ControlIdx>` (capped at `MAX_GOAL_CONTROLS = 16`), and `provenance: Provenance`.
  - [`CybersecurityClaim`] (`ClaimIdx`, `ClaimId`, `EntityKind::CybersecurityClaim`): holds a `name: Arc<str>` and `provenance: Provenance`.
- Extend [`ThreatScenario`] with two optional arena links:
  - `goal: Option<GoalIdx>`
  - `claim: Option<ClaimIdx>`

This many-to-one link from `ThreatScenario` to `CybersecurityGoal` (and many-to-many from `CybersecurityGoal` to `SecurityControl`) matches engineering practice: multiple threat scenarios (for example, several spoofing or tampering threats on a bus) frequently share one high-level cybersecurity goal (for example, "Authenticate safety-critical CAN frames"), which in turn is realized by one or more security controls (for example, "SecOC", "HSM key storage"). Similarly, multiple retained or shared threats can reference a shared cybersecurity claim.

### 2. Operations and Single-Op Invertibility (`op.rs`, `project.rs`)
- Add 10 operations to [`Op`]:
  - `AddCybersecurityGoal`, `RenameCybersecurityGoal`, `LinkGoalControl`, `UnlinkGoalControl`, `RemoveCybersecurityGoal`
  - `AddCybersecurityClaim`, `RenameCybersecurityClaim`, `RemoveCybersecurityClaim`
  - `SetThreatGoal`, `SetThreatClaim`
- To preserve single-op invertibility (`Project::apply(op)` returns a single `Op` that undoes `op` in O(size of op)):
  - `RemoveThreatScenario` requires that `threat.goal()` and `threat.claim()` be `None` first, returning `ModelError::ThreatHasGoal { threat, goal }` or `ModelError::ThreatHasClaim { threat, claim }` otherwise (matching `ThreatHasAttackTree`, `ThreatHasDirectFeasibility`, and `ThreatHasRiskTreatment`).
  - `RemoveCybersecurityGoal` requires that no live threat scenario reference the goal (`ModelError::CybersecurityGoalInUse { goal, threats }`), and returns `Op::AddCybersecurityGoal { id, name, controls, provenance }` with `controls` in `ControlId` order.
  - `RemoveCybersecurityClaim` requires that no live threat scenario reference the claim (`ModelError::CybersecurityClaimInUse { claim, threats }`), and returns `Op::AddCybersecurityClaim { id, name, provenance }`.
  - `RemoveSecurityControl` requires both zero step mitigations (`SecurityControlInUse`) and zero linking cybersecurity goals (`ModelError::SecurityControlInGoal { control, goals }`).

### 3. Reference Counting (`project.rs`)
- Extend `RefCounts` with three `Vec<u32>` counters indexed by arena slot:
  - `goals: Vec<u32>`: how many live `ThreatScenario`s reference each `GoalIdx`.
  - `claims: Vec<u32>`: how many live `ThreatScenario`s reference each `ClaimIdx`.
  - `control_goals: Vec<u32>`: how many live `CybersecurityGoal`s link to each `ControlIdx`.
- All removal checks and unlinked-entity lint rules run in O(1) time per entity using `RefCounts`.

### 4. Completeness and Traceability Linter Rules (`lint.rs`)
- Add 7 `LintSeverity::Warning` rules to [`LintCode`]:
  - `ReduceTreatmentWithoutGoal { threat }`: threat has `RiskTreatment::Reduce` but `threat.goal()` is `None`.
  - `RetainOrShareTreatmentWithoutClaim { threat }`: threat has `RiskTreatment::Retain` or `RiskTreatment::Share` but `threat.claim()` is `None`.
  - `GoalOnNonReduceThreat { threat, goal }`: threat links `goal`, but its treatment is not `Some(RiskTreatment::Reduce)`.
  - `ClaimOnNonRetainOrShareThreat { threat, claim }`: threat links `claim`, but its treatment is neither `Some(RiskTreatment::Retain)` nor `Some(RiskTreatment::Share)`.
  - `CybersecurityGoalWithoutControls { goal }`: goal links to zero security controls.
  - `CybersecurityGoalUnlinked { goal }`: no threat scenario links to `goal`.
  - `CybersecurityClaimUnlinked { claim }`: no threat scenario links to `claim`.
- Keeping these rules at `LintSeverity::Warning` preserves the ADR 0018 soundness invariant that `LintSeverity::Error` is reserved for conditions that block evaluation or leave a `Reduce` treatment with zero step mitigations.

### 5. Project Diff and Store Persistence (`diff.rs`, `store/`)
- `ProjectDiff` gains `cybersecurity_goals(&self) -> &EntityDiff<GoalId>` and `cybersecurity_claims(&self) -> &EntityDiff<ClaimId>`.
- `aegis-project` v1 snapshots include `cybersecurity_goals` and `cybersecurity_claims` top-level arrays (omitted when empty) and optional `goal` / `claim` fields on each threat scenario.
- `Limit::GoalControls { max: 16 }` caps the `controls` list on a cybersecurity goal during snapshot and op-log decoding.

## Consequences

- **Complete ISO/SAE 21434 Clause 15.5 traceability**: Analysts and downstream export/reporting tools can traverse `ThreatScenario -> RiskTreatment::Reduce -> CybersecurityGoal -> SecurityControl -> StepMitigation` and `ThreatScenario -> RiskTreatment::{Retain, Share} -> CybersecurityClaim`.
- **Backwards-compatible v1 wire format**: Existing snapshots without `cybersecurity_goals`, `cybersecurity_claims`, `goal`, or `claim` fields load without change via `#[serde(default)]`, and projects without goals or claims omit those fields when saved.
