# Spec — 064-element-descriptions

## Title
Description text on every TARA element and project notes (`FND-01`)

## Design Reference
- Reference: `docs/adr/0051-element-descriptions.md`, `docs/security-and-architecture-review.md` (`FND-01`)
- Affected crates/paths: `crates/aegis-model/src/{description,op,project,diff,error}.rs`, `crates/aegis-model/src/store/{mod,op_log,snapshot,wire}.rs`, `crates/aegis-client/src/{commands,view,report,pdf}.rs`, `crates/aegis-shell/ui/{app,widgets}.slint`, `crates/aegis-shell/src/controller.rs`

## Problem & Goal
Enable analysts to attach plain-text descriptions (`8,000` chars max) to any asset, damage scenario, threat scenario, attack tree, security control, cybersecurity goal, or claim, plus project-wide scope/assumption notes (`15,000` chars max) on the Overview page (`FND-01`), without bloating entity structs or breaking v1 snapshot compatibility.

## Acceptance Criteria
1. Single operation `Op::SetDescription { target: ElementRef, text: Arc<str>, provenance: Provenance }` stored in a sparse `BTreeMap<ElementRef, Description>` on `Project` (`16 MiB` project-wide cap, control characters other than `\n` rejected, exact undo of text and provenance).
2. Removing an element requires its description to be empty (`ModelError::ElementHasDescription`); client delete commands clear the description in the same atomic batch so one undo restores both.
3. Snapshots omit `descriptions` when empty (preserving byte-for-byte compatibility with existing v1 snapshots) and enforce caps on load; `Project::diff` marks elements with changed descriptions as modified and reports `notes_changed()`.
4. HTML and PDF reports include project notes and element descriptions; `aegis-shell` provides a description box on every element page and a dedicated modal editor (`Ctrl+Enter` saves, `Escape` cancels) that never traps `Tab` focus.
5. All unit and integration tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
