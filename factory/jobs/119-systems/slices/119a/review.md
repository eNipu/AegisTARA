VERDICT: PASS
## For the owner
What changed: A project can now hold "systems" (Brake, ADAS, Gateway) as a second row in the generic element table.
They are added, renamed and removed by the existing element actions and saved by the existing file code, with no new storage code.
A project holds at most 64 systems, and a system cannot link to anything yet.
Look at:
- crates/aegis-model/src/element.rs:118: the new system row (prefix "SYS", file "systems.json", 64 max, no links). This is the whole product change. Check the names are what a customer should see.
- crates/aegis-model/tests/elements.rs:78: an older test said every kind must have link targets. It now says "a kind has targets only if it allows links". This was needed and is a fair rule.
- crates/aegis-model/tests/elements.rs:966: proves the 64-system limit, and that misuse cases are counted separately.
Risk: Low. Old project files still load: the fixture test shows a v1 file loads with 0 systems. A project with no systems writes no systems file, so existing workspaces do not change. Round-trip tests compare the saved bytes, not just `==`. Systems never go past SYS63, so the 3-digit visual-ID problem (L06) cannot happen. No other crate uses `ElementKind` (checked with grep), and the whole workspace still compiles.
View: git -C factory/worktrees/119-systems diff 99582539e4351950e8a9be9052ebd4b85fc5b37b..HEAD
## Findings
- none. Every "Done when" item has a test that would fail if the change were removed. The tests cover: the row fields, `SYS03`, add/rename/remove with exact inverses, separate visual numbers, refusing the 65th system, refusing links, the snapshot `elements.system` key with byte-equal re-save, the op-log `"kind":"system"`, the workspace file `systems/main/systems.json` with byte-equal read-back, no file when there are no systems, and the old fixture. The size cap is enforced at element.rs:644 from the row's `max_elements`. There is no new dependency and nothing outside scope. `workspace_limits.rs` needed no change.
## Lesson
none
NEXT: human
