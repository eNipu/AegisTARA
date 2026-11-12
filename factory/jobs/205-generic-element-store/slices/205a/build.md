Commit: 82111d5 Add element kind table and ElementId (205a)
Files: crates/aegis-model/src/element.rs (new), src/id.rs, src/lib.rs, tests/elements.rs (new)
Done when:
1. cargo test --locked -p aegis-model --test elements: 7 passed
2. scripts/ci.sh crates/aegis-model: fmt, clippy, test all ok (includes doc tests)
3. git diff --numstat master -- crates/aegis-model/src: element.rs +269, id.rs +97/-8, lib.rs +8/-3 = 374 added, under 400
Notes: The three pub(crate) checks carry #[allow(dead_code)] (reason given) because the non-test build flags them, even though a unit test module also covers them. parse_num now delegates to parse_visual_num, behaviour unchanged.
NEXT: reviewer
