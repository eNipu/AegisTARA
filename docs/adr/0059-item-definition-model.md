# ADR 0059: Item definition model (components, interfaces, data flows, trust boundaries)

- Status: accepted (job 074-item-definition)
- Date: 2026-10-27
- Scope: `crates/aegis-model/src/{id,item,asset,description,op,project,lint,diff}.rs`,
  `crates/aegis-model/src/methodology/{mod,pack}.rs`,
  `crates/aegis-model/src/store/{wire,snapshot,op_log}.rs`,
  `crates/aegis-model/packs/iso21434-r155.json`,
  `crates/aegis-client/src/{commands,pages,view,state}.rs`,
  `crates/aegis-client/src/report/{profile,doc,xlsx}.rs`,
  `crates/aegis-shell/{ui/app.slint,src/controller.rs}`
- Rules: D1 (pack vocabulary), D2/D3 (UUIDv7 + arena indices), D6 (provenance),
  D7 (deterministic ordering), D10 (derived state), D15, ARC-02
- Builds on: `0009-domain-model-v0-2.md`, `0051-element-descriptions.md`,
  `0053-pack-names-and-likelihood.md`

## Context

ISO/SAE 21434 Clause 9 ("Item definition") defines the system boundary,
components, interfaces, data flows, and trust boundaries before cybersecurity
assets and threat scenarios are identified. Analysts needed first-class item
definition entities in `aegis-model`, completeness lint rules tying the item
architecture to assets and threats, an Item Definition page in the desktop app,
and a report section and component filter (`ARC-02`, decision D15).

## Decisions

### 1. Four new arena-backed entities and asset component binding

- **IDs and indices (`id.rs`):** `ComponentIdx`/`ComponentId`,
  `InterfaceIdx`/`InterfaceId`, `DataFlowIdx`/`DataFlowId`, and
  `TrustBoundaryIdx`/`TrustBoundaryId` (4-byte `u32` arena indices + 16-byte
  UUIDv7 external IDs).
- **`Component` (`item.rs`):** `name`, `parent: Option<ComponentIdx>` (bounded
  to `MAX_COMPONENT_DEPTH = 16` with cycle rejection), and `Provenance`.
- **`Interface` (`item.rs`):** `name`, `component: ComponentIdx`,
  `kind: InterfaceKind(u8)` (compiled from the methodology pack's
  `interface_kinds` list; e.g., CAN, Automotive Ethernet, Cellular, Wi-Fi/BLE,
  Debug/Hardware, Internal IPC in `iso21434-r155.json`), and `Provenance`.
- **`DataFlow` (`item.rs`):** `name`, `from: ComponentIdx`, `to: ComponentIdx`
  (`from != to`), `interface: Option<InterfaceIdx>` (when set, must belong to
  `from` or `to`), and `Provenance`.
- **`TrustBoundary` (`item.rs`):** `name`, sorted deduplicated
  `members: Vec<ComponentIdx>` (capped at `MAX_TRUST_BOUNDARY_MEMBERS = 64`),
  and `Provenance`.
- **`Asset` binding (`asset.rs`):** `component: Option<ComponentIdx>` binds an
  asset to the component that hosts or transmits it.
- **`ElementRef` (`description.rs`):** Extended with `Component`, `Interface`,
  `DataFlow`, and `TrustBoundary` so every item-definition element supports
  long-form descriptions (`Op::SetDescription`) and figure attachments.

### 2. Typed operations, reference counting, diff, and store compatibility

- **18 new `Op` variants (`op.rs`):** `AddComponent`, `RenameComponent`,
  `SetComponentParent`, `RemoveComponent`, `AddInterface`, `RenameInterface`,
  `SetInterfaceKind`, `RemoveInterface`, `AddDataFlow`, `RenameDataFlow`,
  `SetDataFlowInterface`, `RemoveDataFlow`, `AddTrustBoundary`,
  `RenameTrustBoundary`, `AddTrustBoundaryMember`, `RemoveTrustBoundaryMember`,
  `RemoveTrustBoundary`, and `SetAssetComponent`. Every variant validates
  preconditions before mutation and returns its exact single-op inverse for
  $O(1)$ undo/redo.
- **Reference counts (`RefCounts`):** Tracks child components, interfaces, data
  flows, trust-boundary memberships, and bound assets per component (`ComponentInUse`),
   plus data flows per interface (`InterfaceInUse`). In `aegis-client`,
  `Edit::RemoveComponent` and `Edit::RemoveInterface` expand to safe cascades
  that clear dependent links before removing the target in one undo batch.
- **Persistence (`store/`):** `SnapshotDoc` and `OpWire` serialize the four new
  entity collections with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  and `asset.component` with `#[serde(default, skip_serializing_if = "Option::is_none")]`,
  keeping v1 snapshots and op logs byte-for-byte compatible when no item
  definition is present.

### 3. Architecture-to-threat lint rules (`lint.rs`)

When a project defines at least one component:

- **`LintCode::AssetUnbound` (`Warning`):** Fires on any `Asset` with
  `component == None`, reminding the analyst to bind every asset to a component
  in the item definition.
- **`LintCode::BoundaryCrossingDataFlowWithoutThreat` (`Warning`):** When at
  least one `TrustBoundary` exists, a `DataFlow` crosses a trust boundary if a
  boundary contains one endpoint (`from` or `to`) but not the other. If neither
  endpoint component has a bound asset with at least one `ThreatScenario`, the
  linter flags the data flow.

### 4. Item Definition page and report integration

- **`Page::Item` (`pages.rs`, `view.rs`, `ui/app.slint`):** Dedicated TARA step
  showing Components, Interfaces, Data Flows (with derived boundary-crossing
  indicator), and Trust Boundaries (with member checkboxes), plus a Component
  selector on the Assets page.
- **Reports (`report/doc.rs`, `report/profile.rs`, `report/xlsx.rs`):** Adds an
  Item Definition section (Section 2 when enabled) with tables for components,
  interfaces, data flows, and trust boundaries, exports four worksheets in the
  XLSX workbook when item elements exist, and wires `ReportFilters.component` to
  filter assets, threats, and risk register rows by component name.

## Consequences

- Concept-phase item definitions and cybersecurity assets/threats now live in a
  single auditable op log with completeness lint checks and baseline diffs.
- Old snapshots, op logs, and methodology packs without `interface_kinds` load
  unchanged.
