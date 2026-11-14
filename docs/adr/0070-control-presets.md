# ADR 0070: Control presets (pack-defined security settings)

- Status: accepted (job 124-control-presets)
- Date: 2026-11-14
- Scope: `crates/aegis-model/src/{control,project,methodology,error,op}.rs`,
  `crates/aegis-model/src/store/`, `crates/aegis-client/src/{commands,pages}.rs`,
  the built-in packs
- Rules: D1 (pack vocabulary), D6 (provenance), D26 (settings are controls with rules)
- Requirements: CFG-06
- Builds on: `0064-control-settings.md`, `0011-methodology-pack-files.md`

## Context

An analyst adding "Secure boot" to every ECU should not retype its rules. The
pack knows the usual settings; the project must still own the numbers it signs.

## Decisions

1. **`control_kinds` is an optional pack list** (after `attack_types`): `key`,
   `label`, optional `interface_kinds`, and `rules` as an object from attack
   type key to feasibility level key. Labels per language are in
   `translations.<lang>.control_kinds`. No `interface_kinds` means the preset is
   for components. At most 64 presets, 1 to 16 rules each; every key must be
   declared in the same pack, so a pack cannot point outside itself.
2. **Creating a setting from a preset copies the rules into the control** and
   records the kind with `Op::SetControlKind`. A later pack version never
   changes a saved result without an op. Rejected: reading the rules from the
   pack at evaluation time, because a pack update would silently change signed
   numbers.
3. **The model stores the kind and does not enforce the fit.** The client
   command refuses a wrong fit with a sentence. Enforcing it in the model would
   need guards on `SetInterfaceKind` and `ApplyControl` for no user gain.
4. **`RemoveSecurityControl` is rejected with `SecurityControlHasKind`** while a
   kind is set, so the inverse of a removal stays exact. The client's remove
   batch clears the kind first.
5. **A pack must not drop a preset key between versions.** A snapshot stores the
   kind by key and fails to load on an unknown key, as for attack types.
6. **`ControlRulesDifferFromKind` is a warning**, not a new severity.
   `Edit::UsePackValues` is one undo step and clears rules before it sets them,
   so the 16-rule cap holds inside the batch.
7. **The built-in presets (12 ISO, 8 CRA, 8 STRIDE) are editable examples.** CRA
   and STRIDE presets are for components only because those packs declare no
   interface kinds.
8. **View data**: `pages::add_setting_list` lists existing settings that fit and
   are not yet applied, presets that fit and have no control yet, and the custom
   entry; `ControlDetail` has `preset` and `differs_from_preset`.

## Consequences

- One "Secure boot" setting covers several ECUs; a preset is listed again only
  while no control of its kind exists.
- Left for the GPUI shell: the inline filtered list, its keyboard handling, the
  "(pack preset)" subtitle, the "Use the pack's values" button and the inline
  error. No Slint work was done.
