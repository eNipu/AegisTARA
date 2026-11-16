Commit: 360c9ff8bcc1ac27446c4033a629b7475c863e2c Add conflicts_with on setting presets and conflicting-settings lint (126a)
Files: crates/aegis-model/src/methodology/mod.rs, crates/aegis-model/src/methodology/pack.rs, crates/aegis-model/src/error.rs, crates/aegis-model/src/store/mod.rs, crates/aegis-model/src/item.rs, crates/aegis-model/src/lint.rs, crates/aegis-model/src/project.rs, crates/aegis-client/src/pages.rs, crates/aegis-model/tests/control_kinds.rs, crates/aegis-model/tests/pack_limits.rs, crates/aegis-model/tests/builtin_pack.rs, crates/aegis-model/tests/setting_conflicts.rs, crates/aegis-client/tests/pages.rs
Done when:
- cargo test --locked -p aegis-model --test control_kinds --test builtin_pack --test stride_pack: passed
- cargo test --locked -p aegis-model --test control_kinds --test pack_limits: passed
- cargo test --locked -p aegis-model --test setting_conflicts: passed
- cargo test --locked -p aegis-client --test pages: passed
- cargo test --locked -p aegis-model --test control_presets --test control_settings --test builtin_pack --test stride_pack: passed
- git diff --numstat HEAD~1 -- crates/aegis-model/src crates/aegis-client/src: 220 lines (<=320)
Notes: Implemented model data, lint code, and Problems-page arm; shipped packs unchanged.
NEXT: reviewer