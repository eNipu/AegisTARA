# Reference review: a commercial TARA tool, screen by screen

- Status: decided by the architect review on 2026-11-07 (decisions D27 to D31
  in [decisions.md](../decisions.md)); the owner can change any line.
- Date: 2026-11-07
- Source: 20 screenshots of a commercial C# TARA desktop tool (Bosch
  CycurRISK, the "Armadillo" training project), plus its process diagram (the
  five phases of ISO/SAE 21434 clause 15 with RQ numbers).
- Rule for this review: **take ideas, never copy.** We copy no layout, icon,
  wording, colour or element name that is the tool's own. Generic TARA terms
  (assumption, attack tree, security claim) are the standard's, not theirs.
  Every idea below is re-expressed in our model, our modeless rules
  ([ui-ux-requirements.md](../ui-ux-requirements.md) section 2) and our
  decisions D1 to D26. Nothing here replaces an earlier decision; where an
  idea touches one, the section says which and how it stays true.
- Output: requirements in [requirements.md](../requirements.md) (APL, BLK,
  CFG-10 and CFG-11, FND-08 to FND-13, ARC-05, TRC-05, REV, RPT-09, UI-13
  to UI-19, NFR-12, MTA), architecture in [architecture.md](../architecture.md) Part III
  (III.6, III.7, III.9, III.10), jobs 127 to 159 in
  [factory/backlog.md](../../factory/backlog.md) (Phase 5b).

---

## 0. What the reference tool gets right, and where it stops

| It does well | We adopt as |
|---|---|
| The sidebar follows the TARA steps in order, item definition first | UI-13, nav groups by phase (job 127) |
| Every list has the same shape: ID, name, links shown as IDs, computed values greyed, comment last | UI-14, one table convention (job 129) |
| **Switches that take things out of the analysis**: TOE configuration items, assumptions, an attack filter, control on/off. Off items are hatched and the inspector says why | D27, derived applicability (jobs 133 to 139) |
| Two control toggles, "active for TARA" and "active for residual analysis" | D28, one control status with three values (job 140) |
| Reusable building blocks: attack leaves rated once, technical attack trees used by several trees, a circumvent tree per control | D29, shared steps, shared fragments, bypass trees (jobs 141 to 144) |
| Assumptions turn into security claims, and claims and goals name who is responsible (us, customer, supplier) | FND-08 parties, APL-04 assumption → claim (jobs 136, 145) |
| A reason column on damage scenarios and on each threat × damage pair | FND-09 rationale (jobs 146, 147) |
| A column-by-column traceability graph with filters and counts | TRC-05 (job 151) |
| Glossary, related documents, management summary, review summary | FND-11, FND-12, RPT-09, REV (jobs 154 to 156) |
| "External results" imported from another project | MTA-01, in the backlog (job 158, design session) |

| It stops short | Our answer |
|---|---|
| One TARA file per project; external results come in as numbers only, and a project cannot be **assembled from several TARA files** that each stay workable on their own | Backlog: composite projects (MTA-01 to MTA-04, D31, job 159 `[design]`). D26 (systems inside one project) stays the answer for one team's vehicle |
| A flat sidebar of 25 entries; you must know where things are | Grouped, collapsible, counts and problem badges (UI-13) |
| An "Auto evaluation" switch, because evaluation is slow enough to turn off | **Skip.** Our engine is incremental (ADR 0023, 5 ms edit budget); numbers are always live. No switch |
| Two independent control toggles allow a meaningless state (in the initial analysis but not in the residual one) | One status with three values, no meaningless state (D28) |
| An "inactive" item vanishes from the numbers with one generic note | Every exclusion carries its reasons, in the inspector, the canvas and the report (APL-06) |
| Modal property editing patterns, a detached "Properties" panel per page | Already solved: modeless inspector, drafts, undo toasts (D25, UI-08 to UI-12) |
| Comments are a free text column | Review notes with author, time and open/resolved state, outside the op log so reviewers need no edit lease (REV-01 to REV-03) |

---

## 1. Terminology map (read this first)

The tool and we cut the threat side differently. The map keeps the reviews
from talking past each other. Our names are pack data (D24); the right column
is the default ISO pack.

