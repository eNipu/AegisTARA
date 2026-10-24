# Job 024: Cybersecurity Goals & Claims (`024-cybersecurity-goals-and-claims`)

## Goal
Implement ISO/SAE 21434 Clause 15.5 (and general TARA treatment traceability) **cybersecurity goals** and **cybersecurity claims** in `crates/aegis-model`, completing the traceability chain from threat scenarios and their risk treatment decisions (`Reduce`, `Retain`, `Share`) to cybersecurity goals, security controls, and cybersecurity claims, with full support across operations, referential integrity, completeness linting, project diff, and snapshot/op-log persistence.

---

## Scope & Architecture

### 1. Public IDs, Arena Indices, and Entity Kinds (`id.rs`, `error.rs`)
- **IDs & Indices (`id.rs`)**:
  - `GoalIdx` / `GoalId` via `entity_ids!` for [`CybersecurityGoal`].
  - `ClaimIdx` / `ClaimId` via `entity_ids!` for [`CybersecurityClaim`].
- **`EntityKind` (`error.rs`)**:
  - `EntityKind::CybersecurityGoal` (`"cybersecurity goal"`)
  - `EntityKind::CybersecurityClaim` (`"cybersecurity claim"`)

### 2. Domain Entities (`control.rs`, `threat.rs`)
- **`MAX_GOAL_CONTROLS: usize = 16` (`control.rs`)**:
  - Maximum number of [`SecurityControl`]s a [`CybersecurityGoal`] may link to.
- **`CybersecurityGoal` (`control.rs`)**:
  - `pub(crate) name: Arc<str>` (non-empty after trimming, at most `MAX_NAME_CHARS`)
  - `pub(crate) controls: Vec<ControlIdx>` — sorted by the target's `ControlId`, unique, at most `MAX_GOAL_CONTROLS`.
  - `pub(crate) provenance: Provenance`
  - Methods: `name(&self) -> &str`, `controls(&self) -> &[ControlIdx]`, `provenance(&self) -> &Provenance`.
- **`CybersecurityClaim` (`control.rs`)**:
  - `pub(crate) name: Arc<str>` (non-empty after trimming, at most `MAX_NAME_CHARS`)
  - `pub(crate) provenance: Provenance`
  - Methods: `name(&self) -> &str`, `provenance(&self) -> &Provenance`.
- **`ThreatScenario` Links (`threat.rs`)**:
  - `pub(crate) goal: Option<GoalIdx>` — optional linked cybersecurity goal (used for `RiskTreatment::Reduce`).
  - `pub(crate) claim: Option<ClaimIdx>` — optional linked cybersecurity claim (used for `RiskTreatment::Retain` and `RiskTreatment::Share`).
  - Methods: `pub const fn goal(&self) -> Option<GoalIdx>`, `pub const fn claim(&self) -> Option<ClaimIdx>`.

### 3. Operations (`op.rs`)
All mutating operations carry `Provenance` and return their exact inverse `Op` from `Project::apply`:
- **CybersecurityGoal ops**:
  - `Op::AddCybersecurityGoal { id: GoalId, name: Arc<str>, controls: Vec<ControlId>, provenance: Provenance }` (inverse: `Op::RemoveCybersecurityGoal { id }`)
  - `Op::RenameCybersecurityGoal { id: GoalId, name: Arc<str>, provenance: Provenance }` (inverse: `Op::RenameCybersecurityGoal` with old name & provenance)
  - `Op::LinkGoalControl { goal: GoalId, control: ControlId, provenance: Provenance }` (inverse: `Op::UnlinkGoalControl` with old provenance)
  - `Op::UnlinkGoalControl { goal: GoalId, control: ControlId, provenance: Provenance }` (inverse: `Op::LinkGoalControl` with old provenance)
  - `Op::RemoveCybersecurityGoal { id: GoalId }` (inverse: `Op::AddCybersecurityGoal { id, name, controls, provenance }`, controls in `ControlId` order)
- **CybersecurityClaim ops**:
  - `Op::AddCybersecurityClaim { id: ClaimId, name: Arc<str>, provenance: Provenance }` (inverse: `Op::RemoveCybersecurityClaim { id }`)
  - `Op::RenameCybersecurityClaim { id: ClaimId, name: Arc<str>, provenance: Provenance }` (inverse: `Op::RenameCybersecurityClaim` with old name & provenance)
  - `Op::RemoveCybersecurityClaim { id: ClaimId }` (inverse: `Op::AddCybersecurityClaim { id, name, provenance }`)
- **ThreatScenario link ops**:
  - `Op::SetThreatGoal { threat: ThreatId, goal: Option<GoalId>, provenance: Provenance }` (inverse: `Op::SetThreatGoal` with old `goal` and old `provenance`)
  - `Op::SetThreatClaim { threat: ThreatId, claim: Option<ClaimId>, provenance: Provenance }` (inverse: `Op::SetThreatClaim` with old `claim` and old `provenance`)

