# ADR 0056: One report document model, report profiles, and XLSX export

- Status: accepted (jobs 070-report-doc-model, 071-report-builder, 072-xlsx-export)
- Date: 2026-10-26
- Scope: `crates/aegis-client/src/report/` (the old `report.rs` and `pdf.rs`
  moved here), `crates/aegis-client/src/settings.rs`, `ExportKind`,
  `RecordExportRequest.detail`, `crates/aegis-server/src/service.rs`
- Rules: D5 (licenses), D7/P4 (determinism), R3 (caps), R6 (limits),
  Q3 (typed errors)
- Builds on: `0037-baselines-history-reports.md`, `0044-pdf-report.md`,
  `0047-pdf-tree-figures.md`
- Requirements: RPT-01 to RPT-05

## Context

The HTML report (`report.rs`) and the PDF (`pdf.rs`) each built their own
sections from the same page builders, and a test kept their section lists
equal, but the two models could still drift (ADR 0044 section 2 called the
HTML rewrite a follow-up). Customers also need submission-specific reports
(sections, filters, branding) and Excel, which was promised and only CSV
existed.

## Decisions

### 1. One `ReportDoc` feeds every writer

`report/doc.rs` holds `ReportDoc`, `Block`, `Style`, `Figure`, `Panel` and
the single builder `report_doc_with_options`. `report/html.rs`, `report/pdf.rs`
and `report/xlsx.rs` render the same blocks. The old `report_html` was
rewritten to render the doc instead of building its own sections, and it now
emits a table of contents (`<nav class="toc">`) and an anchor id (`sec-N`)
before every heading, assigned in block order so they are deterministic. The
"same sections" test stays, now also asserting the HTML headings come from the
doc and that the TOC exists.

### 2. Report profiles are data, not a UI

`report/profile.rs` holds `ReportProfile { name, language, sections, filters,
branding }`. Sections are a bool per part (cover, notes, executive summary,
assets, damage, threats, attack trees, risks, controls, goals and claims,
problems) with the section numbers kept fixed so a disabled section leaves a
gap instead of renumbering. Filters narrow the threats and the risk register
by residual risk level and by asset name; `component` is stored but a no-op
until item definition (job 074), and `baseline` adds a comparison section
when a baseline project is supplied. The profile is saved in `Settings`
(`report_profiles` plus the active `report_profile_name`), and the export
audit event records `profile=<name> lang=<lang>` through the new
`RecordExportRequest.detail` field (bounded at 256 printable ASCII bytes on
the server, like the kind).

### 3. Branding on the cover and every page

`ReportBranding` holds the document ID, classification
(`None/Internal/Confidential/Secret`, default `Confidential` because a TARA is
security analysis and the old watermark already said so), approvers, revision
history and a logo (capped at 1 MiB). The cover is a section with a
field/value table, the approvers and the revision history. The classification
mark is the report watermark, printed in the footer of every PDF page and as
a fixed mark in the HTML. The logo is embedded in the HTML as a data URI; the
PDF prints a caption with its alternative text (embedding raster images in
krilla is a later job).

### 4. XLSX without a spreadsheet dependency

The XLSX writer is this crate's own: XLSX is a ZIP of XML parts, and a
stored-entry ZIP plus hand-written XML is about 300 lines and adds no
dependency (the license policy then has nothing new to review). Every cell is
an inline string; text that would be read as a formula (`=`, `+`, `-`, `@`,
leading tab/CR) gets a leading `'`, the same guard the CSV writer uses
(OWASP). Cell text is XML-escaped and control characters are dropped. The
exports are `report_workbook_xlsx` (an Overview sheet, a risk-register sheet,
one sheet per element type, and a Problems sheet) and `risk_register_xlsx`
(the register alone). Sheet names are sanitized to 31 characters and made
unique.

### 5. Determinism and caps

Everything stays deterministic: the block order is fixed, the ZIP writer
emits files in a fixed order, the font cache is looked up only (P4). The
report is still O(project size) and runs on the blocking pool (R5); the PDF
page cap (`MAX_PAGES`) and the logo/profile caps are unchanged or new caps.

## Consequences

- `ExportKind::ReportXlsx` and the Reports page can export a workbook; the
  File/Reports wording stays as is.
- `record_export` takes an extra `detail` argument; the server validates it.
- The report strings are localized through the Fluent catalog of ADR 0057, so
  `ReportLanguage::English/Japanese/Bilingual` produce English and/or Japanese
  reports.

## Alternatives

- A spreadsheet crate (`rust_xlsxwriter`) was rejected to keep the shipped
  graph small and auditable; the subset of XLSX we need is tiny.
- Renumbering sections when one is disabled was rejected: fixed numbers keep
  the heading strings stable and the tests byte-comparable.
