Commit: 62036ff Add shared step entity and lifecycle operations (141a)
Files: crates/aegis-model/src/blocks.rs, crates/aegis-model/src/id.rs, crates/aegis-model/src/description.rs, crates/aegis-model/src/error.rs, crates/aegis-model/src/op.rs, crates/aegis-model/src/diff.rs, crates/aegis-model/src/project.rs, crates/aegis-model/src/lib.rs, crates/aegis-model/src/store/op_log.rs, crates/aegis-model/src/store/wire.rs, crates/aegis-model/tests/common/mod.rs, crates/aegis-model/tests/ops_properties.rs, crates/aegis-model/tests/shared_steps.rs
Done when:
- cargo fmt --all: passed
- cargo test --locked -p aegis-model --test shared_steps --test ops_properties: passed (6 + 11 tests)
- cargo test --locked -p aegis-model --lib shared_step: passed (2 tests)
- scripts/ci.sh crates/aegis-model: fmt and clippy passed; test failed only in store_properties snapshot/workspace round-trips because shared-step persistence is deferred to 141d
- git diff --numstat e1fbce3 -- crates/aegis-model/src crates/aegis-model/benches plus blocks.rs: 560 counted source lines (cap 620)
Notes:
- The prompt's empty-actor rejection conflicts with the existing provenance contract: check_provenance intentionally accepts empty actors and existing tests require that behavior. The regression test now verifies whitespace-only rejection and empty-actor compatibility rather than changing global semantics.
- SharedStep::via remains crate-internal because ItemIdx is an arena index. Public callers use Project::shared_step_via, which returns the stable public ItemRef required by client relations and L29.
- Review found and fixed validate/apply parity at the project-wide arena slot limit; the regression test covers Project::with_slot_limit(..., 0).
- Snapshot/workspace persistence is intentionally deferred to 141d. No persistence property was skipped or weakened; the integrated model gate will be rerun after 141d.
NEXT: reviewer