### 4. Referential Integrity, `RefCounts`, and `ModelError` (`project.rs`, `error.rs`)
- **`RefCounts`**:
  - `goals: Vec<u32>`: how many live `ThreatScenario`s reference each `GoalIdx`.
  - `claims: Vec<u32>`: how many live `ThreatScenario`s reference each `ClaimIdx`.
  - `control_goals: Vec<u32>`: how many live `CybersecurityGoal`s link to each `ControlIdx`.
- **Removal Guards**:
  - `RemoveCybersecurityGoal`: refuses with `ModelError::CybersecurityGoalInUse { goal, threats }` if `refs.goals[idx] > 0`.
  - `RemoveCybersecurityClaim`: refuses with `ModelError::CybersecurityClaimInUse { claim, threats }` if `refs.claims[idx] > 0`.
  - `RemoveSecurityControl`: refuses with `ModelError::SecurityControlInUse { control, steps }` if `refs.controls[idx] > 0`, and with `ModelError::SecurityControlInGoal { control, goals }` if `refs.control_goals[idx] > 0`.
  - `RemoveThreatScenario`: refuses with `ModelError::ThreatHasGoal { threat, goal }` if `t.goal.is_some()` and `ModelError::ThreatHasClaim { threat, claim }` if `t.claim.is_some()`, preserving single-op invertibility of `RemoveThreatScenario`.
- **New `ModelError` Variants**:
  - `DuplicateCybersecurityGoal(GoalId)`
  - `UnknownCybersecurityGoal(GoalId)`
  - `DuplicateGoalControl { goal: GoalId, control: ControlId }`
  - `GoalControlNotLinked { goal: GoalId, control: ControlId }`
  - `TooManyGoalControls { goal: GoalId, max: usize }`
  - `CybersecurityGoalInUse { goal: GoalId, threats: u32 }`
  - `SecurityControlInGoal { control: ControlId, goals: u32 }`
  - `DuplicateCybersecurityClaim(ClaimId)`
  - `UnknownCybersecurityClaim(ClaimId)`
  - `CybersecurityClaimInUse { claim: ClaimId, threats: u32 }`
  - `ThreatHasGoal { threat: ThreatId, goal: GoalId }`
  - `ThreatHasClaim { threat: ThreatId, claim: ClaimId }`
- **Project Accessors (`project.rs`)**:
  - `cybersecurity_goal_count(&self) -> usize`
  - `cybersecurity_goal(&self, idx: GoalIdx) -> Option<&CybersecurityGoal>`
  - `cybersecurity_goal_id(&self, idx: GoalIdx) -> Option<GoalId>`
  - `cybersecurity_goal_idx(&self, id: &GoalId) -> Option<GoalIdx>`
  - `cybersecurity_goals(&self) -> impl ExactSizeIterator<Item = (GoalIdx, GoalId, &CybersecurityGoal)>`
  - `cybersecurity_claim_count(&self) -> usize`
  - `cybersecurity_claim(&self, idx: ClaimIdx) -> Option<&CybersecurityClaim>`
  - `cybersecurity_claim_id(&self, idx: ClaimIdx) -> Option<ClaimId>`
  - `cybersecurity_claim_idx(&self, id: &ClaimId) -> Option<ClaimIdx>`
  - `cybersecurity_claims(&self) -> impl ExactSizeIterator<Item = (ClaimIdx, ClaimId, &CybersecurityClaim)>`

### 5. Linter Rules (`lint.rs`, `project.rs`)
Add 7 `LintSeverity::Warning` rules to `LintCode`:
1. `ReduceTreatmentWithoutGoal { threat: ThreatId }`: A threat scenario has `RiskTreatment::Reduce` but `threat.goal()` is `None`.
2. `RetainOrShareTreatmentWithoutClaim { threat: ThreatId }`: A threat scenario has `RiskTreatment::Retain` or `RiskTreatment::Share` but `threat.claim()` is `None`.
3. `GoalOnNonReduceThreat { threat: ThreatId, goal: GoalId }`: A threat scenario links to `goal`, but its `treatment()` is not `Some(RiskTreatment::Reduce)`.
4. `ClaimOnNonRetainOrShareThreat { threat: ThreatId, claim: ClaimId }`: A threat scenario links to `claim`, but its `treatment()` is neither `Some(RiskTreatment::Retain)` nor `Some(RiskTreatment::Share)`.
5. `CybersecurityGoalWithoutControls { goal: GoalId }`: A cybersecurity goal links to zero security controls (`goal.controls().is_empty()`).
6. `CybersecurityGoalUnlinked { goal: GoalId }`: A cybersecurity goal is not referenced by any threat scenario (`refs.goals[idx] == 0`).
7. `CybersecurityClaimUnlinked { claim: ClaimId }`: A cybersecurity claim is not referenced by any threat scenario (`refs.claims[idx] == 0`).

