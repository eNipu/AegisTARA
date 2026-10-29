# Spec — 072-xlsx-export

## Title
XLSX export (RPT-05)

## Design Reference
- Reference: docs/adr/0056-report-engine-unification-and-export-suite.md
- Affected crates/paths: crates/aegis-client/src/report/xlsx.rs, crates/aegis-client/src/report/mod.rs (ExportKind::ReportXlsx), crates/aegis-shell/src/controller.rs

## Problem & Goal
XLSX export with a permissive, dependency-free writer: a risk-register sheet and a full workbook with one sheet per element type, guarded against formula injection.

## Acceptance Criteria
1. a risk-register sheet; 2. a full workbook (Overview, risk register, one sheet per element type, Problems); 3. formula injection guarded (leading =,+,-,@ get a quote); 4. XML escaped and control characters dropped; 5. no new dependency (hand-written stored ZIP + CRC32).
