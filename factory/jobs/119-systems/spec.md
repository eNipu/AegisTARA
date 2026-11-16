# Spec: 119-systems

## Title

Systems, one per component: model, store, derived scope, lint, and the client data for the Systems table, the picker and the view filter.

## Design Reference

- Requirements: `docs/requirements.md` section 2.5a, SYS-01, SYS-02, SYS-03 (lines 155 to 169).
- Decision: `docs/decisions.md` D26 (line 203). D33 (line 261) later makes a system a folder; that is not part of this job.
- Design: `docs/brainstorm-systems-config.md` sections 5.2 and 5.3 (lines 156 to 223) and the test list (lines 472 to 486); `docs/design/systems-config.md` sections 1, 2, 3, 11, 12.
- Storage: `docs/adr/0068-generic-element-store.md`.
- Scope limit: `factory/jobs/119-systems/rework.md`, section `## Answers`.
- Code: `crates/aegis-model/src/element.rs`, `item.rs`, `op.rs`, `project.rs`, `error.rs`, `lint.rs`, `store/snapshot.rs`, `store/op_log.rs`, `store/wire.rs`; `crates/aegis-client/src/commands.rs`, `undo_label.rs`, `pages.rs`, `view.rs`, `state.rs`, `session.rs`, `settings.rs`.

## Problem & Goal

A project is one flat TARA today. A vehicle has several systems (Brake, ADAS, Gateway, Backend) that a consultant must analyse and report one by one and together (gap G48). After this job a project can hold named systems, each top-level component is in at most one, and every other element follows its links, so a user can look at one system, a few, or all.

Business value (P7):

| Part | Value |
|---|---|
| Systems and a component's system | One project file covers a whole vehicle; no copies that drift apart. |
| Derived system of other elements | No second field to keep in step; moving the gateway is one edit. |
| Lint "component in no system" | Nothing falls outside every TARA unnoticed. |
| View filter, never an op | Looking at Brake leaves no trace in the evidence and changes no number. |
| Remove system with undo | No confirmation dialog; nothing but the grouping is lost. |

## Storage decision (rework answer 2)

A system is a **kind in the generic element store** (ADR 0068), not a typed arena. The store already gives add, rename and remove with exact inverses, visual IDs, the snapshot, the op log, the workspace file, the diff and the cap, so a system costs one table row. The one thing the store cannot do is let a component point at a system (generic elements link only to threat scenarios), so the component gets a typed field `Component.system: Option<ElementId>` and one new op, `SetComponentSystem`. A system changes no computed number, which is the rule ADR 0068 uses to decide what goes in the store.

Consequences:

- The backlog's `AddSystem`, `RenameSystem` and `RemoveSystem` are `AddElement`, `RenameElement` and `RemoveElement` with kind `system`. Only `SetComponentSystem` is new.
- A system has a name and no description in this job. Descriptions for generic elements come with job 148 (ADR 0068, "Left for later") and then apply to systems too.
- The cap is 64 systems. The derived scope keeps the systems of an element in one 64-bit word; a benchmark in slice 119c records the cost of the derived pass at the cap.

## Guards and inverses (lesson L04)

- `SetComponentSystem { id, system, provenance }`: refused for an unknown component, for `Some` on a component that has a parent (`ChildComponentSystem`), for an unknown element (`UnknownElement`), for an element of another kind (`NotASystem`), and for a blank actor. The inverse is the same op with the old system and the old provenance.
- `SetComponentParent` with `Some(parent)` is refused while the component has a system (`ComponentHasSystem`). The client batch clears the system first.
- `RemoveComponent` is refused while the component has a system (`ComponentHasSystem`), because its inverse `AddComponent` carries no system. The client batch clears it first.
- `RemoveElement` on a system is refused while any component is in it (`SystemHasComponents`). The client batch clears those components first, so the removed system is bare and `AddElement` restores it exactly.

## Acceptance Criteria

