VERDICT: PASS

No UI change (no `.slint` or GUI files touched in job `024-cybersecurity-goals-and-claims`).

### UI-Readiness Verification (`crates/aegis-model`)

1. **`CybersecurityGoal`, `CybersecurityClaim`, and `ThreatScenario` Links (`crates/aegis-model/src/control.rs:14-20`, `319-537`; `crates/aegis-model/src/threat.rs:80-81`, `332-412`)**:
   - `CybersecurityGoal` (`Clone`, `Debug`, `PartialEq`, `Eq`) exposes `name() -> &str`, `controls() -> &[ControlIdx]` (sorted by `ControlId`, unique, capped at `MAX_GOAL_CONTROLS = 16`), and `provenance() -> &Provenance` in O(1), enabling goal inspector panels and control badge lists to render deterministically without edit-history ordering artifacts.
   - `CybersecurityClaim` (`Clone`, `Debug`, `PartialEq`, `Eq`) exposes `name() -> &str` and `provenance() -> &Provenance` in O(1) for retain/share treatment justification panels.
   - `ThreatScenario` exposes `goal() -> Option<GoalIdx>` and `claim() -> Option<ClaimIdx>` (`const fn`, O(1)), allowing threat table rows and detail views to resolve the linked goal or claim in O(1) via `Project::cybersecurity_goal(idx)` / `Project::cybersecurity_claim(idx)`.

2. **Project Accessors & Deterministic Iteration (`crates/aegis-model/src/project.rs:1384-1609`)**:
   - `Project` exposes complete, symmetric O(1) / O(log n) lookup and iteration APIs for both entities:
     - `cybersecurity_goal(idx) -> Option<&CybersecurityGoal>`, `cybersecurity_goal_idx(&id) -> Option<GoalIdx>`, `cybersecurity_goal_id(idx) -> Option<GoalId>`, `cybersecurity_goal_count() -> usize`, and `cybersecurity_goals() -> impl Iterator<Item = (GoalIdx, GoalId, &CybersecurityGoal)>` in canonical `GoalId` order.
     - `cybersecurity_claim(idx) -> Option<&CybersecurityClaim>`, `cybersecurity_claim_idx(&id) -> Option<ClaimIdx>`, `cybersecurity_claim_id(idx) -> Option<ClaimId>`, `cybersecurity_claim_count() -> usize`, and `cybersecurity_claims() -> impl Iterator<Item = (ClaimIdx, ClaimId, &CybersecurityClaim)>` in canonical `ClaimId` order.
   - All mutating operations (`AddCybersecurityGoal`, `RenameCybersecurityGoal`, `LinkGoalControl`, `UnlinkGoalControl`, `RemoveCybersecurityGoal`, `AddCybersecurityClaim`, `RenameCybersecurityClaim`, `RemoveCybersecurityClaim`, `SetThreatGoal`, `SetThreatClaim`) return their exact single-op inverse from `Project::apply`, supporting undo/redo stacks in the UI without special cases.

3. **`ProjectDiff` & Change Impact Analysis (`crates/aegis-model/src/diff.rs:554-723`; `crates/aegis-model/src/project.rs:2820-2849`, `2980-3007`)**:
   - `ProjectDiff` exposes `cybersecurity_goals(&self) -> &EntityDiff<GoalId>` and `cybersecurity_claims(&self) -> &EntityDiff<ClaimId>` in canonical ID order and includes both in `ProjectDiff::is_empty()`.
   - `Project::diff` marks a threat's risk delta as affected (`is_threat_affected`) when its linked goal, any security control linked by that goal, or its linked claim is modified, allowing diff review views to highlight treatment-chain changes alongside threat scenarios.

4. **`LintCode` Display, Severity & Data Minimization (`crates/aegis-model/src/lint.rs:135-164`, `209-224`, `256-262`, `330-358`, `469-530`; `crates/aegis-model/src/error.rs:262-266`, `283-287`, `485-509`, `542-557`, `591-624`)**:
   - All 7 new `LintCode` variants (`ReduceTreatmentWithoutGoal`, `RetainOrShareTreatmentWithoutClaim`, `GoalOnNonReduceThreat`, `ClaimOnNonRetainOrShareThreat`, `CybersecurityGoalWithoutControls`, `CybersecurityGoalUnlinked`, `CybersecurityClaimUnlinked`) have `LintSeverity::Warning`, clear human-readable `Display` messages, and carry only stable `ThreatId`/`GoalId`/`ClaimId` identifiers (never user-supplied names or provenance actors, preserving rule R6 and `size_of::<LintFinding>() <= 40`).
   - All 12 new `ModelError` variants and `Limit::GoalControls { max: 16 }` provide clear, PII-safe `Display` output suitable for UI error banners.

VERDICT: PASS
