# ADR 0071: Workspace folder on disk

- Status: proposed (job 164-standalone-workspace, slices 164a and 164b)
- Date: 2026-11-18
- Crate: `crates/aegis-client` (`src/folder.rs`; tests `tests/folder.rs`
  and `tests/folder_recovery.rs`)
- Decision: D32 in `docs/decisions.md` (a workspace is a folder of JSON
  files; git is the history in standalone mode)
- Requirement: WSP-04 (open, save and recover a workspace folder)
- Builds on: `0065-workspace-files.md` (the pure file set and its caps)

## Context

ADR 0065 turns a `Project` into a list of `(path, bytes)` and back without
touching the disk. WSP-04 needs that list read from and written to a real
folder that the user also edits with git and a text editor. The folder is
untrusted input, and a save can be cut off by a crash or a power loss at
any point. A save that leaves some files new and some old can name an
element that is in no file, and the strict reader then refuses the whole
folder.

## Decision

### What is read

- At the root: every `*.json` except `manifest.json` (the old store's).
- Every regular file under `shared/` and `systems/`, at most six folder
  levels below the root (`MAX_DEPTH`); deeper is `TooDeep`.
- Names starting with `.` and names ending in `.aegis-new` are skipped.
- A symbolic link anywhere in the read set is refused (`Symlink`); no link
  is followed.
- The file count, each file's size and the total size are checked from the
  metadata before any file is read (the caps of ADR 0065), and each read is
  capped again in case a file grew.

### Lock

`.aegis.lock` in the folder is held with an OS file lock while the folder
is open. A second window or program gets `Locked`.

### Saves

Only files whose bytes differ from what was last read or written are
written. A file is removed only if this folder read or wrote it and the
project no longer has it; other files in the folder are never touched.

A save runs in four steps:

1. Stage: write each changed file as `<path>.aegis-new` and sync it.
2. Journal: write `.aegis-save.json.tmp` (`format`
   `aegis-save-journal`, `version` 1, the `replace` and `remove` path
   lists), sync it, rename it to `.aegis-save.json`, sync the folder.
3. Commit: rename each staged file over its path, remove each removed
   path, sync the touched folders, remove the journal, sync the folder.
4. Remember the new bytes.

### Recovery on open

Open, create and convert take the lock and then recover:

- If `.aegis-save.json` exists, it is checked whole before anything is
  touched: a regular file (a link is `Symlink`), at most 8 MiB, the right
  format and version, each list at most `MAX_WORKSPACE_FILES` paths, every
  path a workspace path (lower-case letters, digits, `-`, `.`, `/`,
  relative, no `.` or `..` part, ending in `.json`, at most 256 bytes),
  and every existing folder on the way to it a real folder, not a link.
  Any failure is `Journal` or `Symlink` and nothing is changed. Then each
  `replace` path whose staged file still exists is renamed into place, each
  `remove` path is removed, and the journal is removed.
  Every journal path must also be a layout path
  (`WorkspaceFile::is_layout_path`), so recovery never touches `.git/` or
  any other file; the gap between checking a folder on the way and using
  it (a symbolic link planted in between) stays, since an attacker able to
  plant links can already write the folder.
- Then `.aegis-save.json.tmp` and every regular `*.aegis-new` at the root
  and under `shared/` and `systems/` are removed. Links are skipped.

A save cut off before its journal was renamed into place opens as the old
project; one cut off after opens as the new project.

### Leaving with unsaved changes

Closing the window, quitting and opening another folder leave the project
(slice 164eb). A folder that saves is saved silently, with no dialog. The
in-memory sample with changes asks Save As…, Don't save or Cancel; a
folder whose save fails asks Try again, Don't save or Cancel, with the
error in the dialog. Don't save always lets the window close. Only one
dialog is open at a time. Opening the folder that is already open does
nothing.

## Not chosen

- One atomic write per file only (164a). Each file is whole, but a crash
  leaves a mix of old and new files that may not form a valid project.
- Writing a temporary sibling folder and swapping it in. It breaks git and
  editors that hold the folder open, and needs a folder rename, which is
  not atomic on Windows.

## Consequences

- A crash costs at most the save in progress; the folder always opens as a
  whole project.
- A save writes each changed file twice (staged, then renamed) plus a
  small journal, and syncs the touched folders.
- The folder may hold old store files beside the workspace (conversion,
  slice 164c, keeps them); they are not read and never removed.
