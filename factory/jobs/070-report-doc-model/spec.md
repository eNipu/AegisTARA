# Spec — 070-report-doc-model

## Title
One report document model (RPT-01)

## Design Reference
- Reference: docs/adr/0056-report-engine-unification-and-export-suite.md
- Affected crates/paths: crates/aegis-client/src/report/{mod,doc,html,pdf,csv}.rs

## Problem & Goal
Move the HTML report onto ReportDoc so HTML and PDF share one document model and cannot drift; add a table of contents and anchors to the HTML.

## Acceptance Criteria
1. HTML and PDF render the same ReportDoc blocks; 2. the HTML has a TOC and anchor ids on every heading; 3. the same-sections test still passes and now checks the HTML headings come from the doc.
