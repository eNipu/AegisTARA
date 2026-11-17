# Spec: 126-setting-conflicts

## Title

Conflicting settings on one component or interface (CFG-09, D26)

## Design Reference

- `docs/requirements.md`, CFG-09 (line 190): packs may declare presets that conflict; the linter warns when one component or interface has both, never blocks.
- `docs/decisions.md`, D26 (line 203) and the TLDR (line 20): settings are security controls applied to components and interfaces, with presets from the methodology pack.
- `docs/brainstorm-systems-config.md`, line 376 (the "Conflicting settings" row: the maths cannot conflict, ceilings combine with `min`; a *semantic* conflict is flagged by an optional pack `conflicts_with` list as a warning, not a block, because the consultant may be recording a real defect) and lines 503-505 (the test plan for job 126).
- `factory/jobs/124-control-presets/spec.md`: the preset shape this builds on (`control_kinds` in the pack, `ControlKind` index, `Op::SetControlKind`, `ApplyControl` targets, the `ControlRulesDifferFromKind` lint).
- Code: `crates/aegis-model/src/methodology/{mod,pack}.rs` (`ControlKindDef`, `build_control_kinds`, the wire type and the writer), `crates/aegis-model/src/{error,item,lint,project}.rs`, `crates/aegis-model/src/store/mod.rs` (`Limit`), `crates/aegis-client/src/pages.rs` (`problem`).

## Problem & Goal

The rating maths cannot conflict: ceilings combine with `min`, so the strongest setting applies and explain-why lists all of them. But two settings can contradict in *meaning*: "debug port fused" and "debug port open for field service" applied to the same interface say opposite things about the hardware. Today nothing points this out; a reviewer reads every component and interface to find such pairs by hand.

Goal: a methodology pack may declare, on a setting preset, a `conflicts_with` list of other presets. When one component or interface has settings of two conflicting presets, the linter shows one warning for that target per conflicting pair (one row, however the pair was declared). Never a block: the pair may be a real defect the consultant is recording, so the finding is a warning and no risk number, gate or export changes.

Business value of each part (P7):

| Part | Value |
|---|---|
| `conflicts_with` in the pack | The consultancy's house standard carries the knowledge once; the core stays methodology-neutral (D1, D24). |
| Resolution to indices at `Methodology::new` | The lint answers in O(1) per preset; a bad pack is refused at load, not at lint time. |
| One warning per target with both | A reviewer scans the Problems page instead of every interface; one row per target keeps the count meaningful. |
| Warning severity, never an error | The analysis is not blocked; the consultant records the real state of the hardware. |

## Decisions made here

1. **Field name and shape.** `conflicts_with`: an optional list of preset keys on one `control_kinds` entry, written after `rules`. The name comes from the backlog line and the brainstorm.
2. **Cap.** At most `MAX_CONTROL_KINDS` (64) entries, with its own `Limit::ControlKindConflicts` so the decode error names the cap honestly ("more than 64 presets in one conflicts_with list", not "control kinds").
3. **One-sided declaration, symmetric lint.** Declaring a→b is enough; the lint treats it as a→b and b→a. A pack that declares both sides gets one warning, not two. Pack authors should not have to say everything twice.
4. **Canonical pair.** The finding carries `first` and `second` with `first.index() < second.index()`, so it is order-independent (P4) and the dedupe is exact.
5. **Self-conflict is a pack error.** A preset listing itself in `conflicts_with` is refused at `Methodology::new`, like a repeated key: it is meaningless, and silently dropping it would hide an authoring mistake.
6. **The shipped packs stay without conflicts.** Their owner-reviewed preset lists (brainstorm answer 5) contain no contradicting pair, and the field is optional; the ISO, CRA and STRIDE pack files stay byte-identical.
7. **The finding references IDs and indices, never labels** (rule R6): `item: ItemRef`, `first`/`second`: `ControlKind`. The client sentence adds the preset labels.
8. **Direct applications only.** "Has both" counts controls applied directly to that component or interface, the same set `controls_applied_to` returns. A setting on the ECU does not conflict-check against its JTAG interface; that reach is CFG-04 rating behaviour and stays as it is.
9. **The Problems page is the whole UI.** It is generic over `LintCode`; one new `problem()` arm names the target and both presets. No inspector line, no canvas change.

## Acceptance Criteria

1. A pack whose preset has `"conflicts_with": ["other-key"]` decodes and loads, also when `other` is declared later in the same pack (forward reference); `Methodology::control_kind_conflicts(kind)` returns the resolved `ControlKind` indices, sorted; a pack without the field loads exactly as today. Check: `cargo test --locked -p aegis-model --test control_kinds`.
2. Writing a definition with a conflicts list and reading it back gives the same definition and the same bytes twice; a preset without conflicts writes no `conflicts_with` line, so existing pack files are byte-identical. Check: the same command, plus `--test builtin_pack --test stride_pack`.
3. A conflicts entry naming an undeclared preset, a repeated entry, or the preset's own key is refused with a `MethodologyError` that names the preset; a list of more than 64 entries is refused at decode with `PackError::LimitExceeded(Limit::ControlKindConflicts { max: 64 })`; 63 entries (all other presets) load. Check: `cargo test --locked -p aegis-model --test control_kinds --test pack_limits`.
4. One warning per component or interface that has settings of both conflicting presets: `LintCode::ConflictingSettings { item, first, second }` with `first.index() < second.index()`; the same pair on two targets gives two warnings; two conflicting pairs on one target give two warnings; without both presets on one target (only one applied, two controls of the same kind, kinds on different targets, or a custom setting without a preset) there is none. Check: `cargo test --locked -p aegis-model --test setting_conflicts`.
5. The warning never blocks: its severity is `Warning`, and no finding of the new code is an error. Check: the same command.
6. The Problems page shows one row per warning, naming the component or interface and both preset labels, and opens that target. Check: `cargo test --locked -p aegis-client --test pages`.
7. Existing settings and preset tests pass unchanged, including exact lint lists in their fixtures (the shipped packs declare no conflicts). Check: `cargo test --locked -p aegis-model --test control_presets --test control_settings`.
8. The whole workspace passes. Check: `scripts/ci.sh`.

## Out of scope

- Declaring conflicts in the shipped ISO, CRA or STRIDE packs; the owner adds pairs when the house standard has them.
- Any block, error severity, or a suggestion to remove one of the two settings.
- Conflict checks between a component and its interfaces (a component setting reaching steps via its interfaces is CFG-04 rating behaviour and stays as it is).
- Client UI beyond the one Problems row: no inspector line, no canvas marking, no report change (CFG-07 surface stays as job 125 built it).
- Syncing the stale `LintCode` diagram in `docs/architecture.md` (it already misses the systems codes of jobs 119-121); not this job.
