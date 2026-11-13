Commit: 22489ed A library step may suggest an attack type (122dc)
Files: crates/aegis-model/src/library.rs, crates/aegis-model/packs/library-demo.json, crates/aegis-model/tests/library.rs, crates/aegis-client/src/library.rs, crates/aegis-client/tests/library_insert.rs, crates/aegis-client/tests/library_view.rs, tools/aegis-ai/src/{corpus,dataset,dsl,verifier}.rs, docs/architecture.md
Done when:
1. cargo test --locked -p aegis-model --test library: 15 passed, 1 ignored.
2. cargo test --locked -p aegis-client --test library_insert --test library_view: 12 and 7 passed.
3. cargo check --locked --all-targets --manifest-path tools/aegis-ai/Cargo.toml: finished.
4. scripts/ci.sh: fmt, clippy, test all passed.
5. git diff --numstat HEAD~1 over the three src dirs: 63 added, 6 removed (cap 200).
Notes: dataset.rs bind_node_slots sets attack_type: None as the prompt said; it could clone node.attack_type instead. Demo pack diff is the three replacements plus the hash (the rehash test writes canonical bytes). An accidental sed backup commit (*-E files) was removed by amending before hand-off.
NEXT: reviewer
