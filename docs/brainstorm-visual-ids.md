# Brainstorm: human-readable visual IDs on elements

- Status: **proposed**; turns into backlog job 118 (`FND-06`, `FND-07`). The
  mapping mechanism is a small design choice the job can make; the questions in
  section 4 are for that job's review, not a blocking design session.
- Date: 2026-10-30
- Asked for: every element already has an internal ID, but it is a UUID like
  `0199c82c-c001-7000-8000-000000000001` and tells the user nothing. Give each
  element a short, friendly ID the user can say and write, for example `A00`
  for an asset, `D00` for a damage scenario, `T00` for a threat scenario. The
  visual ID must map cleanly to the internal ID for persistence and the corner
  cases.

## 1. What the app does today (checked)

| Question | Answer |
|---|---|
| What identifies an element? | A UUIDv7 stored as a typed `AssetId`, `DamageId`, `ThreatId`, `TreeId`, `ControlId`, `GoalId`, `ClaimId`, `ComponentId`, `InterfaceId`, `DataFlowId`, `TrustBoundaryId`. |
| Is there a friendly name? | Yes, a human **name**, but names are not unique, change, and are not safe to cite. |
| Where do elements show their ID? | Nowhere in the UI. Tables show the name; the op log and snapshots carry the UUID. |
| Can a user point at "threat T03"? | No. Cross-references in reports and the inspector use names only. |

So the idea is new. It is a display-and-citation feature on top of the existing
ID scheme, not a change to the engine's keys.

## 2. The proposed scheme

A visual ID is **a type prefix plus a zero-padded sequence number**, for
example:

| Element type | Prefix | Examples |
|---|---|---|
| Asset | `A` | `A00`, `A01` |
| Damage scenario | `D` | `D00`, `D01` |
| Threat scenario | `T` | `T00`, `T01` |
| Attack tree | `AT` | `AT00` |
| Security control | `SC` | `SC00` |
| Cybersecurity goal | `G` | `G00` |
| Cybersecurity claim | `CL` | `CL00` |
| Component | `C` | `C00` |
| Interface | `IF` | `IF00` |
| Data flow | `DF` | `DF00` |
| Trust boundary | `TB` | `TB00` |
| Condition (planned, D27, job 133) | `CF` | `CF00` |
| Assumption (planned, D27, job 136) | `AS` | `AS00` |
| Shared step (planned, D29, job 141) | `L` | `L00` |
| Shared fragment (planned, D29, job 142; also an attack tree) | `TT` | `TT00` |
| Misuse case (planned, job 148) | `MU` | `MU00` |
| Party (planned, job 145) | `P` | `P00` |
| Function (planned, job 160) | `F` | `F00` |

Attack tree nodes (`NodeId` — gates and steps inside a tree) remain unnumbered:
a tree can hold up to 4,096 nodes (`FND-03`), and users cite the tree (`AT00`)
and the step's name within it rather than a global node number.

The prefix is fixed per type (it never changes even when a pack renames the
type, so a cited ID stays meaningful). The number is padded to a width that
grows with the count (2 digits up to 99, then 3, and so on), so IDs sort in
creation order and stay short.

## 3. Design questions

### 3.1 Stored or derived?

| | Store the number on the element | Derive from the UUID or position |
|---|---|---|
| Stability under edits | The number is fixed at creation; edits never move it | Position changes when the arena reorders; a UUID hash is stable but ugly (`A-3f9a`) |
| Undo/redo | Restores the element with its number; a create-undo must not reuse it | Recomputing from position breaks "undo restores exactly" (UI-10.6) |
| Ops and store | Add ops carry the number (or the model assigns it); snapshot round-trips it | No new field, but fragile |
| Friendly | `A00`, `A01` | `A-3f9a2c` is not what the user asked for |

**Recommendation: store it.** The number is a monotonic per-type counter in the
snapshot; each `Op::Add*` payload carries an optional explicit visual number
`Option<u32>` (`None` on fresh creation lets the model assign the next counter
value; `Some(n)` on op-log replay and on the inverse `Op::Add*` produced by
`Op::Remove*` restores the exact visual number `n` and bumps the high-water
counter to at least `n + 1`). Per-kind high-water counters are saved in the
snapshot so numbers are never reused after reload, while `Project::eq` compares
live elements (and their visual numbers) and ignores the high-water counters on
empty/tombstoned slots — matching how `Arena` tombstones work today so `apply`
followed by `inverse` always restores `Project` equality (`UI-10.6`).

### 3.2 Stability and reuse

- **Never reuse after deletion.** `A00` deleted stays gone; the next asset is
  `A01`. Reports and audit references stay unambiguous. This is the same reason
  the op log is append-only.
- **Per-project or per-system uniqueness?** Today it is per-project. When
  multi-TARA systems land (SYS-01), IDs are unique **per system**, so `A00`
  in Braking and `A00` in ADAS are different elements; a combined view prefixes
  them `Braking · A00`. Until systems exist, per-project uniqueness is the rule.

### 3.3 Persistence and backward compatibility

- The visual number is stored in the snapshot next to the element (like the
  name) and travels in the op log through the existing `Add*`/`Rename*` op
  shapes, so it round-trips and stays deterministic (P4).
- **Old v1 fixtures have no numbers.** On load, assign them in creation order
  per type, deterministically. The assignment is stable for a given fixture, so
  a re-imported old project gets the same IDs.

### 3.4 Where the visual ID shows

1. A new first **ID** column in every element table (assets, damage, threats,
   controls, goals, claims, item definition).
2. The **inspector title**, next to the name.
3. **Cross-references** in reports and lists ("T03 realizes A01") and in the
   traceability matrix (job 078).
4. **Command palette search**, so typing `T03` navigates to that threat.

### 3.5 Diff, imports and exports

- **`Project::diff` (`A7`):** Because an element's visual number is immutable
  for its lifetime, visual IDs never diff as modified; diff lines and reports
  can include the visual ID next to the element name for clarity.
- **CSV / XLSX / ReqIF exports:** Include the visual ID column (`A00`, `T03`,
  ...) so external spreadsheets and ALM round-trips can cite it.
- **Excel import (job 080), ReqIF import (job 079) and library inserts:** Create
  elements through the same `Add*` ops, so newly imported elements get the next
  free number (an optional visual ID column in an imported sheet is ignored or
  used only for row matching, never to overwrite the monotonic counter).
- If an import maps two source rows onto what the user expects to be the same
  element, that is an existing merge question, not an ID question; the visual
  ID simply follows whichever element the ops produce.

## 4. Risks

- **ID churn if derived from position.** Mitigated by storing the number.
- **Collision on import or undo.** Mitigated by a monotonic counter that is
  never decremented on undo.
- **Type renames.** The prefix is fixed per element type, not per pack label,
  so a methodology that renames "damage scenario" to "harm" keeps `D00`.
- **Too-long IDs.** Padded to the width the count needs; a project would need
  100+ of one type before a third digit appears.

## 5. Phased plan

| Step | What |
|---|---|
| 1 | Model + store: add the number to each element, assign on create and on old-fixture load, round-trip in snapshot and op log; extend the undo test to prove the number is restored. |
| 2 | Client + shell: the ID column, inspector title, cross-references, and command palette search. |
| 3 | Reports: print the ID next to names and in the traceability matrix. |

All three steps are one backlog job (118), built end to end like the other
model-plus-UI jobs.

## 6. Questions for the job review

1. Two digits with roll-over, or width that grows with the count (recommended)?
2. Per-project uniqueness now, per-system when systems land (recommended)?
3. Does the visual ID replace the name in reports, or sit next to it (recommended: next to it)?
