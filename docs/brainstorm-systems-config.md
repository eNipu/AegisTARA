# Brainstorm: several systems in one project, and settings on ECUs and interfaces

- Status: **accepted** by the owner on 2026-11-02 (decision D26 in
  [decisions.md](decisions.md)), with one change from the first draft: **each
  component belongs to at most one system**. A component that several systems
  need (the gateway) becomes a system of its own. Section 5.2 records the
  change and why. The answers to section 10 are recorded there.
- Date: 2026-11-02
- Replaces the open architecture question in
  [brainstorm-multi-tara.md](brainstorm-multi-tara.md) section 4.1 and the plan
  in its section 6. That note stays as the record of the first idea.
- UX sketches: [design/systems-config.md](design/systems-config.md).
- Requirements: SYS-01 to SYS-06 (rewritten) and CFG-01 to CFG-09 (new) in
  [requirements.md](requirements.md) section 2.5a and 2.5b.
- Backlog: jobs 122 to 126 (new) and 119 to 121 (rewritten) in
  `factory/backlog.md`.

## 1. In one screen

The owner asked for four things:

1. One project holds several TARAs ("systems"), for example Brake, ADAS and
   Backend. The user looks at one, a chosen subset, or all of them as one TARA.
2. A system has many ECUs and sensors, each with many interfaces (Ethernet,
   Bluetooth, CAN, USB, debug port, ...).
3. The user sets how an ECU or interface is secured (physical separation,
   physical access disabled, secure boot, secure flash, ...), and those
   settings change the TARA result.
4. Odd cases are handled by custom settings, not by code changes.

The proposal, in plain words:

- **A system is a named group of components** inside the one project, with
  its own boundary: **each component is in at most one system**. A component
  that several systems use (the central gateway) becomes a system of its own
  ("Gateway"), linked to the others by data flows that cross the boundary.
  Everything else (interfaces, assets, threats, trees) belongs to a system
  *through its links*, so only the component gains a system field. Looking at
  "Brake + ADAS" is a filter over the one project, not a copy. A risk number
  never depends on which systems you are looking at.
- **A setting is a security control applied to a component or an interface.**
  Controls already exist and already cap attack-step feasibility
  ([ADR 0017](adr/0017-security-controls-and-residual-risk.md)). Today the user
  attaches a control to each step by hand. The proposal adds two small facts to
  an attack step: **which component or interface it goes through** ("via") and
  **what kind of attack it is** ("attack type", a pack-defined list). A control
  carries **rules**: "for attack type X, cap feasibility at level Y". The app
  then caps every matching step on that component automatically, and the
  explain-why says "capped by: Secure boot on ADAS ECU".
- **The methodology pack** supplies the attack types and a set of ready-made
  settings (presets with default rules). A custom setting is the same thing
  with rules the user types. No code change for odd cases.
- **Settings first, systems second.** Settings help every project today,
  including single-system ones; systems are a filter that can land after.

## 2. What a consultant does by hand today

The usual routine in Excel-based TARA work (the kind of workbook job 080
imports):

1. One Excel workbook per system (Brake, ADAS, ...), or one huge workbook with
   a "System" column. Shared ECUs (the central gateway, the telematics unit)
   are copied into each workbook, and the copies drift.
2. An "architecture" sheet lists ECUs, their interfaces and a column per
   security measure: "Secure boot: yes", "JTAG: fused", "Physical access:
   housing sealed". These columns are documentation. Nothing links them to the
   feasibility ratings.
3. For each threat the consultant rates attack feasibility, then reads the
   architecture sheet and lowers the rating by hand, writing the reason in a
   free-text "rationale" cell ("lowered because secure boot is on").
4. When the client changes a setting (secure flash dropped to save cost), the
   consultant searches the workbook for every threat that relied on it and
   re-rates them. Misses are common, and the reviewer cannot see which ratings
   depend on which setting.
5. For the vehicle-level report, the per-system workbooks are pasted together,
   the copied gateway threats are counted twice, and totals are fixed by hand.

What the consultant needs from the tool: settings recorded once per ECU or
interface, ratings that follow when a setting changes, a reason the reviewer
can read, and one combined register with no double counting. That is the
business value; everything below is measured against it.

## 3. What the app has today (checked in the code, 2026-11-02)