| Reference tool | AegisTARA (default pack) | Model type |
|---|---|---|
| Threat (asset + security property + attack tree) | Threat scenario | `ThreatScenario` |
| Threat scenario (one threat × one damage scenario, with reasoning) | One row of the risk register: a threat scenario's link to one damage scenario | `ImpactRisk` per `(ThreatScenario, DamageScenario)` |
| Risk | Risk (per pair, then the worst per threat) | `ImpactRisk`, `ThreatRisk` |
| Attack tree (one per threat) | Attack tree | `TreeIdx` on the threat |
| Technical attack tree | **Shared fragment** (new, D29) | `TreeEntry` with `role = Fragment` |
| Attack leaf | **Shared step** (new, D29) | `SharedStep` |
| Circumvent tree | **Bypass tree** of a control (new, D29) | `SecurityControl::bypass: Option<TreeIdx>` |
| TOE configuration item | **Condition** (new, D27; packs may rename it "TOE configuration") | `Condition` |
| Assumption | Assumption (new, D27) | `Assumption` |
| Attack filter | **Attacker profile** (new, D27) | `AttackFilter` on the project |
| Security control, "active for TARA / RRA" | Security control with **status** Implemented, Planned, Off (D28) | `ControlStatus` |
| Initial AFR / Resid. AFR | Initial / residual feasibility | existing, label change only |
| Misuse case | Misuse case (new, optional) | `MisuseCase` |
| Responsible (Bosch, Customer, Supplier) | **Party** (new) | `Party` |
| TOE description, Scope | Item definition text: description and scope | `ItemText` on the project |
| Security goal / claim | Cybersecurity goal / claim | existing |

Visual ID prefixes for the new types (FND-06 rules: fixed per type, never
reused): Condition `CF`, Assumption `AS`, Shared step `L`, Shared fragment
`TT`, Misuse case `MU`, Party `P`. A tree's role (threat tree or fragment) is fixed
when it is created, so its prefix (`AT` or `TT`) never changes; bypass trees
are fragments (`TT`).

---

## 2. Cross-cutting decisions

These shape every screen below, so they are stated once.

### 2.1 Navigation follows the process diagram (UI-13)

The process diagram has five phases. Our sidebar lists pages in build order,
with Item definition last. New order, grouped and collapsible; a group header
shows the sum of its counts and the worst problem badge of its pages.

```text
┌──────────────────────────────┐
│ ▸ Overview                   │   process map (UI-18)
│ ▾ 1 Item definition        3 │
│     Description and scope    │   ItemText (job 132)
│     Architecture           12 │   components, interfaces, flows, boundaries (today's Item page)
│     Configuration           4 │   conditions (D27)
│     Assumptions             7 │   (D27)
│ ▾ 2 Impact                 5 │
│     Damage scenarios        5 │
│ ▾ 3 Assets and threats    13 │
│     Assets                  6 │
│     Misuse cases            7 │   optional, hidden when the pack turns it off
│     Threat scenarios        7 │
│ ▾ 4 Attack feasibility     9 │
│     Attack trees            7 │
│     Shared building blocks  6 │   steps · fragments (D29)
│     Attacker profile        ● │   dot = a filter is on (D27)
│ ▾ 5 Risk and treatment    20 │
│     Risks and treatment     8 │
│     Security controls       4 │
│     Goals and claims       15 │
│ ▾ Outputs                    │
│     Traceability             │   graph and matrix (TRC-04, TRC-05)
│     Summary                  │   management summary (RPT-09)
│     Reviews                 3 │   open review notes (REV)
│     Reports                  │
│     Glossary and documents   │   (FND-11, FND-12)
│ ▾ Project                    │
│     Problems              ⚠ 3 │
│     Versions · History       │
│     Administration           │
└──────────────────────────────┘
```

- `Page` gets a `group()`; the sidebar, the menu bar and the command palette
  (UI-01) all read the same table, so a new page is one row. Group open or
  closed is remembered per user (settings, not an op).
- A pack can hide a page it does not use (misuse cases, attacker profile) via
  an optional `pages` list; hidden pages keep their data and show again when
  the pack allows them.
