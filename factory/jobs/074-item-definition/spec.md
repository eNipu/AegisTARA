# Spec — 074-item-definition

## Title
Item definition model (ARC-02)

## Design Reference
- Reference: docs/adr/0059-item-definition-model.md
- Affected crates/paths: crates/aegis-model/src/{id,item,asset,description,op,project,lint,diff,methodology/,store/}.rs, crates/aegis-model/packs/iso21434-r155.json, crates/aegis-client/src/{commands,pages,view,state,report/}.rs, crates/aegis-shell/{ui/app.slint,src/controller.rs}

## Problem & Goal
Add components, interfaces, data flows, and trust boundaries with pack-defined interface kinds; bind assets to components; add typed Ops, snapshot/op-log persistence, diff, and lint rules (unbound asset, boundary-crossing flow with no threat); and provide an Item Definition page and report section (plus component filter).

## Acceptance Criteria
1. `Component`, `Interface`, `DataFlow`, `TrustBoundary`, pack `InterfaceKind`, and asset component binding in `aegis-model`; 2. 18 validated, single-op-invertible `Op`s, snapshot/op-log round-trip, `ProjectDiff`, and `AssetUnbound` + `BoundaryCrossingDataFlowWithoutThreat` lint rules; 3. Item Definition page in `aegis-client` and `aegis-shell`; 4. Item Definition report section, XLSX sheets, and `ReportFilters.component`.