| Question | Answer | Where |
|---|---|---|
| How many TARAs per project? | One flat `Project`: one item definition, one set of assets, threats, trees, controls, goals, claims | `crates/aegis-model/src/project.rs` |
| One op log, lease and audit chain? | Yes, per project: `aegis_project.last_seq`, `last_hash`; `aegis_project_op` holds `prev_hash` and `hash` per op | `crates/aegis-server/migrations/001_init.sql` |
| Baselines | Whole-project snapshots | `aegis_baseline` in `migrations/002_accounts_baselines.sql` |
| Methodology | One per project (`methodology_key`, `methodology_version`) | `migrations/001_init.sql` |
| Components | Name, optional parent (depth up to 16); **no kind** (ECU or sensor is just the name) | `Component` in `crates/aegis-model/src/item.rs` |
| Interfaces | Name, one component, a pack-defined `InterfaceKind` (the ISO pack has `can`, `ethernet`, `wireless`, `debug`, `cellular`, `usb`) | `Interface` in `item.rs`; `interface_kinds` in `packs/iso21434-r155.json` |
| Data flows | From and to a component, optional interface | `DataFlow` in `item.rs` |
| Trust boundaries | A named set of up to 64 components | `TrustBoundary`, `MAX_TRUST_BOUNDARY_MEMBERS` in `item.rs` |
| Threat ↔ asset ↔ component | A threat targets exactly one asset; an asset may be bound to one component (`Op::SetAssetComponent`) | `ThreatScenario.asset` in `threat.rs`; `op.rs` |
| Controls | `SecurityControl { name, provenance }`: **no type, no settings, no link to a component or interface** | `crates/aegis-model/src/control.rs` |
| How a control acts | `Op::SetStepMitigation { tree, node, control, feasibility }` puts a ceiling on one step; residual = min(inherent, ceilings); at most 16 per step | `control.rs`, `op.rs`, ADR 0017 |
| Do steps know their component or interface? | **No.** A tree node has a name, gate, ratings and mitigations; nothing points at the item definition | `crates/aegis-model/src/tree.rs` |
| Client edits | `Edit` expands to ops; `Edit::Batch` is one undo step (bulk edit, PR #20) | `crates/aegis-client/src/commands.rs` |
| Every op undoes exactly | A test applies and undoes every `Op` kind | `crates/aegis-client/tests/undo_every_op.rs` |

So nothing connects an ECU or interface setting to the attack trees or the
risk. Gap G49 in [requirements.md](requirements.md).

## 4. User stories

| # | As a ... | I want to ... | So that ... |
|---|---|---|---|
| U1 | consultant | add systems Brake, ADAS, Gateway and Backend to one vehicle project and put each ECU in its system | one project holds the whole vehicle with no copies, and each system has a clear boundary |
| U2 | consultant | look at Brake only, or Brake + ADAS, or everything | I can work and report at the level the client asks for |
| U3 | consultant | model the gateway as its own system, and see from Brake which attacks go through it | a gateway setting lowers Brake and ADAS risks with no copy of the gateway |
| U4 | consultant | list the interfaces of an ECU and mark how each is secured (debug port fused, Bluetooth pairing with PIN) | the architecture sheet lives in the tool |
| U5 | consultant | set "secure boot" on an ECU and see which attack steps and risks it lowers | I do not re-rate by hand |
| U6 | consultant | remove "secure flash" and see at once which risks go up, with Undo | I can show the client the cost of dropping a measure |
| U7 | reviewer | read why a step is rated Low: "capped by: Secure boot on ADAS ECU" | I can check the reason, not trust a free-text cell |
| U8 | consultant | add a setting the pack does not know ("vendor-specific seed-key with HSM") with my own rules | odd cases need no code change |
| U9 | consultant | produce one report for the vehicle with per-system totals and no double counting | the vehicle-level deliverable needs no hand fixing |

## 5. Architecture decision: systems inside one project, or a program of projects

### 5.1 The two options

| | A. Systems inside one project (recommended) | B. Program of projects (one project per system) |
|---|---|---|
| ECU used by two systems | Its own system (Gateway), linked by boundary flows; steps in Brake trees go via it | A copy in each project, or a cross-project reference the model does not have |
| Undo | One history; one undo step can span systems (moving the gateway into ADAS) | One history per project; an action that touches two systems is two undo steps in two windows |
| Edit lease | One lease per project, as today (D4) | One per system; two analysts can edit Brake and ADAS at the same time |
| Baselines | One baseline is the whole vehicle at one moment | A baseline per system; a "vehicle baseline" is a new set-of-baselines concept |
| Diff | `Project::diff` as today, filtered by the view | Diff per project; combined diff needs a new merge step |
| Audit chain | One chain, unchanged; new ops are ordinary ops | One chain per project; nothing proves the set was consistent at one time |
| Reports | One report pipeline with a systems filter | Reports must read several projects and their versions |
| Excel import | One import; an optional "System" column puts each component in its system | One import per system; the shared rows are imported twice |
| Methodology | One per project, so all systems share scales (combined numbers are comparable) | Each project could pick a pack; combining different scales has no meaning |
| Server and standalone | No schema change; standalone folder unchanged | A new "program" table and a parent folder format; import and export of a program |
| Access per system | No (project-level, ACC-01) | Yes |
| Migration | None: a project with no systems is today's project | Existing projects become single-system members of a new program |

**Decision: A.** Every row the consultant feels (the gateway, one undo,
one baseline, one report, one import) favours A, and A needs no server change.
B buys two things: parallel editing of two systems and access per system.
Neither has a customer asking for it, and the consultancy-first plan (D21,
D22) means one consultant per project most of the time. If a customer needs
parallel editing later, a per-system lease can still be built on the single op
log (section 5.3).

### 5.2 One system per component (the owner's rule)

Three shapes were considered:

| Shape | What it means | Verdict |
|---|---|---|
| A `system` field on every element and op ([brainstorm-multi-tara.md](brainstorm-multi-tara.md) section 4.2) | Each asset, threat, tree, control ... names its system | Too much: every op changes, and the field can disagree with the links (an asset in Brake bound to an ADAS component) |
| A membership list per system (this note's first draft) | A component can be in several systems | Blurs the boundary: a shared ECU's threats show in two TARAs, every total needs "counted once" rules, and a reviewer cannot tell which TARA owns the gateway |
| **One system per component** (decided) | `Component.system`, at most one; a component several systems need becomes its own system | Clear boundaries, as an ISO/SAE 21434 item definition has; totals add up; moving the gateway out is one edit |

The decided model:

- `System { name, description, provenance }` in a new arena on `Project`,
  capped (for example 32; the builder sets it with a benchmark, rule R3).
- `Component.system: Option<SystemIdx>`, set only on top-level components.
  A child component (an HSM inside the gateway) is in its parent's system.
  The model rejects a system on a child, and rejects moving a component with
  its own system under a parent (the client's batch clears it first, so the
  child then follows its new parent).
- Everything else is **derived** from links the model already has:
  interface → its component; asset → its component; threat → its asset;
  attack tree → the threats that use it. Each of these is in at most one
  system too, because a component is.
- Elements that describe consequences or measures may be used by several
  systems, and that is correct: a damage scenario ("loss of braking
  deceleration") linked by threats in Brake and Gateway; a control ("Secure
  boot") applied to ECUs in two systems; a goal or claim on threats in two
  systems. They show in each system that uses them and once in a combined
  view. Forcing a copy per system would bring back the drift of section 2.
- A data flow whose two ends are in different systems is a **boundary flow**.
  It is how systems link: ADAS ECU → Gateway → ESC ECU.
- An element with nothing to derive from (a component with no system, an
  asset with no component, a damage scenario no threat links) shows in every
  view, marked "not in a system". The linter already flags the unbound asset;
  one new finding flags a component in no system once the project has systems.
- A project with no systems looks exactly like today: no switcher, no column.
  The whole project is then one TARA, as the owner noted it can be.

What this buys: no change to any existing op, no store migration (the field
is omitted when empty), v1 fixtures load as they are, removing a system
removes no data (its components become "not in a system"), and moving a
component to another system is one op that carries its interfaces, assets and
threats along. It costs one arena, four ops and one field.

### 5.3 How each concern works under A

- **Undo.** System ops are ordinary ops with exact inverses, so
  `undo_every_op.rs` covers them. Removing a system is a batch: clear the
  system of each of its components, then remove the empty system (one undo
  step, an undo toast, UI-10.6). After
  an undo, UI-10.5 selects the changed element; if the current view hides it,
  the view adds that element's system and the scope chip says so.
- **Lease.** One per project, unchanged. Later, if needed: a per-system lease
  would grant the right to apply ops whose targets derive into that system.
  It is possible on one op log, but not planned.
- **Baselines.** Whole project, unchanged. The Versions page compares within
  the current view (diff lines filtered by scope).
- **Diff.** `Project::diff` gains system lines ("Gateway: moved from Brake to
  system Gateway"); `pages::comparison()` filters by the view.
- **Audit chain.** Unchanged. The view is per-user UI state, never an op, so
  looking at Brake leaves no trace in the evidence (design principle 8 in
  [ui-ux-requirements.md](ui-ux-requirements.md)).
- **Reports.** The report builder gains a systems filter next to its asset and
  component filters (job 071). The cover names the systems covered; a combined
  report lists a damage scenario or control used by two systems once. The
  profile and the export audit event
  record the systems (RPT-03).
- **Excel export and import.** Export adds a "System" column (for damage
  scenarios and controls used by several systems, the names joined with ", ").
  Import (job 080) can map a "System" column: each name becomes a system
  (created if missing) and the row's component is put in it. Same dry-run
  preview, same one batch.
- **Visual IDs (job 118).** Stay per project, with no system prefix. An ID
  stays the same when a component moves to another system, so a reviewer's
  citation does not break.
- **Methodology.** One per project. All systems share scales, so combined
  totals mean something.
- **Access.** Per project (ACC-01). Per-system access is not planned.

## 6. Configuration model

### 6.1 The pieces

| Piece | What it is | Where it comes from | What it buys |
|---|---|---|---|
| Step **via** | An attack step names at most one component or interface it goes through ("Flash modified firmware" via *ADAS ECU*; "Open JTAG session" via *ADAS ECU / JTAG*) | Set by the user on a leaf step; the library can fill it on insert | Connects the trees to the item definition: settings can reach steps, and "steps through this ECU" can be listed |
| Step **attack type** | At most one pack-defined key per leaf step ("firmware modification", "debug access", "physical tampering", "remote network", "wireless proximity", "flash read-out", ...) | List in the pack, with labels per language; chosen by the user | Lets a setting cap only the attacks it really stops (secure boot stops firmware modification, not CAN spoofing) |
| Control **applied to** | A control is applied to components and interfaces (capped, for example 64) | Set by the user in the inspector | Records the setting once, where it lives, as the architecture sheet did |
| Control **rules** | Pairs "attack type → feasibility ceiling", at most 16 per control | Copied from a pack preset, or typed by the user | Turns the setting into a deterministic effect on ratings |
| Control **kind** | Optional pack preset key ("secure-boot") | Set when the control is created from a preset | Labels, grouping in reports, the conflict lint (CFG-09) |
| Pack **presets** | `control_kinds`: key, labels per language, applies to component or interface (optionally only some interface kinds), default rules | The three shipped packs, as editable examples | The consultant picks "Secure boot" instead of typing rules |

Existing pieces stay: manual `SetStepMitigation` (a control's ceiling on one
named step) remains the escape hatch for anything the rules cannot express.

### 6.2 The rule (one line of maths, all derived)

For a leaf step *s* with via *v* and attack type *t*:

```text
reach(s)    = controls applied to v,
              plus controls applied to v's component when v is an interface
ceilings(s) = { rule.ceiling | c in reach(s), rule in c.rules, rule.attack_type = t }
              ∪ { m.feasibility | m in manual mitigations of s }   (today's rule, ADR 0017)
residual(s) = min(inherent(s), min ceilings(s))
```

- A component-level setting reaches steps via the component **and** via its
  interfaces (secure boot on the ECU also covers firmware flashed through its
  USB port). An interface-level setting reaches only steps via that interface.
- Settings do not flow down the parent chain: a setting on the gateway does
  not reach the HSM inside it. The user applies it to the HSM if it is true
  there. (Downward flow would make the effect of one click hard to predict.)
- A step with no via or no attack type gets no automatic ceiling. Manual
  mitigations still apply.
- Nothing is stored: ceilings are recomputed from the facts above. Inherent
  ratings never change. The incremental cache (ADR 0023) marks a step dirty
  when a control's rules or targets change, or the step's via or type changes.
- Order does not matter: `min` is commutative, so the same facts give the same
  number however they were entered (P4).

**"Closing" an attack path.** Physical separation or a removed debug port is
modelled as a ceiling at the pack's lowest level (Very low in the ISO pack),
shown as "closed" in the explain-why. The tree stays as it is. A real "no
path" state would need a value below the lowest level in every pack and a risk
for "no feasibility"; no shipped methodology has one, and consultants rarely
claim "impossible" (chip-off still exists). Question Q2 in section 10.

### 6.3 Worked example

ADAS ECU has interfaces *CAN-1*, *USB service port*, *JTAG*.

| Setting | Applied to | Rules (from the ISO preset) |
|---|---|---|
| Secure boot | ADAS ECU | firmware modification → Low |
| Debug port fused | ADAS ECU / JTAG | debug access → Very low (closed) |
| Housing sealed, tamper evident | ADAS ECU | physical tampering → Low |

Threat T07 "Spoofed brake request from ADAS" has an OR tree:

| Step | Via | Attack type | Inherent | Residual | Capped by |
|---|---|---|---|---|---|
| Flash modified firmware over USB | ADAS ECU / USB service port | firmware modification | High | Low | Secure boot on ADAS ECU |
| Read keys over JTAG | ADAS ECU / JTAG | debug access | Medium | Very low | Debug port fused on ADAS ECU / JTAG |
| Inject frames on CAN-1 from OBD | Gateway / OBD | remote network | Medium | Medium | — |

OR takes the easiest branch, so T07's residual feasibility is **Medium**, decided
by the CAN step. The explain-why says so, and the choke-point view (ADR 0024)
names the CAN step as the one to fix next. This is the honest answer the
consultant used to work out by hand.

### 6.4 Explainability

Every residual value carries its reason. The residual step trace gains one
record per ceiling: source (rule or manual), control, target element, attack
type, level. The UI writes them as one line each:

- EN: `Low — capped by: Secure boot on ADAS ECU (firmware modification → Low)`
- JA: `低 — 上限: ADAS ECU のセキュアブート（ファームウェア改ざん → 低）`
- Manual: `Low — capped by: Bootloader patch (set on this step)`
- No cap: `High — not capped (no setting covers "remote network" via Gateway / OBD)`

Shown in: the step inspector, the threat and risk inspectors (for the step on
the why-path), a badge on the canvas node, a "Reduced by" column in the risk
table, reports and XLSX. After a settings change, the undo toast says what
moved: "Secure boot applied to ADAS ECU: 3 steps capped, 1 risk lowered (T07
4 → 3). Undo". The client gets those counts by comparing residual risks
before and after the edit in its own workspace: no server call.

### 6.5 What is deterministic, and what a person rates

| Deterministic (the engine) | A person decides |
|---|---|
| Which steps a setting reaches (via + attack type + target) | The step's inherent rating (as today) |
| The ceiling a rule gives | The step's via and attack type |
| The residual of every step, tree, threat and risk | That a setting is true for this ECU or interface |
| The explain-why lines and the "Reduced by" column | The ceiling in a custom rule, or a change to a preset's ceiling on one control |
| Which risks moved after an edit | The pack author's default ceilings (a methodology owner reviews them once) |
| Systems of every element (derived) | Which components are in which system |

The shipped presets are **examples**, like the CRA pack's risk matrix (job
111): a consultant reviews them before using them on a client project. The
pack says so in its description.

### 6.6 Custom settings

A custom setting is a control created with no preset: a name, the targets,
and rules the user picks from the pack's attack types and feasibility levels.
It behaves exactly like a preset-based one. Presets are **copied** into the
control when it is created, so a later pack version never changes a saved
result without an op (the audit chain shows every change to a rule).

If even the attack type list does not fit, the user still has the manual
mitigation on a single step (today's feature). Custom attack types per project
are not planned (section 8).

### 6.7 Model changes in short (for the builder)

New ops, each with an exact inverse, all additive (store fields omitted when
empty, v1 fixtures load unchanged):

| Op | Inverse | Job |
|---|---|---|
| `SetStepVia { tree, node, via: Option<ItemRef> }` (`ItemRef` = component or interface) | `SetStepVia` with the old value | 122 |
| `SetStepAttackType { tree, node, attack_type: Option<AttackType> }` | same, old value | 122 |
| `ApplyControl { control, target: ItemRef }` / `UnapplyControl { ... }` | each other | 123 |
| `SetControlRule { control, attack_type, ceiling: Option<FeasibilityLevel> }` | same, old value | 123 |
| `SetControlKind { control, kind: Option<ControlKind> }` | same, old value | 124 |
| `AddSystem`, `RenameSystem`, `SetComponentSystem { component, system: Option<SystemId> }` (top-level components only), `RemoveSystem` (only when no component is in it) | the matching op | 119 |

Remove rules follow today's pattern (the model rejects; the client sends a
batch): `RemoveComponent` and `RemoveInterface` are rejected while a control is
applied to them or a step goes via them; `RemoveSecurityControl` is rejected
while it has targets or rules; the client's remove expands to unapply, clear,
then remove, in one undo step with a toast that names what else changed
(UI-10.10). The `SecurityControlUnused` lint counts a control applied to a
target as used. Pack format: `attack_types` and `control_kinds` are optional lists,
so packs without them load and behave as today.

## 7. Corner cases

| Case | What happens |
|---|---|
| **An ECU two systems need (the gateway)** | It becomes a system of its own, "Gateway", with its interfaces, assets, threats and settings. Brake and ADAS link to it by boundary flows. Splitting it out of Brake is one op (`SetComponentSystem`), one undo step; its interfaces, assets and threats move with it. |
| **An attack that crosses systems** (ADAS compromised → brake request through the gateway) | The threat is on a Brake asset, so the tree is in Brake. Its steps go via ADAS and Gateway components, which the step's via chip shows with their system ("Gateway · Gateway system"). Gateway and ADAS settings cap those steps, with no tree-to-tree link, and the number is the same whether Brake is viewed alone or with the others. |
| **A sensor that feeds several systems** (wheel speed to Brake and ADAS) | Put it in the system that owns it (Brake) and draw a boundary flow to the ADAS component that reads it. If it matters on its own (many consumers, its own threats), make it a system ("Chassis sensors"). Either way it is in one system. |
| **A damage scenario or control used by two systems** | Allowed: these describe consequences and measures, not parts. "Loss of braking deceleration" is linked from Brake and Gateway threats and shows in both; in the combined view it is listed once. |
| **A component with no system** once systems exist | Shown in every view as "not in a system", and a lint finding lists it, so nothing falls outside every TARA unnoticed. |
| **A setting for one interface only** | Apply the control to the interface, not the ECU. It reaches only steps via that interface. A component-level setting also reaches its interfaces; the interface inspector shows those as "from ADAS ECU". |
| **A setting the methodology does not know** | A custom setting: name, targets, rules over the pack's attack types. If no attack type fits, the manual ceiling on one step. No code change in either case. |
| **Conflicting settings** | The maths cannot conflict: ceilings combine with `min`, so the strongest applies and the explain-why lists all of them. A *semantic* conflict ("debug port fused" and "debug port open for field service" on one interface) is flagged by an optional pack `conflicts_with` list as a lint warning, not blocked (job 126): the consultant may be recording a real defect. |
| **A rule is too broad for one step** (secure boot does not stop this bootloader exploit) | Give the step the attack type that matches what it really is (for example "bootloader exploit"), which the preset does not cap. No per-step exception lists. |
| **A step with no via** | No automatic cap; a lint info lists leaf steps with no via once the project has any applied setting, so gaps are visible. |
| **Removing a component that has settings** | One batch: unapply settings, clear step vias, remove. Toast: "Removed ADAS ECU and 3 settings; 4 steps lost their via. Undo". |
| **Pack upgraded with different preset ceilings** | Saved controls keep their copied rules. A lint info names controls whose rules differ from their preset's current defaults, with a one-click "use the pack's values" (a normal undoable batch). |
| **A setting that makes things worse** (debug port left open) | Not a setting. Exposure is modelled as today: the step exists and its inherent rating says how easy it is. Settings only lower feasibility. |

## 8. What we cut, and why

| Idea | Why it is out | Add it when |
|---|---|---|
| A `system` field on every element and op (first idea) | Only the component needs it; the rest follows from links and cannot disagree with them | never |
| A component in several systems (first draft of this note) | Blurs each system's boundary and needs "counted once" rules everywhere; a system of its own is clearer (owner, 2026-11-02) | never |
| Program of projects | Section 5.1 | a customer needs two analysts editing two systems at once, or per-system access |
| Tree-to-tree links across systems (old SYS-04 idea) | "Via" on a step lets ADAS and Gateway settings cap Brake steps already; tree links need cross-tree cycle checks and a second explain-why path | a consultant shows a case via cannot express |
| Component kinds (ECU, sensor, actuator) | The name already says it; no rule needs to tell them apart yet | a pack preset must apply only to ECUs, or the diagram (075) needs icons |
| Factor-level effects (raise "window of opportunity" to Difficult) | One effect type (a ceiling) works for every pack, including plain likelihood (D24); factor effects would only fit attack potential | the ISO pack's users ask for factor-level reasoning |
| A true "closed" state below the lowest level | Section 6.2 | a methodology needs "no path" with its own risk rule (Q2) |
| Settings that raise feasibility | Exposure is the inherent rating's job | never |
| Custom attack types per project | Packs are the vocabulary (D1, D24); a pack copy is easy to edit | many consultants hit the same missing type |
| "Planned" vs "implemented" settings | Inherent vs residual already shows with and without; a status adds a third number | a customer's process requires residual risk "as planned" and "as built" side by side |
| Settings flowing down the parent chain | Hard to predict what one click changes | never |
| Stored combined results | A view is recomputed; stored results drift | never |

## 9. Plan: slices that each ship alone

Settings first: they help every project, single-system ones included, and the
systems work does not depend on them. The two tracks touch the same model
files, so they run one after the other, not in parallel.

| Order | Job | What ships | Requirements | Size |
|---|---|---|---|---|
| 1 | 122 | Step via and attack type: ops, store, diff, lint, pack `attack_types` (EN, JA) in the three packs, two fields in the step inspector, a via chip on canvas nodes, a "Steps through this" list in the component and interface inspectors | CFG-01, CFG-02 | S |
| 2 | 123 | Settings: apply a control to components and interfaces, control rules, the derived ceilings in residual evaluation and the cache, explain-why lines, "Security settings" section in the component and interface inspectors, custom settings, canvas badge, undo toast with what moved | CFG-03, CFG-04, CFG-05, CFG-08 | M |
| 3 | 124 | Presets: pack `control_kinds` with default rules in the three packs (EN, JA), control kind, "Add setting" menu grouped by preset, the "rules differ from preset" lint | CFG-06 | S |
| 4 | 125 | "Reduced by" in the risk and control tables, a "Security settings" report section per component, XLSX columns | CFG-07 | S |
| 5 | 119 | Systems: arena and ops, store, diff, lint, systems table on the item definition page, a system picker in the component inspector, the view switcher (one, a subset, all) filtering every page | SYS-01, SYS-02, SYS-03 | M |
| 6 | 120 | Combined view: the overview's per-system totals and boundary flows, top paths and choke points within the view, Versions diff within the view, undo revealing hidden elements | SYS-04, SYS-05 | S |
| 7 | 121 | Reports and exports by system: builder filter, cover lists systems, XLSX Systems column, Excel import System column | SYS-06 | S |
| 8 | 126 | Conflicting settings: optional pack `conflicts_with`, one lint warning | CFG-09 | XS |

Jobs 119 to 121 keep their numbers but are rewritten: 119 puts the system on
the component only (the rest is derived); 120 loses "composing trees across
system boundaries" (a view, no tree links) and is renamed
`120-system-overview`; 121 is unchanged in scope.

### 9.1 Acceptance tests per slice

Model property tests go in `crates/aegis-model/tests/` (proptest, as in
`eval_properties.rs` and `ops_properties.rs`); client tests in
`crates/aegis-client/tests/`; headless shell tests in `crates/aegis-shell/tests/`
(driving the real window, as `bulk_edit.rs` and `history_panel.rs` do).

**122 Step via and attack type**
- Model: random op sequences with the new ops round-trip through the store and
  through undo (extend `ops_properties.rs`, `store_properties.rs`); a gate node
  is rejected (`NotAStep`), as `SetStepFeasibility` is; removing a component
  or interface that a step goes via is rejected.
- Model: v1 fixtures and packs without `attack_types` load unchanged.
- Client: `undo_every_op.rs` covers both ops; the remove-component batch
  clears vias and undoes as one step.
- Shell: pick via and attack type for a step by keyboard only; the via chip
  shows on the canvas; Japanese labels show with the ja locale.

**123 Settings**
- Model property: for random trees, steps and controls, residual ≤ inherent
  for every step and threat; the result is the same for any order of the same
  ops; with no applied controls and no rules, every residual equals today's
  (`evaluate_residual_*` unchanged); a component-level control reaches steps via
  its interfaces, an interface-level control reaches no step via a sibling
  interface or via the component itself.
- Model: the incremental cache agrees with a full re-evaluation after every
  random settings op (extend the ADR 0023 property test).
- Model: explain-why lists every ceiling with its source, and the deciding one
  first.
- Client: the edit that applies a setting yields a toast text with the number
  of capped steps and lowered risks; Undo restores the residuals exactly.
- Shell: apply "custom setting" to an interface from the inspector with no
  modal (`dialogs.rs` allow list unchanged); the step inspector shows "capped
  by" in EN and JA; read-only users see the section without edit controls.

**124 Presets**
- Model: the three packs decode with `control_kinds`; `builtin_pack.rs` checks
  canonical output; a preset limited to interface kinds cannot be applied to a
  component (inline error, no op).
- Client: creating from a preset is one batch (add control, kind, rules,
  apply) and one undo step.
- Shell: the "Add setting" menu lists presets for the selected element's kind,
  filtered as you type, in EN and JA.

**125 Reduced by**
- Client: risk rows carry "reduced by" names in why-path order; XLSX cells are
  formula-guarded like other text.
- Shell: the column shows, truncates with a tooltip, and is in the report
  preview.

**119 Systems**
- Model property: derived scope is a union: `scope(A ∪ B) = scope(A) ∪
  scope(B)`; every component, interface, asset and threat is in at most one
  system; an element with no link is in every scope; removing a system changes
  no element except clearing its components' system; system ops round-trip and
  undo exactly.
- Model: a system on a child component is rejected; moving a component with a
  system under a parent is rejected.
- Model: v1 fixtures load with no systems; a project with no systems shows
  no system UI.
- Client: page rows are filtered by the view; the view is a user setting per
  project, not an op (op log length unchanged after switching).
- Shell: switch view with keyboard only (Tab to the chip, arrows, Space);
  remove a system with an undo toast; the chip reads "Brake +1" in a 900 px wide
  window.

**120 Combined view**
- Model property: a risk number is the same in every view that contains its
  threat.
- Client: the per-system rows add up to the "All" row for components, assets
  and threats; a damage scenario used by two systems is in both rows and once
  in "All"; boundary flows are the flows with ends in different systems.
- Shell: undo of an edit outside the view adds that system to the view and
  selects the element.

**121 Reports by system**
- Client: a report profile with systems round-trips; the export audit detail
  names the systems; Excel import with a System column creates systems and
  puts components in them in one batch with a dry-run preview.
- Shell: the report builder's systems filter is keyboard reachable.

**126 Conflicts**
- Model: a pack with `conflicts_with` decodes; the lint fires once per target
  with both kinds and not at all without them.

## 10. Questions for the owner, and the answers (2026-11-02)

The owner agreed with every recommendation and changed one point of the
model: each system has its own boundary, and a component several systems need
becomes its own system (section 5.2).

| # | Question | Answer |
|---|---|---|
| 1 | Are systems groups of components inside one project, with everything else following its links? | Yes, inside one project. Changed: a component is in at most one system; the gateway is its own system. |
| 2 | "Closing" an attack path: cap at the lowest level and label it "closed", or a real "no path" state? | Cap at the lowest level. |
| 3 | Settings before systems (jobs 122 to 125 first)? | Yes. |
| 4 | Cross-system attacks through components and "via", with no tree-to-tree links for now? | Yes. |
| 5 | The shipped presets and their default ceilings are editable examples the owner reviews before client use? | Yes. The owner reviews the ISO pack's list (about 12 presets: secure boot, secure flash, debug port fused, physical access disabled, housing sealed, physical separation, message authentication on CAN, TLS on Ethernet, Bluetooth pairing with PIN, USB disabled in production, firewall on the gateway, rate limiting) before job 124 ships. |
