VERDICT: PASS
## For the owner
What changed: the workbench now has a start screen view (not shown in the window yet; that comes in 208d2).
It has a project folder field and a license file field, both filled from the saved settings, plus Open, New workspace… and Try the sample.
Open and New workspace check the license file first, and nothing opens unless the license is valid and signed by a trusted key.
Look at: crates/aegis-workbench/src/app/start.rs:151-179 (`checked`: the one gate every local open goes through. Empty folder, then empty license, then `read_license`, which caps the file size and checks the signature (208c)).
crates/aegis-workbench/src/app/start.rs:191-209 (`create_local`: a new workspace may only go into a missing or empty folder, so it never writes over files that are already there).
crates/aegis-workbench/src/app/start.rs:167-175 (when a check fails, the license from an earlier check stays in `self.license`, so the "Licensed to X" line can still show next to the new error. It is a display problem only: no event goes out. 208d2 can clear it).
Risk: low. The view is byte-for-byte the reviewed branch file (`git diff factory/208d-full HEAD` on start.rs, Cargo.toml and Cargo.lock prints nothing). The only lock change is the workspace crate `aegis-license`, and no third-party crate was added. An expired license still sends `Open` with `editing == false`, which is the intended read-only mode; the window has to respect that flag in 208d2. Seven view tests cover each path, and each would fail if its feature were removed.
View: git -C factory/worktrees/208-gpui-sign-in diff 720eb6a3760f62256e59cd92e34189855f6e3835..HEAD
## Findings
- none blocking. Note for 208d2: `checked` (start.rs:172-174) does not reset `self.license` on error, so the license line can go stale. Set `self.license = None` in the `Err` arm when start.rs is next touched (this slice was told not to change start.rs).
- Every "Done when" item is met. Tests: empty-field order (start_local.rs:137), valid Open with customer and editing (149), foreign key refused (171), create into a non-empty folder refused and into a missing folder allowed (189), sample (213), remembered paths with no setter calls (222), Enter key through the real input subscription (243). Locales: 13 `start-*` ids in en, ja and pseudo, and no `cmd-back-to-start*`. mod.rs has only the two lines asked for.
## Lesson
none
NEXT: human
