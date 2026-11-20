# Tier 2 EGM Spec Audit — `188-inspector`

- **Date:** 2026-11-20
- **Job:** `188-inspector` (Inspector with explain-why and go to cause, WBX-06)
- **Auditors:**
  - `egm-codebase-details` (`4ed35c5d-8fc8-4d03-9fc1-9722e1104e29`, 6 turns)
  - `egm-implementation-readiness` (`e0cbe9bc-b6d1-471b-8c93-473ccf452626`, 5 turns)

## Summary of Pre-Build Findings & Resolutions

1. **[Major — Fixed] `StepCeiling::control()` returns `ControlIdx`, not `ControlId` (`188a.md`):**
   - `aegis_model::StepCeiling` exposes `.control() -> ControlIdx` (no `.control_id()` method). Converting to `ControlId` requires `project.security_control_id(c.control())`.
   - **Resolution:** Updated `188a.md` lines 148 and 155 to use `project.security_control_id(c.control())` when building `Selection::Control(cid)` cause targets.

2. **[Major — Fixed] `aegis_model::Reason` and `ElementRef` are `#[non_exhaustive]` (`188a.md`):**
   - Matching on `Reason` or `ElementRef` in `aegis-client` without a `_ =>` wildcard arm fails compilation (`non-exhaustive patterns`).
   - **Resolution:** Updated `188a.md` to specify the 11-arm `element_ref_selection(elem: ElementRef) -> Option<Selection>` helper with `_ => None` and `Reason::AllPathsExcluded | _ => None`.

3. **[Major — Fixed] `DecidingStep` vs `why_path` return types (`188a.md`):**
   - `project.deciding_step(tree_idx, eval)` returns `Option<DecidingStep>` (`.node_id() -> NodeId`, `.ceilings() -> &[StepCeiling]`), whereas `eval.why_path(tree.root())` returns `Vec<NodeIdx>`.
   - **Resolution:** Explicitly documented both lookup paths in `188a.md` lines 148 and 151.

4. **[Major — Fixed] Reuse `table::selection_element_ref` and `table::selection_system_set` (`188b.md`):**
   - `selection_element_ref` and `selection_system_set` in `crates/aegis-client/src/table.rs` were private (`fn`), and `view::level_tone` in `view.rs` is private.
   - **Resolution:** Updated `188b.md` to mark `table::selection_element_ref` and `table::selection_system_set` as `pub(crate) fn` (saving ~25 lines of duplicate match blocks in `inspector.rs`) and specified a local `feasibility_tone` helper using `Tone::from_level`.

5. **[Major — Fixed] Borrow-checker & double-lease safety in `InspectorPanel::open_chip` (`188c.md`):**
   - Calling `self.set_selection(..., wb.read(cx).workspace(), ...)` directly would borrow `cx` immutably and mutably at the same time.
   - **Resolution:** Updated `188c.md` to specify computing `let built = inspector_view(wb.read(cx).workspace(), target, &self.catalog);` before calling `self.apply_built(Some(target), built, None, cx)` after `wb.update` returns.

6. **[Major — Fixed] Post-`cargo fmt` `max_lines` headroom (`plan.json`, `188a.md`, `188b.md`, `188c.md`):**
   - `188a` (90 `.ftl` lines + `inspector.rs` + tests), `188b` (14-variant `inspector_view` + 14-kind test fixture), and `188c` (`InspectorPanel` UI + 6 headless GPUI scenarios) were close to `520–540` lines after `cargo fmt`.
   - **Resolution:** Raised `max_lines` in `plan.json` to `560` across `188a`, `188b`, and `188c` (below the 600 hard cap) and added conciseness guidance to the slice prompts.

7. **[Nit — Fixed] Align reverse relation key names in `spec.md` with `188a.md`/`188b.md`:**
   - Updated `spec.md` Decision #3 to reference `inspector-rev-step-via` (`RelationKey::STEP_VIA`) and `inspector-rev-step-mitigations` (`RelationKey::STEP_MITIGATIONS`).
