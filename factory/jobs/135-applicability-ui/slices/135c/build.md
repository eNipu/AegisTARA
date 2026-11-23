Commit: 0a0a674 Hatch excluded canvas nodes and speak their reasons (135c)
Files:
- crates/aegis-canvas/src/scene.rs
- crates/aegis-canvas/src/render.rs
- crates/aegis-canvas/src/access.rs
- crates/aegis-canvas/tests/access.rs
- crates/aegis-canvas/tests/scene.rs
- crates/aegis-client/src/view.rs
- crates/aegis-client/tests/excluded_canvas.rs
Done when:
1. `NodeStyle::excluded` and `access::sentence` work and are tested — `cargo test --locked -p aegis-canvas --test access` passed (9 passed, 0 failed).
2. Canvas hatch lines, `"excluded"` label, and `tree_scene` integration work for partial and fully-excluded trees — `cargo test --locked -p aegis-client --test excluded_canvas` passed (1 passed, 0 failed).
3. Existing canvas scene snapshots and client tests remain green — `scripts/ci.sh crates/aegis-canvas crates/aegis-client` passed (fmt, clippy, test all ok in 2 crates).
Notes:
- Added `NodeStyle::excluded` and `NodeAccess::excluded`, 45-degree `Item::Polyline` hatch lines at `z >= DETAIL_ZOOM`, bold `Paint::Graphite` `"excluded"` score text and via-chip width reservation at `z >= TEXT_ZOOM`, `access::sentence` `" Excluded: <reasons>."` suffix, and `tree_scene` integration via `Project::applicability()` and `crate::exclusion::reasons_text`.
- Source diff is 70 added / 6 removed lines across the four allowed source files; no new `Item` variants, no dependencies added, no `svg.rs` edits.
NEXT: reviewer
