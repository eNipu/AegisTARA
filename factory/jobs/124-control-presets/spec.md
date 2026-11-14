# Spec: 124-control-presets

## Title

Setting presets from the pack (CFG-06, D26)

## Design Reference

- `docs/requirements.md`, CFG-06 (line 187).
- `docs/decisions.md`, D26 (line 203): attack types and setting presets come from the pack; a custom setting is a control with rules the user types.
- `docs/design/systems-config.md`, section 5 "Add setting (presets and custom), modeless" (line 176) and section 6 (line 234, the "Rules differ from the pack" line).
- `docs/adr/0064-control-settings.md` (job 123): a setting is a `SecurityControl` with targets and rules; removal is rejected while referenced.
- `factory/jobs/124-control-presets/rework.md`, `## Answers`: no Slint work in this job.
- Code: `crates/aegis-model/src/methodology/{mod,pack}.rs`, `crates/aegis-model/src/{control,op,project,lint,error}.rs`, `crates/aegis-model/src/store/{mod,wire,op_log,snapshot}.rs`, `crates/aegis-model/packs/{iso21434-r155,eu-cra,stride-services-cloud}.json`, `crates/aegis-client/src/{commands,pages,undo_label}.rs`.

## Problem & Goal

Today every security setting is typed by hand: the consultant names a control and fills in its rules ("firmware modification is at most Low") for each project. The same dozen settings (secure boot, locked debug port, sealed housing) come back in every TARA.

Goal: a pack can ship setting presets. Picking one creates the setting with the preset's rules and applies it, in one undo step.

Business value of each part (P7):

| Part | Value |
|---|---|
| `control_kinds` in the pack | A consultancy ships its own house presets with a pack; the core stays methodology-neutral (D1, D24). |
| Rules copied into the control | A later pack version never changes a saved risk number without an op in the history. Needed for signed reports and baselines. |
| `kind` on the control (`Op::SetControlKind`) | The app can say "this is Secure boot from the pack" and tell when its rules were changed. |
| Refusal when a preset does not fit | A "debug port" preset on a whole ECU would cap steps it has nothing to do with. The user sees why at once. |
| Lint note and "use the pack's values" | A reviewer sees which settings differ from the house standard; one undoable step brings one back. |

## Decisions made here

1. **Pack shape.** `control_kinds` is an optional list in the methodology object, after `attack_types`. One entry: `key`, `label`, optional `interface_kinds` (list of interface kind keys), `rules` (an object from attack type key to feasibility level key). No `interface_kinds` means the preset is for components; with it, the preset is for interfaces of those kinds. Labels per language go in `translations.<lang>.control_kinds`, like attack types.
2. **Limits.** At most `MAX_CONTROL_KINDS = 64` presets per pack. A preset has 1 to `MAX_CONTROL_RULES` (16, exists in `control.rs`) rules. All keys must be declared in the same pack.
3. **The model stores the kind and does not enforce the fit.** `Op::SetControlKind { control, kind: Option<ControlKind>, provenance }` only checks that the control and the kind exist. The fit ("for debug interfaces only") is checked by the client command, which is where the user's intent is known. Enforcing it in the model would also need guards on `SetInterfaceKind` and `ApplyControl`, with no gain for the user.
4. **Removal guard (lesson L04).** `RemoveSecurityControl` is rejected with `ModelError::SecurityControlHasKind` while the control has a kind, like targets and rules today. The inverse of a removal stays `AddSecurityControl` with no new field. The client's remove batch clears the kind first, in the same undo step.
5. **A pack must not drop a preset key between versions.** A snapshot stores the kind by key and fails to load when the key is unknown, the same rule as for attack types.
6. **The lint note is a warning.** `LintSeverity` has only `Warning` and `Error`; a third level is not worth its cost. `LintCode::ControlRulesDifferFromKind` never blocks anything.
7. **CRA and STRIDE presets are for components only.** These packs declare no interface kinds, so a project in them has no interfaces.
8. **The preset values are editable examples**, written from general engineering knowledge, not from a standard's text. The owner reviews the list in slice 124c before the job merges.

## Acceptance Criteria

1. A pack with `control_kinds` loads; `Methodology` gives each preset's key, label, label per language, interface kinds and rules. A pack without the field loads as before. Check: `cargo test --locked -p aegis-model --test control_kinds`.
2. A preset with an unknown attack type, feasibility level or interface kind, a repeated interface kind, no rule, more than 16 rules, or a repeated key is refused with a `MethodologyError` that names the preset. More than 64 presets are refused while decoding. Check: `cargo test --locked -p aegis-model --test control_kinds --test pack_limits`.
3. Writing a definition with presets and reading it back gives the same definition and the same bytes. Check: `cargo test --locked -p aegis-model --test pack --test pack_properties`.
4. The ISO pack ships 12 presets, the CRA pack 8 and the STRIDE pack 8, each with Japanese labels, and each file equals the writer's output byte for byte. Check: `cargo test --locked -p aegis-model --test builtin_pack --test stride_pack`.
5. `Op::SetControlKind` sets and clears a control's kind; its inverse restores the old kind and the old provenance exactly; it survives the op log and the snapshot; a control with a kind cannot be removed. Check: `cargo test --locked -p aegis-model --test control_presets`.
6. Every `Op` variant is still undone and redone exactly. Check: `cargo test --locked -p aegis-client --test undo_every_op`.
7. A control whose rules differ from its preset gives one `ControlRulesDifferFromKind` warning; equal rules give none; a control without a kind gives none. Check: `cargo test --locked -p aegis-model --test control_presets`.
8. `Edit::AddPresetSetting` creates the control, sets its kind, copies the rules and applies it; one undo restores the project exactly. Check: `cargo test --locked -p aegis-client --test control_presets`.
9. A preset for debug interfaces is refused on a component with the text `"Debug port locked" is a setting for Debug interfaces; it cannot be applied to a component`, and the project is unchanged. Check: same test.
10. `Edit::UsePackValues` makes the rules equal to the preset in one undo step and the warning goes away. Check: same test.
11. `pages::add_setting_list` gives, for a target and a filter text, the existing settings, the fitting pack presets and the custom entry. Check: same test.
12. The whole workspace passes. Check: `scripts/ci.sh`.
13. Owner check (not for an agent): the preset list in `factory/jobs/124-control-presets/slices/124c/build.md` is read and accepted or corrected by the owner.

## Out of scope

- **The screen.** The inline filtered list under "+ Add setting", its keyboard handling, the "(pack preset)" subtitle, the "Use the pack's values" button and the inline error are drawn by the GPUI shell in a later job. This job gives that shell the commands and the view data. No `.slint` file and no Slint glue in `aegis-shell` changes.
- Enforcing the fit of a preset inside the model.
- Presets for interfaces in the CRA and STRIDE packs (they have no interface kinds).
- "Reduced by" columns in tables and reports (CFG-07, job 125).
- Editing presets in the app; a preset is pack data.
