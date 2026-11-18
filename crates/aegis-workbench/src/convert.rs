//! Converting an old standalone project folder (job 107: `manifest.json`,
//! `ops.jsonl`, `ops.meta`, `snapshots/`) into workspace files written
//! beside the old files (D32). Nothing is deleted, so the next open reads
//! the new files and the old ones stay as they were.

use std::path::Path;
use std::sync::Arc;

use aegis_client::folder::{WorkspaceFolder, is_workspace};
use aegis_model::{Methodology, Project};
use aegis_server::actor::Loaded;
use aegis_server::store::LoadedProject;
use aegis_server::{FolderStore, ProjectStore};

const NO_PROJECT: &str = "the folder holds no project";
const CANNOT_READ: &str = "cannot read the old project folder: ";
const SHOWN_CHARS: usize = 64;

/// Whether `root` is an old standalone project folder: its `manifest.json`
/// is a file and it holds no workspace yet. O(1).
#[must_use]
pub fn is_legacy(root: &Path) -> bool {
    root.join("manifest.json").is_file() && !is_workspace(root)
}

/// Rebuilds the project of the old folder `root` through the server's own
/// reader (`FolderStore` and `Loaded::rebuild`) and writes it as workspace
/// files beside the old files. The returned folder holds the lock.
///
/// `FolderStore::open_existing` runs the old store's own crash recovery on
/// the folder first (an uncommitted log tail is cut back), as the Slint
/// shell does on every open.
///
/// # Errors
///
/// A message if the old folder cannot be read or rebuilt, holds no
/// project, was made with another methodology or version than
/// `methodology`, or the workspace files cannot be written.
///
/// O(snapshot bytes + ops).
pub fn convert_legacy(
    root: &Path,
    methodology: Arc<Methodology>,
) -> Result<(WorkspaceFolder, Project), String> {
    let read = |e: &dyn std::fmt::Display| format!("{CANNOT_READ}{e}");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| read(&e))?;
    let loaded = runtime.block_on(read_old(root, &methodology))?;
    drop(runtime);
    let project = Loaded::rebuild(loaded, Arc::clone(&methodology))
        .map_err(|e| read(&e))?
        .project()
        .clone();
    let folder = WorkspaceFolder::convert_beside(root, &project).map_err(|e| e.to_string())?;
    Ok((folder, project))
}

async fn read_old(root: &Path, methodology: &Methodology) -> Result<LoadedProject, String> {
    let read = |e: &dyn std::fmt::Display| format!("{CANNOT_READ}{e}");
    let store = FolderStore::open_existing(root).map_err(|e| read(&e))?;
    let meta = store
        .list(1)
        .await
        .map_err(|e| read(&e))?
        .into_iter()
        .next()
        .ok_or_else(|| NO_PROJECT.to_owned())?;
    if meta.methodology_key != methodology.key()
        || meta.methodology_version != methodology.version()
    {
        return Err(format!(
            "the folder uses {} {}, but this app opens {} {}",
            shown(&meta.methodology_key),
            shown(&meta.methodology_version),
            methodology.key(),
            methodology.version()
        ));
    }
    let loaded = store
        .load(meta.id)
        .await
        .map_err(|e| read(&e))?
        .ok_or_else(|| NO_PROJECT.to_owned())?;
    drop(store);
    Ok(loaded)
}

/// At most `SHOWN_CHARS` characters of a value read from the old folder,
/// with `…` when cut, so a message never carries unbounded input. O(1).
fn shown(value: &str) -> String {
    let mut chars = value.chars();
    let mut out: String = chars.by_ref().take(SHOWN_CHARS).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}
