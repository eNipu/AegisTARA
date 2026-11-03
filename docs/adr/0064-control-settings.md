# ADR 0064: Security settings (control targets, attack-type rules, derived step ceilings)

- Status: accepted (job 123-control-settings)
- Date: 2026-11-03
- Scope: `crates/aegis-model/src/{control,item,error,op,project}.rs`,
  `crates/aegis-model/src/store/{wire,snapshot,op_log,mod}.rs`,
  `crates/aegis-canvas/src/{scene,render,access}.rs`,
  `crates/aegis-client/src/{commands,pages,view,session,undo_label}.rs`,
  `crates/aegis-shell/{ui/app.slint,ui/widgets.slint,src/controller.rs}`
- Rules: D1 (pack vocabulary), D6 (provenance), D7 (deterministic order), D10
  (derived state), D26 (settings are controls with rules)
- Requirements: CFG-03, CFG-04, CFG-05 (partly), CFG-08
- Builds on: `0017-security-controls-and-residual-risk.md`,
  `0023-incremental-evaluation-cache.md`, `0059-item-definition-model.md`, job
  122 (step via and attack type)

## Context

Job 122 let an attack step name the component or interface it goes through and
carry an attack type, but nothing read them. Analysts need to say "JTAG is
disabled on this ECU" once and have every matching step lose feasibility, with
the reason shown and one undo step per change.

## Decisions

1. **A setting is a `SecurityControl`, not a new entity (D26).** The control
   gains `targets: Vec<ItemIdx>` (at most `MAX_CONTROL_TARGETS = 64`) and
   `rules: Vec<ControlRule>` (at most `MAX_CONTROL_RULES = 16`). Both are
   sorted, so equal content compares equal whatever the op order. Ops:
   `ApplyControl`, `UnapplyControl` (each other's inverse) and
   `SetControlRule { control, attack_type, ceiling: Option }` (inverse: the
   old value). Option considered and rejected: a separate `Setting` arena,
   which would duplicate names, visual IDs, goal links and the controls page.
2. **The ceiling is derived, never stored.** Residual = min(inherent, manual
   mitigations, every matching rule of every control applied to the step's via
   item, or for an interface via, to its owning component). Inherent ratings
   never change. Settings do not flow to child components, and a control on an
   interface does not cap steps through the whole component. Storing the
   result would need a fix-up op on every change of via, type, rule or target.
3. **Explain-why returns every ceiling with its source, deciding first.**
   `Project::step_ceilings` gives `StepCeiling { control, feasibility,
   source }`, where `source` is `CeilingSource::StepMitigation` or
   `ControlRule { target, attack_type }`. The model holds no names (R6); the
   client builds the sentence.
4. **The cache marks steps dirty, it does not recompute everything.**
   `SetStepVia` and `SetStepAttackType` mark the one step. `ApplyControl` and
   `UnapplyControl` mark the steps whose via is the target or an interface of
   it; `SetControlRule` marks the steps with that attack type that the control
   reaches. Cost is O(steps with a via) per op. A property test applies the ops in
   shuffled order, updating the cache after each, and compares the cache with a
   full evaluation at the end.
5. **Removal is rejected while referenced** (the 122 pattern) so inverses stay
   exact: `ComponentHasControls`, `InterfaceHasControls`,
   `SecurityControlHasTargets`, `SecurityControlHasRules`. The client's remove
   batches unapply and clear first, in one undo step.
6. **Store**: snapshot controls get `targets` and `rules`, omitted when empty
   (v1 fixtures load unchanged); the op log has the three ops; decode caps are
   `Limit::ControlTargets` and `Limit::ControlRules`.
7. **UI**: a "Security settings" section in the component and interface
   inspectors (a component's settings are dimmed on its interfaces), a custom
   setting is an inline draft with a rules table (no modal; the dialog allow
   list is unchanged), a shield badge with an accessible sentence on capped
   canvas steps, and an undo toast that counts capped steps and names the risks
   that moved. A custom setting is one `Edit::AddCustomSetting` batch.

## Consequences

- A settings edit costs O(what changed) in the cache; nothing is stored twice.
- Memory: `SecurityControl` grew from 24 to 72 bytes (two `Vec`s, no heap while
  empty; measured with `size_of`), and each component and interface slot has a
  4-byte control count. `benches/store_memory.rs` has no controls case, so the
  gates do not see this; a project has tens to hundreds of controls, far below
  what trees and assets cost.
- `SecurityControlUnused` counts a control with targets as used, and
  `ReduceTreatmentWithoutControls` accepts a rule ceiling.
- `StepSettingsIncomplete` (a warning) flags a step that has an attack type
  without a through item, or the reverse, but only while some control carries
  rules: such a step can never match a rule. A step with neither is just
  unfilled and is not flagged.
- The version diff already compared targets (by public ID, in canonical order)
  and rules, so a changed setting shows as a modified control; the client's
  comparison line now says what changed (count of applied items, count of
  rules, or that items or ceilings changed with the count the same).
- The threat inspector's "why" lines name the settings (and manual
  mitigations) that cap the step deciding the residual feasibility, deciding
  one first, at most three, ceilings below the rating only.
- Job 124 adds presets (a `kind` on the control and pack `control_kinds`); the
  rules are copied into the control, so a later pack version never changes a
  saved result.
