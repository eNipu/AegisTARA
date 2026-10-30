# Spec — 118-visual-ids

## Title
Human-readable visual IDs on every element (FND-06, FND-07)

## Design Reference
- Reference: `docs/requirements.md` FND-06 and FND-07; `docs/brainstorm-visual-ids.md`.
- Affected crates/paths:
  - `crates/aegis-model/src/{lib.rs,project.rs,op.rs,store/snapshot.rs,store/op_log.rs,store/wire.rs}`
  - `crates/aegis-client/src/{pages.rs,view.rs,report/doc.rs,report/csv.rs,report/xlsx.rs}`
  - `crates/aegis-shell/src/{shortcuts.rs,controller.rs}`
  - `crates/aegis-shell/ui/app.slint`
  - `crates/aegis-model/tests/project.rs`, `crates/aegis-client/tests/pages.rs`, `crates/aegis-shell/tests/ui.rs`

## Problem & Goal
Every TARA element has an internal UUIDv7 identifier (`AssetId`, `DamageId`, `ThreatId`, `TreeId`, `ControlId`, `GoalId`, `ClaimId`, `ComponentId`, `InterfaceId`, `DataFlowId`, `TrustBoundaryId`) that cannot be spoken or cited in reviews. Give each element a short, fixed-prefix visual ID (`A00`, `D00`, `T00`, `AT00`, `SC00`, `G00`, `CL00`, `C00`, `IF00`, `DF00`, `TB00`) alongside its internal UUID, persist it across snapshots and op logs without breaking v1 fixtures, never reuse deleted numbers, restore numbers on undo, and surface the visual ID in tables, inspector titles, cross-references, reports, and command palette search.

## Acceptance Criteria
1. **Model & prefix scheme (`FND-06`)**: Every element of the 11 project element types (`Asset` -> `A`, `DamageScenario` -> `D`, `ThreatScenario` -> `T`, `AttackTree` -> `AT`, `SecurityControl` -> `SC`, `CybersecurityGoal` -> `G`, `CybersecurityClaim` -> `CL`, `Component` -> `C`, `Interface` -> `IF`, `DataFlow` -> `DF`, `TrustBoundary` -> `TB`) carries a visual number (`u32`) and formatted visual ID (`format!("{prefix}{num:02}")`, growing to 3+ digits at `>= 100`). Attack tree nodes (`NodeId`) are not numbered.
2. **Stability, non-reuse, and undo (`FND-06`, `UI-10.6`)**: Visual numbers are assigned monotonically per element type within a project. Deleting an element never reuses its visual number for a new element (`A00` deleted means the next new asset is `A01`). Undoing a removal (`inverse` of `Op::Remove*`) restores the removed element's original visual number, and `Project::eq` compares live elements including their visual numbers.
3. **Persistence & v1 fixture compatibility (`FND-06`, `P4`, `R3`)**: Snapshots and op logs round-trip non-default visual IDs and per-type high-water counters as string-encoded visual IDs (`"A01"`, etc., respecting the store's `only_keys_on_disk` rule and decode caps) while omitting redundant defaults so v1 golden fixtures load deterministically (`00, 01, ...` in saved order) and re-save byte-for-byte identically.
4. **UI tables, inspector titles, cross-references, reports, and command palette (`FND-07`)**:
   - Every element table in the desktop shell has a leading `ID` column showing the visual ID.
   - Detail / inspector titles display the selected element's visual ID alongside its title.
   - Cross-references in link lists and inspector summaries include the visual ID.
   - HTML, PDF, CSV, and XLSX reports surface element visual IDs.
   - The command palette (`Ctrl+K` / `⌘K`) indexes open project elements by visual ID and name, and selecting an element entry navigates to its page and selects it.
