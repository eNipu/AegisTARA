# ADR 0058: Figure attachments and report image rendering

- Status: accepted (job 073-figures)
- Date: 2026-10-27
- Scope: `crates/aegis-proto/proto/aegis/v1/aegis.proto`,
  `crates/aegis-server/src/store/blob.rs`,
  `crates/aegis-server/src/store/{mod,memory,folder,postgres}.rs`,
  `crates/aegis-server/migrations/005_figures.sql`,
  `crates/aegis-server/src/{registry,service,import}.rs`,
  `crates/aegis-client/src/{commands,connection,session,pages,view,state}.rs`,
  `crates/aegis-client/src/report/{profile,doc,html,pdf,xlsx}.rs`,
  `crates/aegis-shell/{ui/app.slint,src/controller.rs}`
- Rules: R3 (size caps), R4 (bounded streaming), D5 (licenses), P4 (determinism),
  ARC-01, RPT-06
- Builds on: `0050-pdf-report.md`, `0056-report-engine-unification-and-export-suite.md`

## Context

Analysts need to attach system architecture diagrams, photos of hardware debug
ports, and block diagrams (PNG, JPEG, or static SVG) with captions to a project
or to a specific element (`ElementRef`), view and manage them in the desktop
application, and include them in HTML, PDF, and XLSX reports (`ARC-01`,
`RPT-06`). Previously `Block::Image` in `crates/aegis-client/src/report/pdf.rs`
only printed a caption fallback line instead of rendering the image in the PDF.

## Decisions

### 1. Server-side validation, sanitization, and size caps (`store/blob.rs`)

All figure uploads pass through `validate_and_sanitize_figure` before touching
storage:

- **Caps (R3):** `MAX_FIGURE_BYTES = 5 MiB` per file,
  `MAX_FIGURES_PER_PROJECT = 64`, `MAX_PROJECT_FIGURES_BYTES = 50 MiB` per
  project, `MAX_FIGURE_CAPTION_CHARS = 512`, `MAX_FIGURE_TARGET_BYTES = 128`,
  and `MAX_IMAGE_DIMENSION = 16_384` pixels.
- **Raster validation:** PNG (`\x89PNG\r\n\x1a\n` + `IHDR` dimensions) and JPEG
  (`FF D8 FF` + `SOF` frame dimensions) headers are parsed without pulling in an
  external image-decoding crate on the server.
- **Static SVG sanitizer:** Parses and reconstructs only an allowlist of static
  vector elements (`svg`, `g`, `path`, `rect`, `circle`, `ellipse`, `line`,
  `polyline`, `polygon`, `text`, `tspan`, `defs`, `linearGradient`,
  `radialGradient`, `stop`, `clipPath`, `marker`, `title`, `desc`, `use`) and
  safe presentation/geometry attributes. Rejects `<!DOCTYPE`, `<!ENTITY`,
  processing instructions, `<![CDATA[`, `<script>`, `<foreignObject>`,
  `<iframe>`, `<image>`, animation tags, `on*` event handlers, and any external
  or `javascript:`/`data:` URLs (`href` and `url(...)` must be local `#id`
  fragments; `xmlns` is restricted to `http://www.w3.org/2000/svg`).
- **Integrity:** Each stored figure records its `sha256` digest, pixel
  dimensions, byte size, caption, target (`""` for project-level or
  `<kind>:<uuid>` for an element), author, and timestamp.

### 2. Storage across all three backends and standalone import

- **PostgreSQL (`005_figures.sql`):** Table `aegis_figure` with
  `ON DELETE CASCADE` from `aegis_project`. Enforces per-project count and byte
  caps inside a database transaction (`put_figure`).
- **Standalone folder (`FolderStore`):** Stores `<figure_id>.bin` and
  `<figure_id>.json` inside `<project_folder>/figures/` with atomic temp-file
  write + rename.
- **Standalone import (`Registry::import_folder`):** Copies all figures from the
  standalone folder into the target server store alongside ops and baselines.
- **Wire protocol (`aegis.proto`):** `PutFigure`, `ListFigures`, `GetFigure`
  (server-streaming `FigureChunk` in `chunk_bytes` slices), `UpdateFigure`, and
  `DeleteFigure`.

### 3. Real PDF image rendering in `report/pdf.rs`

`ReportDoc` emits `Block::Image { media_type, alt, bytes }` both in the
project-level Figures section and inline after any element (`Component`,
`Interface`, `DataFlow`, `TrustBoundary`, `Asset`, `Threat`, `Tree`) targeted by
a figure.

- **HTML (`report/html.rs`):** Renders `<figure class="figure"><img src="data:...;base64,..."><figcaption>...</figcaption></figure>`.
- **PDF (`report/pdf.rs`):** Decodes PNG (`krilla::image::Image::from_png`) and
  JPEG (`krilla::image::Image::from_jpeg`) into PDF Image XObjects, and
  rasterizes sanitized static SVGs deterministically into an RGBA8 pixel buffer
  (`krilla::image::Image::from_rgba8`), drawing them onto the page surface via
  `surface.draw_image` scaled to fit the content column with the caption below.
  Invalid image bytes still fall back cleanly to the caption line.
- **XLSX (`report/xlsx.rs`):** Adds a `Figures` worksheet listing each figure's
  caption, target, media type, dimensions, byte size, author, and timestamp.

## Consequences

- PNG, JPEG, and static SVG figures round-trip through standalone folders,
  PostgreSQL, and `import-folder`, and render as real images in both HTML and
  PDF exports.
- No new third-party dependencies were added (`krilla`'s existing `raster-images`
  feature handles PNG, JPEG, and RGBA8).
