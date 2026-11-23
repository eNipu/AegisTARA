# EGM Spec Audit — 135-applicability-ui

Auditors: `egm-codebase-details` (BLOCKING), `egm-implementation-readiness` (PASS).

## Blocking Defects

1. **135a — Missing `crates/aegis-client/src/table.rs` in `allow` and missing `"conditions"` cell handler**
   - `135a` adds hidden `Links(RelationKey::*_CONDITIONS)` columns with key `"conditions"` to `TableId::Assets`, `Threats`, `Goals`, and `Claims` in `view.rs`.
   - In `crates/aegis-client/src/table.rs`, `build()` calls `cell(r, c.key).unwrap_or_else(|| Cell::from_key(c.key))`, setting `cell.text = "[conditions]"` for any unmatched key. The `ColumnKind::Links` pass only fills `cell.chips` and does not clear or populate `cell.text`.
   - **Fix**: Add `crates/aegis-client/src/table.rs` to `135a`'s `allow` list in `plan.json` (and remove it from `prompts/135a.md`'s Do-not list), instruct `135a` to handle `"conditions"` in `table.rs` (e.g., return `text(String::new())` in those four table arms or populate `cell.text` in the `Links` pass), and ensure `135a`'s `max_lines` has comfortable headroom.

2. **135d — Test (d) formula-guard assertion contradicts `xlsx::guard()`**
   - `prompts/135d.md` step 5 test (d) asserts that a condition named `=HYPERLINK("x")` gets a leading `'` in the `Conditions` sheet **and** in any `Excluded` reason cell referencing it.
   - `crates/aegis-client/src/report/xlsx.rs::guard()` only prepends `'` when the cell text starts with `=`, `+`, `-`, `@`, `\t`, or `\r`. An `Excluded` reason cell starts with `"Needs CF00 =HYPERLINK(\"x\") (off)"` (`'N'`), so `guard()` does not prepend `'`.
   - **Fix**: Update test (d) in `prompts/135d.md` to assert the leading `'` on the `Conditions` sheet `Name` cell (`'=HYPERLINK("x")`), and that the `Excluded` reason cell does not start with `=` (contains `Needs CF00 =HYPERLINK("x") (off)`).

## Non-Blocking Notes (fix while updating)

- **135a**: `crates/aegis-client/src/commands.rs` uses `Edit::created(&self) -> Option<Selection>` (not `Edit::target`), which maps `Add*`/`Duplicate*` variants (`Edit::AddCondition { id, .. } => Some(Selection::Condition(*id))`).
- **135a**: Explicitly require `project.condition_idx(condition).ok_or(EditError::Missing)?` for both `Edit::RenameCondition` and `Edit::SetConditionPresent` in `to_ops`.
- **135a**: Align `spec.md` AC 1 `Edit::AddCondition { id, name }` with `prompts/135a.md`.
- **135b**: Component visual IDs use prefix `CP` (`CP00`), not `A`. Also trim `{vis} {label}` for `OwnerExcluded`/`ViaExcluded` to avoid double spaces, and note `"Via is excluded"` when `reason_target` returns `None` for `ViaExcluded`.
- **135a/135b `max_lines`**: Note that `135a` (440) and `135b` (400) are tight if fixtures or `table.rs` edits are included; keep `max_lines` realistic (up to 600 non-test cap) so builders aren't tripped by a few lines.
