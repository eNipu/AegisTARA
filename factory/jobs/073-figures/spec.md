# Spec — 073-figures

## Title
Figures (ARC-01, RPT-06)

## Design Reference
- Reference: docs/adr/0058-figures-and-report-images.md
- Affected crates/paths: crates/aegis-proto/proto/aegis/v1/aegis.proto, crates/aegis-server/src/store/{blob,mod,memory,folder,postgres}.rs, crates/aegis-server/migrations/005_figures.sql, crates/aegis-server/src/{registry,service,import}.rs, crates/aegis-client/src/{commands,connection,session,state,view,report/}.rs, crates/aegis-shell/{ui/app.slint,src/controller.rs}

## Problem & Goal
Attach PNG, JPEG, and sanitized static SVG images with captions to the project or to an element, stored by the server (and in the project folder in standalone mode) with per-file and per-project caps, streamed in chunks, shown in the app, and rendered as real images in HTML, PDF (`krilla`), and XLSX reports.

## Acceptance Criteria
1. PNG, JPEG, and static SVG validated and sanitized with size/dimension caps (`store/blob.rs`); 2. stored in `MemoryStore`, `FolderStore`, and `PostgresStore` (`005_figures.sql`) and copied by `import_folder`; 3. streamed in chunks over gRPC and shown in the app; 4. rendered as real images in HTML, PDF (`krilla::image::Image`), and XLSX reports.
