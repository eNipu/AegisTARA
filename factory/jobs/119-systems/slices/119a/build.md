Commit: 7232db9 Add a system kind to the element store (119a)
Files: crates/aegis-model/src/element.rs, src/lib.rs, src/store/mod.rs, tests/elements.rs, tests/elements_store.rs, tests/workspace.rs
Done when:
1. cargo test --locked -p aegis-model --test elements: pass
2. cargo test --locked -p aegis-model --test elements_store --test workspace --test workspace_limits: pass
3. Doc tests: pass (run inside scripts/ci.sh crates/aegis-model)
Before commit: cargo fmt --all, scripts/ci.sh crates/aegis-model (fmt, clippy, test ok), the four test files pass.
Notes: Non-test source change is 23 lines. Test row_caps_fit_the_global_caps asserted every kind has targets; changed to "targets non-empty iff max_links > 0". workspace_limits.rs needed no change. The op-log test uses common::write_log/read_log.
NEXT: reviewer
