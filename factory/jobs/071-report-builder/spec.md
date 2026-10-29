# Spec — 071-report-builder

## Title
Report builder (RPT-02 to RPT-04)

## Design Reference
- Reference: docs/adr/0056-report-engine-unification-and-export-suite.md
- Affected crates/paths: crates/aegis-client/src/report/profile.rs, crates/aegis-client/src/settings.rs, crates/aegis-client/src/session.rs, crates/aegis-client/src/commands.rs, crates/aegis-proto/proto/aegis/v1/aegis.proto, crates/aegis-server/src/service.rs

## Problem & Goal
Select sections and filters (risk level, asset, component, baseline), language, logo, document ID, revision history, approvers and a classification mark; save the profile; record it in the export audit event.

## Acceptance Criteria
1. sections on/off; 2. filters narrow threats and the risk register by risk level and asset; 3. branding (document ID, classification, approvers, revisions, logo) on the cover and every page; 4. profiles persist in Settings; 5. RecordExportRequest.detail records the profile.
