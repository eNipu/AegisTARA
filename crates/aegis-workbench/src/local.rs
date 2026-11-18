//! Edits, undo and redo on a workspace with no server: each change is
//! confirmed locally right after it is applied, so nothing stays pending.

use aegis_client::commands::Edit;
use aegis_client::workspace::{Origin, Workspace, WorkspaceError};

/// The name recorded as the author of local edits: `USER`, else
/// `USERNAME`, else `"local"`.
#[must_use]
pub fn actor() -> String {
    ["USER", "USERNAME"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "local".to_owned())
}

/// Confirms every pending edit, as a server would.
fn confirm(ws: &mut Workspace) -> Result<(), String> {
    while let Some(s) = ws.next_submission() {
        let last = s.base_seq + s.ops.len() as u64;
        ws.on_accepted(last).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn done(ws: &mut Workspace, res: Result<(), WorkspaceError>) -> Result<(), String> {
    res.map_err(|e| e.to_string())?;
    confirm(ws)
}

/// Applies `edit` as `actor` at `now_ms` and confirms it.
///
/// # Errors
///
/// The text of the refused edit or op; the project is left as it was.
pub fn apply_edit(ws: &mut Workspace, edit: &Edit, actor: &str, now_ms: u64) -> Result<(), String> {
    let ops = edit
        .to_ops(ws.project(), actor, now_ms)
        .map_err(|e| e.to_string())?;
    let res = ws.apply(ops, Origin::Edit);
    done(ws, res)
}

/// Undoes the newest edit and confirms it.
///
/// # Errors
///
/// "Nothing to undo" or the text of the refused op.
pub fn undo(ws: &mut Workspace) -> Result<(), String> {
    let res = ws.undo();
    done(ws, res)
}

/// Redoes the newest undone edit and confirms it.
///
/// # Errors
///
/// "Nothing to redo" or the text of the refused op.
pub fn redo(ws: &mut Workspace) -> Result<(), String> {
    let res = ws.redo();
    done(ws, res)
}
