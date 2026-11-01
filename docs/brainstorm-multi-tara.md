# Brainstorm: systems and multi-TARA (one project, many analyses)

- Status: **superseded** by [brainstorm-systems-config.md](brainstorm-systems-config.md)
  and decision D26 (proposed, 2026-11-02), which answer section 4.1 (systems
  inside one project) with a smaller model: a system is a list of components,
  not a `system` field on every element. Kept as the record of the first idea.
- Date: 2026-10-30
- Asked for: the app creates **one TARA per project**. A real item, for
  example a vehicle, is one project that holds several **system TARAs** —
  Braking, ADAS, Infotainment — each configured independently (interfaces,
  connections, protocols, physical controls), and the user wants to evaluate
  and report on **one system, a chosen combination (1..2), or all of them
  together**.

## 1. What the app does today (checked)

| Question | Answer |
|---|---|
| What is a project? | One flat `Project`: one methodology, one item definition (components, interfaces, data flows, trust boundaries), one set of assets, damage scenarios, threats, trees, controls, goals, claims. |
| Can I have two TARAs in one project? | No. The only grouping is the item definition, and there is exactly one of it per project. |
| How do I model a vehicle? | One project for the whole vehicle (mixed systems in one item definition), or one project per system (Braking, ADAS, ...) with no way to evaluate them together. |
| Lease and audit | One edit lease per project; one append-only op log and audit hash chain per project. |
| Reports | One report per project; sections pick from the one item definition and one risk set. |

So the ask is a new grouping level above the current project: a **system**.

## 2. The concept

- A **project** (a folder, a client deliverable) holds 1..N **systems**.
- Each **system** is a complete TARA scope: its own item definition (the
  components, interfaces, data flows, trust boundaries and their protocols and
  physical controls), and its own assets, damage scenarios, threats, attack
  trees, controls, goals and claims.
- Systems can **connect**: an interface or data flow can cross a system
  boundary (ADAS requests braking), so a combined evaluation can follow an
  attack path that starts in one system and ends in another.
- **Selection** for evaluation and reports: one system, a chosen subset
  (1..N), or all. A combination is a *view* over the selected systems, not a
  copy of them.

## 3. How it behaves for the user

1. On the start screen, New project creates a project; its first screen is the
   **systems list**, with "Add system" (Braking, ADAS, ...).
2. A system opens into the existing pages (item definition, assets, threats,
   trees, ...), scoped to that system.
3. The project header and a left-hand tree let the user switch systems and see
   which one is open.
4. Evaluation and reports offer a **scope**: "Braking", "Braking + ADAS", or
   "All systems". A combined scope composes the selected systems' trees and
   risks and shows cross-system attack paths.
5. Reports and exports produce per-system and combined risk views.

## 4. Design questions to decide

### 4.1 One project with systems, or a project per system?

This is the decision the design session must make; everything else follows.

| | System inside a project | Project per system under a "program" |
|---|---|---|
| Op and store shape | Ops carry a `system` scope; one project op log and audit chain | One op log per project as today; a new "program" links them |
| Lease and undo | One lease per project covers all systems; undo spans systems | One lease per system; a program has no single edit unit |
| Diff, baselines, history | One history and baseline per project; a baseline is the whole vehicle | Baselines are per system; combining them is a report-time view |
| Combined evaluation | The engine unions selected systems in memory; cross-system links are first-class | The engine must read across projects; cross-system links are external |
| Migration | The existing single-TARA project becomes "one system", a small migration | "Program" is a new server concept above projects |

**Recommendation: system inside a project.** One project keeps one op log, one
lease, one undo history, one audit chain, and one baseline — all of which the
tool already does well. A system is a scope on elements, and "combination" is
an evaluation/report view. This also makes the current behavior the special
case of "one system".

### 4.2 How is an element scoped?

- Every element carries an optional `system` reference; `Project` gains a
  `systems` arena with names and metadata.
- The item definition belongs to a system (a component is inside one system).
  An interface or data flow **between** systems references one end in each.
