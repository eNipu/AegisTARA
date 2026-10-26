# ADR 0047: Attack tree pictures in the PDF report

- Status: accepted (job 053-pdf-tree-figures)
- Date: 2026-10-26
- Scope: `crates/aegis-canvas/src/tiles.rs`, `crates/aegis-client/src/pdf.rs`
- Rules: A5, A9, P4, R6 in `docs/engineering-standards.md`
- Builds on: `0044-pdf-report.md` (section 6 "Attack trees are text for now")

## Context

The PDF showed each attack tree only as its indented outline. The HTML report
already had a picture. A tree can be very wide (deep nesting: each level adds
about 266 world pixels) or very tall (many siblings: 76 pixels each). One page
is 500 x 540 points of drawing area, so a tree of 30 siblings shrunk onto one
page has a zoom near 0.2, where text is unreadable (the screen drops text
below zoom 0.5 for the same reason).

## Decisions

### 1. The same display list as the screen

The picture is the canvas display list, drawn as PDF vector paths and text
(not an embedded SVG or raster). Vector output stays sharp, searchable
(`pdftotext` finds the box titles) and small. Colors come from the light
print palette of the SVG adapter (`SvgTheme::Light`), so the HTML and PDF
pictures agree.

### 2. A plan decides how a tree is cut (`aegis_canvas::tiles`)

- Fits at zoom 0.5 or more: **one panel**.
- Otherwise: an **overview** panel (the whole tree on one page, boxes and
  rating colors only) followed by **detail panels** at zoom 0.6, in reading
  order (top to bottom, left to right), each tied to a grid position that its
  caption names ("Detail: row 2 of 3, column 1 of 7").
- Neighbouring panels overlap by one box, so every box is whole on at least
  one panel (a test checks this for every box).
- A panel is never larger than the part of the tree it shows, so a short
  wide tree gives short panels that share a page.
- Panels with no box are skipped. The grid is never iterated: the plan
  buckets each box by its centre, so the cost is O(n log n) in the boxes
  (1,000,000 boxes plan in 89 ms in a release build) and not in the grid size.
- At most 12 detail panels per tree. The rest are counted, the page says
  "N more detail panels are not drawn. The outline below lists every node.",
  and the outline, which is always printed, lists every node (R6).

### 3. The plan is plain data

`TilePlan` holds viewports. The PDF, an HTML export and a PNG exporter can
share it. The tile limits (12 panels, zoom 0.6, 6 point margin) are one
struct, `TileLimits`.

### 4. Edges that cross an empty cell

A cell holds a panel only if a box centre lies in it. A long edge from a gate
to a far child can cross a cell without boxes and so not appear there. It
always appears in the cell of the box it ends at, and the overview shows the
whole tree. Planning from edges too would need one range query per candidate
cell. This is a known limit, not a bug (covered by the overlap and the
overview).

## Consequences

- `Block` and `ReportDoc` lost `Eq` (floats in the drawing). Nothing used it.
- A big tree adds up to 13 panels to the report, most of a page each.
- Text width in the canvas is estimated, the PDF text uses the real font;
  a long title can be a little wider or narrower than its box. The frame
  clips everything at the panel edge.
- Not done: a figure for the residual view (the HTML report shows inherent
  only too), and drawing the selection or the critical path.

## Tests

Plan: small tree is one panel, tall and deep trees, every box whole on a
panel, cap and count, empty cells skipped, short panels, deterministic,
degenerate page, stress (`AEGIS_STRESS_NODES`, 1,000,000 locally). PDF: the
document has a figure before each outline, a tall tree (overview and detail
panels, text found by `pdftotext`, deterministic bytes), a deep tree (column
captions), and a removed tree (no figure, no failure). Samples are written to
`target/report-samples/` for a human to look at.