- With systems (D26) the view chip in the top bar filters every page as
  today; the grouping does not change.

### 2.2 One table convention (UI-14)

Every element page uses `DataTable` (UI-09) with column kinds, so each page
is a column list, not a layout:

| Kind | Look | Behaviour |
|---|---|---|
| `Id` | visual ID, fixed width | sort key; copy on ⌘/Ctrl+C |
| `Name` | wraps to one line, tooltip | inline rename (UI-08.3) |
| `Links` | chips of visual IDs (`D02`, `D04`), "+N" when they do not fit | click a chip to open that element; tooltip shows names |
| `Computed` | shaded background, never editable, a small ƒ icon in the header | tooltip "Computed. Open the inspector for why." |
| `Rating` | level swatch and label | editable inline where the user rates it |
| `Text` | first line of description or rationale, dimmed | opens the inspector field |

Row states: selected, picked (UI-04), **excluded** (diagonal hatch, text at
70 % contrast, an icon with the reason as its accessible name; D27), and
**suggested** (ghost, job 089). Hatch is drawn by one `Excluded` style in
`theme.slint`, legible in light and dark.

A column chooser per table (which columns, order, width) is remembered per
user. It is a C requirement: wide pages (threat scenarios with 10 columns)
need it first.

### 2.3 Switches explain themselves

A page that holds switches (configuration, assumptions, attacker profile,
control status) shows one plain line above its table that says what
switching does, with the count of elements now excluded:

```text
ⓘ Switching a condition off excludes the assets, threats, steps, goals and claims
  that need it. Excluded items do not count in feasibility or risk.   4 excluded ›
```

`4 excluded ›` opens the table of excluded elements with their reasons.
After any switch the undo toast (UI-10) says what moved: "Condition CF03 off:
2 steps excluded, 1 risk lowered (T05 Risk 4 → 3). Undo".

### 2.4 The inspector says why something is excluded (APL-06)

```text
┌ Step · AT01.7 ─────────────────────────────┐
│ ▨ Excluded from the analysis               │
│   · Condition CF03 "ECU has Ethernet" off  │
│   · Assumption AS07 "Actuators are not     │
│     physically manipulated" holds          │
│   Show in Configuration ›                  │
├────────────────────────────────────────────┤
│ Manipulate Ethernet frames                 │
│ …                                          │
```

Reasons come from the engine (`Applicability::reasons`), names from the
client (R6). The same list is the accessible name of the hatched canvas node
and a line in the report's step table.

### 2.5 One relation registry (modularity)

The screenshots show the same link many times: a threat's damage scenarios
appear in the threat table, the threat-scenario table, the damage inspector
and the traceability graph. Today each page in `pages.rs` builds its own
link text. With six new element types that would multiply.

Decision: `aegis-client::relations` holds **one table of relation
descriptors** (from type, to type, label key, cardinality, a read function
over `Project`, and the `Edit` that links and unlinks). Inspector "Linked"
sections, link chips, the multi-link picker, the traceability graph and
matrix, find-in-page and report cross-references all read it. A new element
type adds rows, not code in five places. Details in architecture III.9.

### 2.6 Status bar

Bottom left keeps the server and engine versions. Added: "Saved 16:01"
(last accepted op), and problem counts by severity (`⛔ 0 ⚠ 3 ⓘ 1`), each a
button that opens Problems filtered to that severity (UI-17). The reference
tool's evaluation switch is not adopted (section 0).

---

## 3. Screen by screen

Each screen: what it is, what we have, the decision, and the job.

### 3.1 Process diagram (five phases, RQ numbers)

- **What it shows.** Item definition → impact assessment and asset/threat
  identification → attack feasibility → risk assessment → treatment, with the
  ISO clause 15 RQ numbers on each box.
- **We have.** Every box except assumptions and configuration; the Overview
  page shows counts and risk bars.
- **Decision.** Make the diagram our Overview's top section: a **process map**
  with one box per page group, each with its count and a completeness mark
  from the linter (✓ no findings, ⚠ N findings). Click opens the page. RQ
  numbers are pack data (`clause_refs` on pages, optional), so a CRA pack shows
  CRA Annex I references instead (D24). Not a drawing to edit, a navigation and
  progress view.