- Ops gain a `system` field (or the element's system is implied by its target
  ID, so the op stays small). The store is additive, so v1 fixtures load as a
  single unnamed system.
- **System deletion and cascade rules:** Removing a system must remove (or
  reassign) all elements scoped to that system plus any cross-system interfaces
  or data flows touching it in a single atomic, undoable op batch (`UI-10.6`),
  analogous to how `RemoveComponent` or `RemoveAsset` cascades links today.

### 4.3 Combined evaluation

- The engine already evaluates trees in topological order with an incremental
  cache. A combined evaluation builds a **virtual union** of the selected
  systems' trees and their cross-system links, recomputes only what changed,
  and explains every number with the existing explain-why trace (so "why is the
  combined risk high" still works).
- **Explicit cross-system tree/threat link (`SYS-04`, `SYS-05`):** Today in
  `aegis-model`, `Interface` and `DataFlow` do not link directly to `AttackTree`
  or `ThreatScenario`. For combined evaluation to follow an attack path across
  a system boundary, the design session must define the explicit model link —
  for example, allowing an attack step in System B's tree to reference a threat
  scenario or attack tree root in System A across a cross-system `DataFlow` or
  `Interface` (with cycle detection across the composed graph).
- Cross-system attack paths come from the same top-k path enumeration (job
  021), now walking links that cross a system boundary.

### 4.4 Reports and exports

- The report builder's scope selector gains the system selection (one, a
  subset, all), and each section prints either per-system or combined rows.
- The traceability matrix (job 078) and the architecture diagram (job 075)
  follow the same scope, so a diagram can show two systems and the links
  between them.

### 4.5 Roles and access

- Access (ACC-01) stays per **project** for the first cut. Per-system access
  is a later refinement; the combined view requires read on every selected
  system, so it is enforced at the server the same way any project read is.

## 5. Risks

- **Scope creep.** "Combination" is a view, not a copy; resist storing combined
  results. Recompute deterministically from the source systems.
- **Op and store churn.** Additive `system` fields keep v1 fixtures loading;
  the migration is "existing project becomes one system".
- **Lease granularity.** One lease per project means two analysts cannot edit
  two systems of one vehicle at once. If that becomes a customer need, revisit
  with a per-system lease, which the single-op-log design can still support.
- **Report ambiguity.** A combined risk must always name which systems it
  covers; otherwise the number looks like a single system's.

## 6. Phased plan (after the 4.1 decision)

| Job | What | Builds on |
|---|---|---|
| 119 | Systems arena + `system` scope on elements and ops; per-system item definition and configuration; systems list and switch in the shell; v1 fixtures load as one system | the existing model, store, lease |
| 120 | Cross-system interfaces and data flows; combined and selected evaluation with explain-why and top-k paths across boundaries | jobs 021, 025, 074 |
| 121 | Per-system and combined reports, exports and traceability, following the report builder's scope | jobs 070, 071, 072, 078 |

## 7. Questions for the session

1. Confirm **system inside a project** (recommended) over project-per-system.
2. One lease per project now, or a per-system lease from the start?
3. Should "All systems" be a first-class default scope, or always an explicit selection?
4. Do visual IDs (job 118) stay per-project or become per-system when systems land (recommended: per-system, prefixed by the system name in combined views)?
5. **Shared vs. per-system elements:** Are vehicle-level damage scenarios (for example "Loss of braking deceleration"), security controls, and cybersecurity goals strictly per-system, or can a project also hold shared project-scope damage scenarios, controls, and goals referenced by threats across multiple systems?
6. **Methodology compatibility:** Must all systems in a project share the project's single `Methodology` (recommended, so impact, feasibility, and risk scales are directly commensurable in combined evaluation), or can systems pick different packs?
7. **Cross-system attack tree link (`SYS-04`, `SYS-05`):** How does an attack path cross a system boundary in the model — by linking an attack step in one system to a threat scenario/tree root in another system over a cross-system `DataFlow`/`Interface`?
8. Is the current single-TARA project enough for the first customer, with systems as the next wave, or is multi-system needed before any customer demo?
