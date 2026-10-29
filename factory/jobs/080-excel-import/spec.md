# Spec — 080-excel-import

## Title
Excel TARA import (INT-02)

## Design Reference
- Reference: docs/adr/0060-excel-tara-import.md
- Affected crates/paths: crates/aegis-client/src/excel.rs, crates/aegis-client/src/{commands,session,state,view}.rs, crates/aegis-shell/{ui/app.slint,src/controller.rs}

## Problem & Goal
Read an `.xlsx` workbook, map columns to TARA elements in a step with a dry-run preview and a list of unmapped rows, and apply as one batch with `ProvenanceSource::Import`.

## Acceptance Criteria
1. Bounded dependency-free `.xlsx` reader (`read_workbook`); 2. auto-detected and customizable column mapping (`ExcelMapping`); 3. dry-run preview (`preview_import`) reporting counts and unmapped rows with reasons; 4. single-batch apply with `Provenance::import` (`ProvenanceSource::Import`).
