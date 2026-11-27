Commit: 3ad1ee7b0355aae51f8f08b06a8aa48f2e306b51 Show reference-aware inspector ratings and causes (142g)
Files: crates/aegis-client/src/inspector.rs, crates/aegis-client/locales/en/main.ftl, crates/aegis-client/locales/ja/main.ftl, crates/aegis-client/locales/pseudo/main.ftl, crates/aegis-client/tests/inspector_view.rs
Done when: `cargo fmt --all` — PASS
Done when: `scripts/ci.sh crates/aegis-client` — PASS (fmt, clippy, and test)
Done when: `cargo test --locked -p aegis-client --test shared_fragments --test relations --test inspector_view` — PASS (19 tests: 8 inspector_view, 7 relations, 4 shared_fragments)
Notes: Reference Before/After values use cached host-node evaluations independently in each mode. Cross-tree paths retain boundary ceilings, controls, condition causes, and nested targets. Regressions cover Planned, Implemented, Off, nested shared, unrated, and excluded fragments, exact tones/labels/identities, and 142f relation behavior. Production diff: 294 changed lines including locales (cap 360).

NEXT: reviewer