All new findings carry only stable `UUIDv7` IDs, never entity names or provenance actors, and maintain `size_of::<LintFinding>() <= 40`.

### 6. Project Diff (`diff.rs`, `project.rs`)
- `ProjectDiff`:
  - `cybersecurity_goals(&self) -> &EntityDiff<GoalId>`
  - `cybersecurity_claims(&self) -> &EntityDiff<ClaimId>`
  - Included in `ProjectDiff::is_empty()`.
- `Project::diff`:
  - Compares `goals` via `same_goal` (comparing `name`, `provenance`, and `controls` resolved by `ControlId`).
  - Compares `claims` via `==` (`name` and `provenance`).
  - `same_threat` compares `self.goal_key(x.goal) == other.goal_key(y.goal)` and `self.claim_key(x.claim) == other.claim_key(y.claim)`.
  - `is_threat_affected` returns `true` if a threat's linked goal is in `cybersecurity_goals.modified()` (or links a control in `controls.modified()`) or its linked claim is in `cybersecurity_claims.modified()`.

### 7. Store Persistence (`store/mod.rs`, `store/wire.rs`, `store/snapshot.rs`, `store/op_log.rs`)
- **`Limit::GoalControls { max: usize }`** (`Limit::GOAL_CONTROLS = Limit::GoalControls { max: MAX_GOAL_CONTROLS }`):
  - Enforced while decoding `controls` in `GoalWire` and `OpWire::AddCybersecurityGoal`, and checked in `within_caps` before writing to the op log.
- **Snapshot (`aegis-project` v1)**:
  - `cybersecurity_goals` (`#[serde(skip_serializing_if = "Goals::is_empty")]` / `#[serde(default)]`): array of `{"id": "<uuid>", "name": "<str>", "controls": ["<control_uuid>", ...], "provenance"?: ...}` in `GoalId` order, with `controls` in `ControlId` order.
  - `cybersecurity_claims` (`#[serde(skip_serializing_if = "Claims::is_empty")]` / `#[serde(default)]`): array of `{"id": "<uuid>", "name": "<str>", "provenance"?: ...}` in `ClaimId` order.
  - `ThreatView` / `ThreatWire`: optional `goal: "<goal_uuid>"` and `claim: "<claim_uuid>"` fields, omitted when `None`.
  - `Project::load`: applies `security_controls` before `cybersecurity_goals` and `cybersecurity_claims`, and applies both before `threat_scenarios`.
- **Op Log (`aegis-oplog` v1)**:
  - Encodes and decodes all 10 new `Op` variants (`add_cybersecurity_goal`, `rename_cybersecurity_goal`, `link_goal_control`, `unlink_goal_control`, `remove_cybersecurity_goal`, `add_cybersecurity_claim`, `rename_cybersecurity_claim`, `remove_cybersecurity_claim`, `set_threat_goal`, `set_threat_claim`).

---

## Acceptance Criteria
1. **AC1 (Entities & Operations)**: `CybersecurityGoal` and `CybersecurityClaim` can be added, renamed, linked/unlinked to controls (`MAX_GOAL_CONTROLS = 16`), linked/unlinked from threats (`SetThreatGoal`, `SetThreatClaim`), and removed; every mutating op returns an exact single-op inverse that restores both domain state and provenance.
2. **AC2 (Referential Integrity)**: Removing a `SecurityControl` linked by a goal fails with `SecurityControlInGoal`; removing a `CybersecurityGoal` or `CybersecurityClaim` referenced by a threat fails with `CybersecurityGoalInUse` / `CybersecurityClaimInUse`; removing a `ThreatScenario` with a linked goal or claim fails with `ThreatHasGoal` / `ThreatHasClaim`; `RefCounts` invariants pass across random op sequences in `proptest`.
3. **AC3 (Lint Rules)**: `Project::lint` reports all 7 goal/claim completeness and consistency warnings in canonical order, and resolving each condition clears the corresponding finding.
4. **AC4 (Project Diff)**: `Project::diff` reports added, removed, and modified cybersecurity goals and claims, marks threats as modified when their linked goal or claim ID changes, and marks a threat's risk delta as affected when its linked goal, goal's controls, or linked claim is modified.
5. **AC5 (Store Persistence & Decode Caps)**: Snapshots and op logs round-trip all goal/claim entities, links, and operations with canonical output and capped decoding (`Limit::GoalControls { max: 16 }`).
6. **AC6 (Data-Minimized Diagnostics)**: No `ModelError`, `StoreError`, or `LintFinding` ever formats a goal or claim `name` or provenance `actor`.