1. `ElementKind::System` exists with key `system`, prefix `SYS`, file `systems/main/systems.json`, no link targets and a cap of 64; the 65th `AddElement` is refused. Check: `cargo test --locked -p aegis-model --test elements --test elements_store --test workspace`.
2. `SetComponentSystem` applies, and each guard above has a test that only that guard refuses and that checks the error. Check: `cargo test --locked -p aegis-model --test systems`.
3. Every new or changed op has an exact inverse: the model op generator covers `SetComponentSystem` and system elements, and `undo_every_op` passes. Check: `cargo test --locked -p aegis-model --test ops_properties` and `cargo test --locked -p aegis-client --test undo_every_op`.
4. The snapshot omits `system` on a component with none and omits the `system` list when there are no systems; `tests/fixtures/project-v1.json` and `oplog-v1.jsonl` load with no systems and save the same bytes as before; a snapshot, an op log and workspace files with systems round-trip. A file that puts a system on a child component, or names a missing system, is refused. Check: `cargo test --locked -p aegis-model --test systems --test store --test workspace`.
5. `Project::diff` marks a component whose system changed as modified and lists added, removed and renamed systems. Check: `cargo test --locked -p aegis-model --test systems`.
6. `Project::system_use()` gives each component, interface, asset and threat at most one system, and each data flow, trust boundary, tree, damage scenario, control, goal and claim the set of systems that use it. Property tests: every component, interface, asset and threat is in at most one system; `shows(A ∪ B) = shows(A) or shows(B)` for every element; an element with nothing to derive from shows in every view; with no systems every set is empty. Check: `cargo test --locked -p aegis-model --test systems_properties`.
7. Removing a system (clear its components, then remove it) changes nothing but those components' system and last-changed record, as a property test. Check: `cargo test --locked -p aegis-model --test systems_properties`.
8. The benchmark `systems` runs and its result is written in the build note of slice 119c. Check: `cargo test --locked -p aegis-model --bench systems`.
9. The linter reports each top-level component with no system once the project has at least one system, and nothing in a project with no systems. Check: `cargo test --locked -p aegis-model --test systems` and `cargo test --locked -p aegis-client --test systems`.
10. Client edits: add, rename and remove a system (one undo step; the label says how many components are now in no system); move one or several components to a system or to none (one undo step); giving a component a parent or removing it clears its system in the same batch. Undo restores the project exactly. Check: `cargo test --locked -p aegis-client --test systems`.
11. Client rows: the Systems table rows, a draft row that creates a system, the system name on component rows, the picker data in the component detail (read-only text for a child), and comparison lines for systems and moved components. A project with no systems gives no system rows, no picker choices and no view menu. Check: `cargo test --locked -p aegis-client --test systems`.
12. The view (all, one, a subset) has a menu with component counts that add up, a chip text ("All systems", "Brake", "Brake + ADAS", "Brake +2"), command palette entries, and is stored in the settings file per user and project. Check: `cargo test --locked -p aegis-client --test systems_view`.
13. The view filters the table rows of every page, and switching it adds no op: the confirmed sequence number, the pending batches, the undo history and the project are unchanged, in read-only mode too. This replaces the headless keyboard test of the backlog line. Check: `cargo test --locked -p aegis-client --test systems_view --test session`.
14. The whole workspace passes the gate, including `aegis-shell`, which this job does not change. Check: `scripts/ci.sh`.

## Out of scope

- **The screen, left for the GPUI shell** (owner rule, rework answer 1): the Systems table and its draft row on the item definition page, the system picker in the component inspector, the "Move to system" button in the bulk bar, the view chip and its dropdown in the top bar, the command palette wiring, the undo toast for a single move, the 900 px layout, the keyboard path, the English and Japanese strings of `docs/design/systems-config.md` section 13, and loading and saving the remembered view when a project opens and closes. No `.slint` file and no Slint glue in `aegis-shell` changes.
- A description on a system (job 148).
- Boundary flows, per-system totals on the overview, and the combined view (job 120, SYS-04, SYS-05). The overview page is not filtered here.
- Reports, exports and Excel import per system (job 121, SYS-06).
- Filtering the comparison lines by the view.
- A duplicated component starts in no system; the lint flags it.
- After an undo, adding the changed element's system to the view (brainstorm 5.3).
- A folder per system (D33). All systems stay in `systems/main/`, and the list of systems is `systems/main/systems.json`.
