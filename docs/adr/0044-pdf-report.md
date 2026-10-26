# ADR 0044: PDF report export

- Status: accepted (job 050-pdf-report)
- Date: 2026-10-26
- Scope: `crates/aegis-client/src/pdf.rs`, `crates/aegis-client/assets/fonts/`, `ExportKind::ReportPdf`, `.github/workflows/ci.yml`
- Rules: D5 (licenses), P4, R6 in `docs/engineering-standards.md`; budget "PDF report: 10 s on typical"
- Builds on: `0037-baselines-history-reports.md`

## Context

The report existed as HTML ("print to PDF" by the user). A signed, paged
document with a watermark on every page needs a real PDF. The direction paper
named Typst; the owner allowed any permissive library.

## Options

| Option | Licenses | Verdict |
|---|---|---|
| Typst as a library | Apache-2.0, large tree, own font and layout stack | Too heavy for one report: dozens of crates to audit, long builds, big binary |
| `printpdf` / `genpdf` | MIT | Older, weaker font subsetting and Unicode |
| `pdf-writer` directly | MIT/Apache-2.0 | Fonts, subsetting and text mapping by hand |
| `krilla` 0.8 | MIT/Apache-2.0 | Chosen: embeds and subsets fonts, writes tagged-ready PDF, small tree |

`scripts/deny.sh` passes (licenses, bans, sources, advisories).

## Decisions

### 1. No text shaper

`krilla`'s `simple-text` feature pulls in `rustybuzz`, which carries the
advisory RUSTSEC-2026-0206 (unmaintained). CI blocks on advisories, so the
feature is off. The module places glyphs itself, left to right, with the real
advances of the font (read with `skrifa`, the same version `krilla` uses).
There is no kerning, no ligatures, no right to left text. For names, numbers
and Latin script (German included) this is correct. A missing character
prints as `?`. If a customer needs other scripts, a shaper (`harfrust`) can
be added behind the same layout code.

### 2. One document model, two layouts

`report_doc` makes headings, paragraphs, tables and outlines from the same
page builders as the HTML report. A test keeps the section list of the two
equal. The HTML renderer was not rewritten on top of the model in this job
(no risk to the existing output); doing so removes the duplication and is a
follow-up.

### 3. Layout rules

A4 portrait; words wrap by their real width and a word wider than the column
is cut at the edge, so nothing is lost; a table column is never narrower than
its longest header word; a row taller than the rest of the page continues on
the next page line by line, with the header row repeated; a heading is kept
with the lines after it; every page has the watermark and "Page n of N" in
the footer. At most 2000 pages (R6); a longer report ends with a sentence
saying it was cut off.

### 4. Deterministic

The creation date is the export time in the context, there is no random
document ID, and the font cache is only looked up, never iterated (P4). A
test compares two runs byte for byte.

### 5. Font

Inter Regular and Bold (SIL Open Font License 1.1, allowed for fonts by D5),
about 1.2 MB, in `crates/aegis-client/assets/fonts/` with its license text.
Only the used glyphs are embedded in each PDF. The binary grows by the two
files.

### 6. Attack trees are text for now

Each tree appears as its indented outline, the same text the HTML report
shows under the picture. The picture itself (the canvas display list drawn as
PDF paths) is a follow-up; it needs no new dependency.

## Consequences

- `pdftotext` (poppler-utils) is a test tool: CI installs it, and the tests
  fail on CI if it is missing and skip on a machine without it.
- The app has "Export TARA report (PDF)" in the File menu and on the Reports
  page; the event log records `pdf-report`.
- Colour is not used to carry risk levels in the PDF; the level is printed
  as words, so a black and white print loses nothing.
