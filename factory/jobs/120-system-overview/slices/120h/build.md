Commit: 26c2e55 Add the system of a hidden element to the view on undo and redo (120h)
Files: crates/aegis-client/src/systems.rs, crates/aegis-client/src/session.rs, crates/aegis-client/tests/systems_view.rs, crates/aegis-client/tests/session.rs
Done when:
1. reveal: `cargo test --locked -p aegis-client --test systems_view` ok (8 passed)
2. undo/redo reveal: `cargo test --locked -p aegis-client --test session` ok (24 passed)
3. `scripts/ci.sh`: fmt, clippy, test all passed
Notes: Non-test source change is 31 lines. In the session test, "saved" is detected by `confirmed_seq` rising (the undo label text does not contain the new name). The two-system test uses a control applied to Camera (ADAS) and a Gateway component.
NEXT: reviewer