- **Job.** 131 (UI-18).

### 3.2 Assumptions

- **What it shows.** A list of assumptions with an on/off switch, linked TOE
  configuration items, a requirements link and a comment; a banner says that
  switching changes linked tree nodes, circumvent trees, leaves and claims.
- **We have.** Nothing. A claim has a name only.
- **Decision (D27).** `Assumption { name, description, holds: bool, conditions,
  excludes: steps and trees, claim }`.
  - While an assumption **holds**, the steps and trees it names are excluded
    (the assumption says that attack cannot happen). Switching it off is the
    what-if "and if the assumption fails?", which brings them back.
  - An assumption must be **ensured by someone**: the linter warns on an
    assumption with no claim (APL-04), and "Create claim" in its inspector
    makes the claim as a draft (UI-08) with the responsible party asked for.
    This is the diagram's "translate into" arrow.
  - Its own conditions (D27) switch it off with the condition.
  - Requirements link uses `ExternalRef` (TRC-01, job 076); until then the
    description holds it.
- **Job.** 136.

### 3.3 TOE configuration

- **What it shows.** Configuration items ("there is an OEM diagnostic port",
  "the ECU has an Ethernet interface", "hardware variant XY", "third-party
  software is used") with an on switch; the banner lists everything that can
  depend on one.
- **We have.** Nothing. Variants are separate projects or hand edits.
- **Decision (D27).** `Condition { name, description, present: bool }`, named
  "Configuration" in the default pack (packs may call it "TOE configuration"
  or "variant"). Assets, components, interfaces, threats, trees, steps,
  goals, claims and assumptions may **need** conditions. An element is
  excluded when any condition it needs is off (all-of semantics; any-of is
  not offered, it has not been asked for).
  - Not the same as D26 systems: a system is a view and never changes a
    number; a condition is an op and does change numbers. Not the same as D26
    settings: a setting caps feasibility; a condition says whether a part of
    the product exists in this variant.
  - A component or interface that needs a condition takes its assets,
    interfaces and the steps through it (via, CFG-01) with it: one switch
    removes "the Ethernet interface" from the whole analysis.
  - Switching is `Op::SetConditionPresent`: in history, in baselines, in the
    diff and on the report cover ("Configuration: CF02 on, CF03 off"). A
    report of a variant always says which variant it is.
- **Job.** 133 (model), 134 (engine), 135 (pages and inspector).

### 3.4 Attack filter

- **What it shows.** Threat scalability (all, scales, does not scale) and, per
  attack potential factor (time, expertise, knowledge, access, equipment),
  check boxes for the allowed values, plus attacker type. Unchecked values
  exclude leaves.
- **We have.** Attack potential factors per step (ADR 0021), pack-defined.
- **Decision (D27).** An **attacker profile** on the project:
  `AttackFilter { factors: per factor the allowed value keys, scalability:
  Any | Scales | DoesNotScale, attacker_types }`, default "everything allowed",
  set by `Op::SetAttackFilter` (whole value; the inverse is the old value).
  - A step whose rated factor value is not allowed is excluded with the reason
    "Outside attacker profile: Expertise = Multiple experts". An unrated factor
    never excludes (we do not guess).
  - A threat whose scalability does not match is excluded (needs the threat
    flag, job 138).
  - The page lists the pack's factors as columns of toggle chips, not a wall
    of check boxes; "Select all" per column; the header line says how many
    steps are excluded. It sits in group 4 next to the trees.
  - A non-default profile is printed in the report's method section and on
    the cover. A filtered TARA that does not say so would mislead a reviewer.
  - Attacker types (insider, remote hacker, organised crime) are an optional
    pack list on steps; a C requirement, job 139.
- **Job.** 137 (after 134 and 138), 139 later.

### 3.5 Security controls (active for TARA / active for RRA)

- **What it shows.** Controls with two switches, the goals they serve, a
  description and a requirements link. The banner says that switching
  changes the control's circumvent tree.
- **We have.** Controls with step mitigations, targets and rules (ADR 0064);
  "risk before controls" vs "after controls".
- **Decision (D28).** One `ControlStatus`:

  | Status | Initial (before treatment) | Residual (after treatment) | Meaning |
  |---|---|---|---|
  | Implemented | counts | counts | in the design already |
  | Planned | — | counts | the treatment we propose |
  | Off | — | — | what-if, kept for the record |

  Old projects load every control as **Planned**, which is exactly today's
  maths (initial = no controls, residual = all controls), so no saved number
  changes. The table gets a three-way segmented control in a Status column;
  the label "Risk before controls" becomes "Initial risk" with the tooltip
  "with implemented controls only".
- **Job.** 140.

### 3.6 Misuse cases

- **What it shows.** Attacker goals in plain words ("Flood the bus to launch a
  DoS attack"), linked from threats.
- **We have.** STRIDE category and catalog references on threats.
- **Decision.** A light `MisuseCase { name, description }` with N:M links to
  threat scenarios (on the threat side, like damages). Useful when a
  consultant runs an elicitation workshop with customer engineers who think
  in abuse stories, not in assets. Priority C; packs can hide the page.
- **Job.** 148.

### 3.7 Damage scenarios

- **What it shows.** One impact category and one impact level per damage
  scenario, with a reasoning column.
- **We have.** An impact level per category (safety, financial, operational,
  privacy) per damage scenario, which is what ISO/SAE 21434 15.5 asks.
- **Decision.** Keep our model (richer, and the standard's). Adopt the
  **rationale**: `rationale: Text` per damage scenario, shown as a column
  (UI-14 `Text`) and in the inspector next to the ratings; the linter notes a
  rated damage scenario without rationale (info, never blocks). Same field
  later on threats for the feasibility rationale (FND-09).
- **Job.** 147.

### 3.8 Assets

- **What it shows.** Name, security properties, TOE configuration,
  description.
- **We have.** All of it except configuration, plus the component binding.
- **Decision.** Only the Conditions column (D27) and link chips (UI-14).
- **Job.** 135.

### 3.9 Threats (their threat = our threat scenario)

- **What it shows.** Damage scenarios, configuration, misuse cases, initial
  and residual feasibility (computed, grey), asset, property, "Scales" and
  "Scaling effects reasoning".
- **We have.** Asset, category, damages, feasibility, treatment, goal, claim.
- **Decision.** Add `scales: Option<bool>` and a reasoning text on the
  threat scenario (FND-10); it feeds the attacker profile (3.4) and is a
  column. Initial and residual feasibility become `Computed` columns.
  Conditions and misuse cases are link columns.
- **Job.** 138; columns in 129.

### 3.10 Threat scenarios (their threat × damage pairs)

- **What it shows.** One row per threat × damage scenario with a free-text
  reasoning: why this threat leads to this damage.
- **We have.** The pairs exist as rows of the risk register (`ImpactRisk`);
  there is no place for the reason.
- **Decision.** No new page; the risk register already is this list. Add a
  **pair rationale**: the threat's damage links become
  `Vec<DamageLink { damage, rationale: Option<Text> }>` (sorted by damage, so
  equal content compares equal). Shown in the risk table as a `Text` column
  and in the risk inspector. Store: omitted when empty, v1 fixtures load.
- **Job.** 146.

### 3.11 Attack trees (table)

- **What it shows.** Per tree: the five factor values and AFR, initial and
  residual side by side, configuration, comment.
- **We have.** A narrow tree list (ID, tree, rating) beside the canvas.
- **Decision.** The list becomes a full-width table when the canvas is
  collapsed (or a "Table" mode toggle on the page): factor columns with the
  pack's factor short labels, initial and residual groups as `Computed`
  columns. The canvas page stays the default.
- **Job.** 152.

### 3.12 Attack tree canvas

- **What it shows.** Nodes with ID and name; leaves with factor icons and
  values, the score and level; the root with initial and residual blocks;
  "1 more…" when more than one path decides; gate label at the bottom; control
  nodes inline (dark, hatched when off); an inactive leaf hatched; a minimap;
  a zoom slider; the deciding path in yellow.
- **We have.** Layout, deciding path, zoom presets (PR 29), capped shield badge
  (ADR 0064), library, outline.
- **Decision.** Adopt three ideas:
  1. **Detail levels** tied to zoom: compact (name and level, today), factors
     (adds the five factor values in one row), full (adds initial and residual
     on gates). The level follows zoom by default and can be pinned. Pure
     `aegis-canvas` work into the same display list.
  2. **Excluded** nodes hatched (D27) with the reason as accessible text.
  3. **Minimap** in the bottom-left of the canvas for trees over one screen,
     drawn from the same display list at low detail, click and drag to pan.
  Not adopted: control nodes inside the tree. Our controls cap steps by
  mitigation and rule (ADR 0064); a bypass tree (D29) gives the ceiling a
  computed value instead of a node in every tree. A capped step shows the
  shield badge and, on hover, "capped by SC02 (bypass: Medium)".
- **Job.** 153; hatch in 135.

### 3.13 Technical attack trees

- **What it shows.** Named sub-trees ("read from internal flash memory") used
  in several threat trees, with "Used in" columns and configuration and
  assumption links.
- **We have.** The library inserts a **copy** (D14). Inside a project the
  same technique is rated again in each tree.
- **Decision (D29).** A **shared fragment**: an attack tree with
  `role = Fragment`, referenced from a node of another tree
  (`Op::UseFragment { tree, node, fragment }`); the node's value is the
  fragment root's value. Rate once, used everywhere, and a change shows in
  every tree that uses it.
  - Fragments can use shared steps and other fragments; a cycle is rejected
    with the cycle path (Kahn, ADR 0002, over the fragment graph).
  - On the canvas a reference node shows a link badge and expands in place as
    a read-only ghost; "Open fragment" jumps to it.
  - "Make shared" on a selected sub-tree creates a new fragment from it and
    puts a reference in its place, in one undo step; "Unshare" copies it
    back. An existing tree never changes role, so visual IDs stay stable.
  - The library (D14) still inserts copies. "Save to library" works on
    fragments as on any selection.
  - D26 is unchanged: fragments are reuse inside one project, not a way to
    link attacks across systems ("via" does that). A fragment used by trees of
    two systems is shared like a damage scenario.
- **Job.** 142.

### 3.14 Circumvent trees

- **What it shows.** Per security control, a tree of how to get around it;
  inactive (hatched) when the control is off; inserted into attack trees.
- **We have.** Controls cap steps with a fixed ceiling per mitigation or rule.
- **Decision (D29).** A control may have one **bypass tree** (a fragment).
  When the control counts (D28), its ceiling on a step is the bypass tree's
  feasibility instead of the typed level. "Capped by SC02: bypass Medium"
  says which. Mathematically this is the reference tool's "step AND
  circumvent control" (AND takes the minimum), without putting a control node
  into every tree. A bypass tree whose control is Off is shown hatched with
  "control off".
- **Job.** 143.

### 3.15 Attack leaves

- **What it shows.** Reusable leaves with their factor ratings, used in
  several trees; one can be excluded by the filter.
- **Decision (D29).** A **shared step**: `SharedStep { name, description,
  feasibility, factors, via, attack_type, attacker_type }`; a leaf node can
  reference one (`Op::UseSharedStep`), and its ratings come from it. The step
  inspector shows "Shared step L03, used in 5 trees" and edits the shared
  values (with that sentence above the fields, so nobody changes five trees by
  surprise). Unlinking copies the values into the node, so the inverse is
  exact.
- **Job.** 141.

### 3.16 External results

- **What it shows.** An empty table "can be imported via the project cockpit":
  results from other TARA projects (source ID, project name, factors, AFR,
  configuration).
- **Decision (D31).** This is the start of the capability the reference tool
  lacks: a project built from **several TARA files** that each stay workable
  on their own. Kept in the backlog behind a design session (section 4).
- **Job.** 158 (first slice), 159 (composite), both `[design]`.

### 3.17 Security claims

- **What it shows.** Claims with assumptions, responsible (Bosch, customer,
  supplier, or several), configuration, requirements link.
- **We have.** Claims with a name and description, linked from threats.
- **Decision.** Claims get `assumptions` (D27), `responsible: Vec<PartyIdx>`
  (job 145) and conditions. Responsible also on goals. Parties are a short
  project list (`Party { name, kind: Us | Customer | Supplier | Other }`),
  so a supplier's TARA says "Customer" and the customer's says "Supplier"
  without renaming text. The linter warns on a "share" treatment whose claim
  has no other party.
- **Job.** 145, 136.

### 3.18 Security goals

- **What it shows.** Goals with responsible, configuration, requirements link.
- **Decision.** Responsible and conditions as for claims; requirements link
  through `ExternalRef` (076).
- **Job.** 145, 135.

### 3.19 Traceability graph

- **What it shows.** Five columns (damage scenarios, threats, risks, goals and
  claims, controls) with "n/N" counts and a filter per column, curved links,
  the selection's details on the right.
- **We have.** Planned: a traceability matrix page (TRC-04, job 078).
- **Decision.** Add a **graph mode** next to the matrix (TRC-05): columns from
  the relation registry (2.5), drawn by `aegis-canvas` into the same display
  list (one layout per column, links as cubic curves, R-tree hit test).
  Selecting a box highlights its whole chain left and right and dims the
  rest; each column has a filter picker and a count; the view follows the
  system view (D26) and can show a baseline. Click opens the inspector.
  Also an export to SVG and a report figure.
- **Job.** 151 (after 149 and 078).

### 3.20 Glossary, related documents, management summary, review summary

- **Glossary.** Term, description, per-language text (FND-04). A report
  section. Not the translators' glossary of job 068; this one is the
  customer's project vocabulary. Job 156.
- **Related documents.** Title, document ID, version, link (an `ExternalRef`
  when 076 lands, a URL until then). A report section ("Referenced
  documents"), which every ISO 21434 work product needs. Job 156.
- **Management summary.** One page for a manager: initial vs residual risk
  bars, the highest residual risks, goals and claims per party, the
  configuration and attacker profile in effect, excluded elements by reason,
  open review notes. The same content is a report section (RPT-09). Job 155.
- **Review summary and the "All" review filter.** See section 5. Job 154.

### 3.21 Toolbar and find

- **What it shows.** Back and forward, undo and redo, save, cut, copy,
  duplicate, a review filter, add and delete, and Find (Ctrl+F) with a regex
  toggle.
- **We have.** Undo and redo in the top bar, row actions, the command palette.
- **Decision.** Add **find in page** (⌘/Ctrl+F): a filter field above the
  table that matches ID, name and description; a `.*` toggle for a regular
  expression (`regex` crate, MIT/Apache, already allowed by D5, with a size
  and time cap); "N of M" count; Escape clears; on the canvas it highlights
  matching nodes. **Back and forward** (⌘[ ⌘] / Alt+←→) over selections across
  pages, because link chips make jumping common. No toolbar of 20 icons: the
  commands live in menus, the palette and row actions (D25).
- **Job.** 128 (find), 130 (back and forward).

---

## 4. Composite projects from several TARA files (backlog, D31)

The owner asked for this to stay in the backlog for the current project. It
is written down here so the design session starts from a clear question.

**The need.** A supplier's ECU TARA, a platform team's gateway TARA and an
OEM's vehicle TARA are made by different people, at different times, often
in different companies. The vehicle TARA should be **assembled** from them,
while each one stays a normal project that its owner opens, edits, baselines
and reports on alone.

**Why D26 does not cover it.** D26 puts several systems inside one project,
with one lease and one history. That fits one team. It does not fit files
that live with other owners and change on their own schedule.

**Shape to design (not decided):**

1. **External results (first slice, job 158).** Import from another project
   file or server project **a pinned baseline** (its hash and ID): chosen
   threats' feasibility and factors, claims and goals. They arrive as
   read-only elements with the source shown. An external result can be used
   by a step like a shared step (D29), so a supplier's "flash read-out:
   Very low" feeds the vehicle tree. When the source has a newer baseline, the
   app says so and shows the diff before anything changes (no silent update).
2. **Composite project (job 159).** A project whose item definition lists
   member projects (each pinned to a baseline). The composite view is built
   in memory from the members plus the composite's own elements (boundary
   flows, vehicle-level damage scenarios). Edits go to the member that owns
   the element, under that member's lease; a member opened alone looks
   exactly as today.

**Questions for the session:** one methodology for all members or a mapping
between packs; who may pin and update a member; how IDs read across members
(`ECU:T03`); whether a composite is a server concept, a file concept, or
both; how a member's NDA (D21) limits what the composite may show; and
whether "assemble from several files" replaces any part of D26 (the default
answer is no).

---

## 5. Review notes (REV)

The reference tool has a comment column on every list and a review summary.
For a consultancy (D21) the customer's reviewer is the main user of that.

- A review note belongs to an element (by element ID), has an author, a time,
  text, a state (open, resolved) and replies.
- **Not an op.** Notes live outside the project op log: a server table
  (`aegis_review_note`, hash-chained like events) or, in standalone mode
  (D22), a side file in the project folder. So a Reviewer (D19) adds notes to
  the saved version without the edit lease, and notes never change a number
  or a baseline. A note records the op sequence it was written against; when
  its element changed since, it shows "changed since this note".
- Tables get a "Notes" filter chip (all, with open notes, resolved); the
  inspector shows the thread; the Reviews page lists open notes by element.
  A report can include open notes (off by default).

---

## 6. What we do not adopt

| Idea | Why not |
|---|---|
| Auto evaluation switch | Evaluation is incremental and always live |
| Two control switches | One status, no meaningless state (D28) |
| Controls drawn as nodes inside every attack tree | Ceilings and bypass trees give the same number with less to draw and keep in sync (D29) |
| A single impact category per damage scenario | ISO/SAE 21434 rates each category; we already do |
| A big icon toolbar | Menus, palette, row actions and shortcuts (D25) |
| A separate "threat scenarios" page for threat × damage pairs | The risk register already is that list; the pair rationale goes there |
| A fixed company name as a responsible party | Parties are project data |

---

## 7. Order of work

1. Foundations with no model change: 127 nav groups, 128 find, 129 table
   convention, 130 back and forward, 131 process map, 149 relation registry,
   150 multi-link picker.
2. Small model additions: 132 item text, 145 parties, 146 pair rationale,
   147 rationale, 138 threat scaling.
3. Applicability (D27): 133, 134, 135, then 136 assumptions, 137 attacker
   profile.
4. Control status (D28): 140. It can run in parallel with step 3; both touch
   the evaluation, so they merge one after the other.
5. Building blocks (D29): 141, 142, 143, 144.
6. Outputs: 151 graph, 152 tree table, 153 canvas detail, 154 review notes,
   155 summary, 156 glossary and documents, 148 misuse cases.
7. Later: 139 attacker types, 157 what-if preview; 158 and 159 after the
   design session.

Each job names its acceptance tests in the backlog line. Every job follows
the element recipe in architecture III.9.3, so new types arrive complete
(ops with inverses, store, diff, lint, pages, report, XLSX, Excel import,
undo label, `@tr` and Fluent keys in en and ja).

---

## 8. Check against ISO/SAE 21434 Figure 3 (2026-11-07)

Figure 3 of the standard ("Relationship between item, function, component and
related terms") checked against the code in `aegis-model`.

| Figure 3 link | Our model | Verdict | Closed by |
|---|---|---|---|
| item consists of component | project (or a D26 system) is the item; `Component.parent` | ✓ | |
| item implements function(s) | none | ✗ | ARC-06, job 160 |
| item contains asset | `Asset.component` | ✓ | |
| cybersecurity property attribute of asset | `Asset.properties` | ✓ | |
| threat scenario compromises property | `ThreatScenario.asset` and `category`; a category violating a property the asset lacks is rejected | ✓ | |
| threat scenario realizes damage scenario | `ThreatScenario.damages`, one risk per pair | ✓ | |
| damage scenario affects road user | impact levels per pack category (safety, financial, operational, privacy) | ✓ | |
| goal associated with threat scenario | `ThreatScenario.goal`, at most one | ◐ | FND-14, job 161 |
| goal protects asset; associated with item | derived through the threat and the project | ✓ | |
| goal realized by requirement | planned (architecture III.3, job 077) | ◐ | TRC-03, job 077 |
| requirement allocated to item, component | not even planned | ✗ | TRC-06, job 077 |
