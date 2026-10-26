# Brainstorm notes: logs, big trees in reports, HTML report

- Status: notes for a later discussion. What is built is marked **built**.
- Date: 2026-10-26

## 1. Keeping logs: storage and encoding

Three different logs exist. They have different needs.

| Log | Role | Stored as | Size control |
|---|---|---|---|
| Project operation log (`aegis_project_op`) | Source of truth, hash chained | Canonical JSON in `bytea`, one row per op | Never trimmed. Snapshots bound replay time |
| Event log (`aegis_event`) | Who did what: logins, exports, admin actions | Typed rows, short `text` columns | **Built (job 052):** age and row cap, hourly |
| Server log (`tracing`) | Operations: errors, startup, timing | Lines on stderr | The platform's log rotation |

Recommendations:

1. **Event log: typed rows, not files.** The admin screen needs newest first,
   a limit and a filter. Indexed rows do that with no parser. Values are bounded
   (256 characters), so a row has a known size (about 150 bytes).
2. **Do not invent a binary encoding for events.** PostgreSQL already
   compresses large values (TOAST) and these are small. A custom encoding
   would save a few percent and cost audit tooling. Use plain columns.
3. **Retention is the real saving.** Age plus a row cap (built). A login
   flood is the realistic growth risk, because it is unauthenticated.
4. **For customers who must keep audit data for years:** before trimming,
   write the old events to a compressed archive file (newline-delimited JSON
   with zstd, one file per month, with the SHA-256 of each file in a manifest).
   That is 5 to 10 times smaller than the table and easy for auditors to read
   with standard tools. Not built; needs a decision on where the archive lives
   (disk, S3-compatible store).
5. **Large installations:** partition `aegis_event` by month and drop whole
   partitions. Dropping a partition is instant and leaves no dead rows.
6. **Tamper evidence:** the project log has a hash chain; the event log has
   none. Chaining events (each row holds the hash of the previous row) would
   make deleted rows detectable. The cost is a serialized write path. Decide
   if auditors need it.
7. **The operation log encoding.** Canonical JSON is easy to hash, diff and
   debug, and the hash chain is defined over it. A compact binary form
   (CBOR or postcard) with zstd would shrink the table, but the chain and
   every fixture would change. If op storage becomes large, compress the
   `op` column with zstd per batch and keep the canonical JSON as the thing
   that is hashed. Measure first: the large reference project is small.

## 2. Big trees in the PDF

**Built (job 053, ADR 0047).** The rule is "readable beats complete on one
page":

- A tree that fits at zoom 0.5 or more is one panel.
- Otherwise an overview panel (boxes and colors) plus detail panels at
  zoom 0.6 in reading order, with captions "row r of R, column c of C".
- Panels overlap by one box. Empty panels are skipped. A panel is no larger
  than its content. At most 12 detail panels; the rest are named in a note,
  and the printed outline always lists every node.
- Cost is O(n log n) in the boxes. A million boxes plan in under 0.1 s.

Open questions for the brainstorm:

- Should the printed limit (12 panels) be a setting? Large customers may
  prefer an appendix with every panel.
- Collapse by depth ("show the first 3 levels, then a box that says '+ 40
  steps'") would give an overview that carries meaning. It needs a "collapsed"
  concept in the scene, which the screen could use too.
- A landscape page for wide trees would show about 1.6 times more per panel.
- Do trees really grow past 32 nodes? The model caps a tree at 32 nodes
  today (`MAX_ATTACK_TREE_NODES`), so the cases above are insurance until the
  cap is raised (decision needed, it touches the wire format caps).
- Edge only cells: a long edge crossing a cell with no box is not drawn
  there (ADR 0047 section 4).

## 3. An HTML report next to the PDF

An HTML report **already exists** (File menu, Reports page). It has the same
sections as the PDF (a test keeps them equal) and an inline SVG per tree.
What is missing is what HTML is good at:

| Idea | Value | Cost |
|---|---|---|
| Zoom and pan of the tree picture in the browser | Big trees become usable (SVG scales, a small script pans) | One inline script, no network. Content-security rules must be checked (the file opens from disk) |
| Collapsible sections and a table of contents | Long reports are navigable | Small |
| Search box over the risk table | Finding a threat in 500 rows | Small script |
| Click a risk to jump to its tree | Traceability | Anchors only |
| Use the same document model as the PDF | One source for two outputs, no drift | Rewrite of `report_html` (ADR 0044 follow-up) |
| Light and dark, print stylesheet | Reads well on screen and paper | Small |

Recommendation: worth doing, in this order: (1) move `report_html` onto the
`ReportDoc` model, (2) table of contents and anchors, (3) pan and zoom for
tree pictures, (4) search. All need no network and no dependency. The HTML
file is the better "working copy" and the PDF the better "signed copy".

## 4. Local environment, Docker and stress tests

- The Docker daemon can run in the cloud session, but Docker Hub answered 429
  when pulling `postgres:16`, so images could not be built or run here.
  `deploy/compose.test.yaml` and `deploy/verify-image.sh` still run in CI.
  The same checks were run natively against a local PostgreSQL 16.
- Stress tests added: event log (16 writers plus a trimmer; set
  `AEGIS_STRESS_EVENTS`, 200,000 events passed at about 8,000 per second) and
  the tile plan (`AEGIS_STRESS_NODES`, 1,000,000 boxes in 89 ms).
- Idea: one `scripts/stress.sh` that runs every stress test at a large size
  and prints a table, plus a nightly CI job.
