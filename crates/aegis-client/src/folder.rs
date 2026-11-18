//! A workspace folder on disk (WSP-04, D32): open, create and save through
//! the file set of `aegis-model`. std only.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aegis_model::{
    MAX_PROJECT_FILE_BYTES, MAX_WORKSPACE_FILE_BYTES, MAX_WORKSPACE_FILES, Methodology, Project,
    StoreError,
};
use serde::{Deserialize, Serialize};

/// The OS lock file, held while a folder is open.
const LOCK: &str = ".aegis.lock";
/// Suffix of a file being written, renamed over its target once synced.
const STAGED: &str = ".aegis-new";
/// Deepest directory level below the root that is read.
const MAX_DEPTH: usize = 6;
/// Longest path shown in an error, in characters.
const MAX_SHOWN: usize = 256;
/// The record of a save being committed; see [`recover`].
const JOURNAL: &str = ".aegis-save.json";
/// The journal while it is written, renamed to [`JOURNAL`] once synced.
const JOURNAL_TMP: &str = ".aegis-save.json.tmp";
/// Largest journal that is read.
const MAX_JOURNAL_BYTES: u64 = 8 * 1024 * 1024;
/// The journal's `format` value.
const JOURNAL_FORMAT: &str = "aegis-save-journal";

/// The roll-forward record of a save: once it is on disk, every staged
/// `replace` file is renamed over its path and every `remove` path removed.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    format: String,
    version: u32,
    replace: Vec<String>,
    remove: Vec<String>,
}

/// Where [`WorkspaceFolder::save_until`] stops, as a crash would.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStop {
    /// Every changed file is staged; no journal yet.
    AfterStaging,
    /// The journal is on disk; nothing is committed yet.
    AfterJournal,
}

/// Why a workspace folder could not be opened, created or saved.
#[derive(Debug, thiserror::Error)]
pub enum FolderError {
    /// Reading or writing a file failed.
    #[error("{path}: {source}")]
    Io {
        /// The file, relative to the folder, or the folder itself.
        path: String,
        /// The OS error.
        #[source]
        source: io::Error,
    },
    /// Another window or program holds the folder's lock.
    #[error("{path} is open in another window or program")]
    Locked {
        /// The folder.
        path: String,
    },
    /// A new workspace was asked for in a folder that holds files.
    #[error("{path} is not empty")]
    NotEmpty {
        /// The folder.
        path: String,
    },
    /// The folder already has an `aegis.json`.
    #[error("{path} already holds a workspace")]
    AlreadyWorkspace {
        /// The folder.
        path: String,
    },
    /// The folder has no `aegis.json`.
    #[error("{path} holds no aegis.json")]
    NotAWorkspace {
        /// The folder.
        path: String,
    },
    /// A file or folder of the workspace is a symbolic link.
    #[error("{path} is a symbolic link")]
    Symlink {
        /// The link, relative to the folder.
        path: String,
    },
    /// A folder lies more than six levels below the workspace folder.
    #[error("{path} is nested too deep")]
    TooDeep {
        /// The folder, relative to the workspace folder.
        path: String,
    },
    /// One file is over its size cap.
    #[error("{path} is larger than {max} bytes")]
    TooLarge {
        /// The file, relative to the folder.
        path: String,
        /// The cap in bytes.
        max: u64,
    },
    /// The folder holds more files than a workspace may have.
    #[error("the workspace has more than {max} files")]
    TooManyFiles {
        /// The cap on the number of files.
        max: usize,
    },
    /// The files together are over the project size cap.
    #[error("the workspace files are larger than {max} bytes in total")]
    TooBig {
        /// The cap in bytes.
        max: u64,
    },
    /// The files do not form a valid project, or the project cannot be
    /// written as files.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The record of an unfinished save cannot be trusted; nothing was
    /// changed.
    #[error("the unfinished save record is not valid: {problem}")]
    Journal {
        /// What is wrong, with at most 256 characters of the record.
        problem: String,
    },
}

/// What a [`WorkspaceFolder::save`] changed on disk.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveReport {
    /// Paths written, relative to the folder, sorted.
    pub written: Vec<String>,
    /// Paths removed, relative to the folder, sorted.
    pub removed: Vec<String>,
}

/// An open workspace folder. It holds the folder's OS lock until dropped.
#[derive(Debug)]
pub struct WorkspaceFolder {
    root: PathBuf,
    /// Per layout path, the bytes last read from or written to disk.
    written: BTreeMap<String, Vec<u8>>,
    _lock: File,
}

/// Whether `root` holds a workspace: its `aegis.json` is a file. O(1).
#[must_use]
pub fn is_workspace(root: &Path) -> bool {
    root.join("aegis.json").is_file()
}

impl WorkspaceFolder {
    /// Opens the workspace in `root` and reads its project. Only
    /// `*.json` at the root (except `manifest.json`) and the files under
    /// `shared/` and `systems/` are read; names starting with `.` are
    /// skipped. The folder stays locked while the result lives. A save
    /// that was cut off is first finished, or undone if its journal was not
    /// yet written; left-over staged files are removed.
    ///
    /// # Errors
    ///
    /// [`FolderError::NotAWorkspace`] without `aegis.json`,
    /// [`FolderError::Locked`] if it is open elsewhere,
    /// [`FolderError::Journal`] or [`FolderError::Symlink`] for a save
    /// record that cannot be trusted (nothing is changed then),
    /// [`FolderError::Symlink`], [`FolderError::TooDeep`],
    /// [`FolderError::TooLarge`], [`FolderError::TooManyFiles`] or
    /// [`FolderError::TooBig`] for a folder outside the caps (checked before
    /// any file is read), [`FolderError::Store`] for files that are not a
    /// valid project and [`FolderError::Io`] for a failed read.
    ///
    /// O(total bytes).
    pub fn open(
        root: &Path,
        methodology: Arc<Methodology>,
    ) -> Result<(Self, Project), FolderError> {
        let unfinished = fs::symlink_metadata(root.join(JOURNAL)).is_ok();
        if !is_workspace(root) && !unfinished {
            return Err(FolderError::NotAWorkspace { path: shown(root) });
        }
        let lock = take_lock(root)?;
        recover(root)?;
        if !is_workspace(root) {
            return Err(FolderError::NotAWorkspace { path: shown(root) });
        }
        let mut written = BTreeMap::new();
        let mut total: u64 = 0;
        for (rel, full, _) in collect(root)? {
            let bytes = read_capped(&full, &rel, MAX_WORKSPACE_FILE_BYTES)?;
            total = total.saturating_add(bytes.len() as u64);
            if total > MAX_PROJECT_FILE_BYTES {
                return Err(FolderError::TooBig {
                    max: MAX_PROJECT_FILE_BYTES,
                });
            }
            written.insert(rel, bytes);
        }
        let project = Project::from_workspace_files(
            written.iter().map(|(p, b)| (p.as_str(), b.as_slice())),
            methodology,
        )?;
        let folder = Self {
            root: root.to_path_buf(),
            written,
            _lock: lock,
        };
        Ok((folder, project))
    }

    /// Creates a new workspace in `root` and writes `project` to it. `root`
    /// is created if missing; if it exists it may hold only names starting
    /// with `.` (such as `.git` or `.DS_Store`).
    ///
    /// # Errors
    ///
    /// [`FolderError::NotEmpty`] for a folder with other files,
    /// [`FolderError::Locked`] if it is open elsewhere, and as
    /// [`save`](Self::save).
    ///
    /// O(total bytes).
    pub fn create(root: &Path, project: &Project) -> Result<Self, FolderError> {
        if root.exists() {
            for entry in fs::read_dir(root).map_err(io_at(&shown(root)))? {
                let entry = entry.map_err(io_at(&shown(root)))?;
                if !entry.file_name().to_string_lossy().starts_with('.') {
                    return Err(FolderError::NotEmpty { path: shown(root) });
                }
            }
        }
        fs::create_dir_all(root).map_err(io_at(&shown(root)))?;
        Self::start(root, project)
    }

    /// Writes `project` as a workspace into the existing folder `root`,
    /// beside what it already holds. Other files in the folder are not
    /// touched. The caller must have released any other lock on
    /// `.aegis.lock` first (an old project folder's store holds one).
    ///
    /// # Errors
    ///
    /// [`FolderError::AlreadyWorkspace`] if `root` has an `aegis.json`,
    /// [`FolderError::Io`] if `root` does not exist,
    /// [`FolderError::Locked`] if it is open elsewhere, and as
    /// [`save`](Self::save).
    ///
    /// O(total bytes).
    pub fn convert_beside(root: &Path, project: &Project) -> Result<Self, FolderError> {
        fs::metadata(root).map_err(io_at(&shown(root)))?;
        if fs::symlink_metadata(root.join("aegis.json")).is_ok() {
            return Err(FolderError::AlreadyWorkspace { path: shown(root) });
        }
        Self::start(root, project)
    }

    fn start(root: &Path, project: &Project) -> Result<Self, FolderError> {
        let mut folder = Self {
            root: root.to_path_buf(),
            written: BTreeMap::new(),
            _lock: take_lock(root)?,
        };
        recover(root)?;
        folder.save(project)?;
        Ok(folder)
    }

    /// Saves `project`, writing only the files whose bytes differ from what
    /// was last read or written, and removing files this folder wrote or
    /// read that the project no longer has. Nothing changed means nothing is
    /// touched. The save is all or nothing across a crash: each file is
    /// staged as `<path>.aegis-new` and synced, then a journal naming every
    /// change is synced, then the staged files are renamed into place and
    /// the journal removed. The next open finishes a save whose journal
    /// exists and drops one whose journal does not.
    ///
    /// # Errors
    ///
    /// [`FolderError::Store`] if the project cannot be written as files and
    /// [`FolderError::Io`] for a failed write; the next save writes the
    /// changed files again.
    ///
    /// O(total bytes).
    pub fn save(&mut self, project: &Project) -> Result<SaveReport, FolderError> {
        self.save_inner(project, None)
    }

    /// Stops a save part way, as a crash would. Tests only.
    ///
    /// # Errors
    ///
    /// As [`save`](Self::save).
    #[doc(hidden)]
    pub fn save_until(&mut self, project: &Project, stop: SaveStop) -> Result<(), FolderError> {
        self.save_inner(project, Some(stop)).map(drop)
    }

    fn save_inner(
        &mut self,
        project: &Project,
        stop: Option<SaveStop>,
    ) -> Result<SaveReport, FolderError> {
        let files = project.to_workspace_files()?;
        let changed: Vec<_> = files
            .iter()
            .filter(|f| self.written.get(&f.path) != Some(&f.bytes))
            .collect();
        let kept: BTreeSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
        let removed: Vec<String> = self
            .written
            .keys()
            .filter(|p| !kept.contains(p.as_str()))
            .cloned()
            .collect();
        if changed.is_empty() && removed.is_empty() {
            return Ok(SaveReport::default());
        }
        let mut dirs = BTreeSet::new();
        for file in &changed {
            let target = full_path(&self.root, &file.path);
            let dir = target.parent().unwrap_or(&self.root).to_path_buf();
            fs::create_dir_all(&dir).map_err(io_at(&file.path))?;
            write_synced(&staged(&target), &file.bytes, &file.path)?;
            dirs.insert(dir);
        }
        if stop == Some(SaveStop::AfterStaging) {
            return Ok(SaveReport::default());
        }
        let journal = Journal {
            format: JOURNAL_FORMAT.to_owned(),
            version: 1,
            replace: changed.iter().map(|f| f.path.clone()).collect(),
            remove: removed.clone(),
        };
        let bytes = serde_json::to_vec(&journal).map_err(|e| io_at(JOURNAL_TMP)(e.into()))?;
        let tmp = self.root.join(JOURNAL_TMP);
        write_synced(&tmp, &bytes, JOURNAL_TMP)?;
        fs::rename(&tmp, self.root.join(JOURNAL)).map_err(io_at(JOURNAL))?;
        let root = BTreeSet::from([self.root.clone()]);
        sync_dirs(&root)?;
        if stop == Some(SaveStop::AfterJournal) {
            return Ok(SaveReport::default());
        }
        for file in &changed {
            let target = full_path(&self.root, &file.path);
            fs::rename(staged(&target), &target).map_err(io_at(&file.path))?;
        }
        for path in &removed {
            let target = full_path(&self.root, path);
            remove_if_there(&target, path)?;
            dirs.insert(target.parent().unwrap_or(&self.root).to_path_buf());
        }
        sync_dirs(&dirs)?;
        fs::remove_file(self.root.join(JOURNAL)).map_err(io_at(JOURNAL))?;
        sync_dirs(&root)?;
        let report = SaveReport {
            written: changed.iter().map(|f| f.path.clone()).collect(),
            removed,
        };
        for file in changed {
            self.written.insert(file.path.clone(), file.bytes.clone());
        }
        for path in &report.removed {
            self.written.remove(path);
        }
        Ok(report)
    }

    /// The folder. O(1).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folder's own name (its last path component), or the whole path
    /// if it has none. O(path length).
    #[must_use]
    pub fn name(&self) -> String {
        self.root.file_name().map_or_else(
            || self.root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }
}

#[cfg(unix)]
fn sync_dirs(dirs: &BTreeSet<PathBuf>) -> Result<(), FolderError> {
    for dir in dirs {
        File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(io_at(&shown(dir)))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_dirs(_dirs: &BTreeSet<PathBuf>) -> Result<(), FolderError> {
    Ok(())
}

/// Cuts a path to at most [`MAX_SHOWN`] characters.
fn cut(path: &str) -> String {
    path.chars().take(MAX_SHOWN).collect()
}

fn shown(path: &Path) -> String {
    cut(&path.display().to_string())
}

fn io_at(path: &str) -> impl FnOnce(io::Error) -> FolderError {
    let path = cut(path);
    move |source| FolderError::Io { path, source }
}

/// `root` joined with a `/`-separated relative path.
fn full_path(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, s| p.join(s))
}

/// `target` with [`STAGED`] appended.
fn staged(target: &Path) -> PathBuf {
    let mut staged = target.as_os_str().to_owned();
    staged.push(STAGED);
    PathBuf::from(staged)
}

/// Writes `bytes` to `path` (created or truncated) and syncs it.
fn write_synced(path: &Path, bytes: &[u8], rel: &str) -> Result<(), FolderError> {
    let mut out = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(io_at(rel))?;
    out.write_all(bytes).map_err(io_at(rel))?;
    out.sync_all().map_err(io_at(rel))
}

fn remove_if_there(path: &Path, rel: &str) -> Result<(), FolderError> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(io_at(rel)(e)),
        _ => Ok(()),
    }
}

fn journal_error(problem: String) -> FolderError {
    FolderError::Journal { problem }
}

/// Whether a journal path is a workspace path: lower-case ASCII letters,
/// digits, `-`, `.` and `/`, relative, no empty, `.` or `..` part, at most
/// 256 bytes, ending in `.json`. O(length).
fn valid_journal_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= MAX_SHOWN
        && p.bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'/'))
        && !p.starts_with('/')
        && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
        && p.ends_with(".json")
}

/// Reads and checks the journal in `root`, touching nothing.
fn read_journal(root: &Path) -> Result<Journal, FolderError> {
    let path = root.join(JOURNAL);
    let meta = fs::symlink_metadata(&path).map_err(io_at(JOURNAL))?;
    if meta.file_type().is_symlink() {
        return Err(FolderError::Symlink {
            path: JOURNAL.to_owned(),
        });
    }
    if !meta.is_file() {
        return Err(journal_error("it is not a file".to_owned()));
    }
    let too_large = || journal_error(format!("it is larger than {MAX_JOURNAL_BYTES} bytes"));
    if meta.len() > MAX_JOURNAL_BYTES {
        return Err(too_large());
    }
    let bytes = read_capped(&path, JOURNAL, MAX_JOURNAL_BYTES).map_err(|e| match e {
        FolderError::TooLarge { .. } => too_large(),
        e => e,
    })?;
    let journal: Journal = serde_json::from_slice(&bytes).map_err(|e| {
        journal_error(format!(
            "it is not a save record (line {}, column {})",
            e.line(),
            e.column()
        ))
    })?;
    if journal.format != JOURNAL_FORMAT {
        return Err(journal_error("its format is unknown".to_owned()));
    }
    if journal.version != 1 {
        return Err(journal_error(format!(
            "version {} is not supported",
            journal.version
        )));
    }
    for list in [&journal.replace, &journal.remove] {
        if list.len() > MAX_WORKSPACE_FILES {
            return Err(journal_error(format!(
                "it names more than {MAX_WORKSPACE_FILES} paths"
            )));
        }
    }
    for p in journal.replace.iter().chain(&journal.remove) {
        if !valid_journal_path(p) {
            let shown: String = p
                .chars()
                .take(MAX_SHOWN)
                .flat_map(char::escape_debug)
                .collect();
            return Err(journal_error(format!(
                "\"{shown}\" is not a workspace path"
            )));
        }
        for (end, _) in p.match_indices('/') {
            let at = &p[..end];
            match fs::symlink_metadata(full_path(root, at)) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(FolderError::Symlink {
                        path: at.to_owned(),
                    });
                }
                Ok(m) if !m.is_dir() => {
                    return Err(journal_error(format!("{at} is not a folder")));
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => break,
                Err(e) => return Err(io_at(at)(e)),
            }
        }
    }
    Ok(journal)
}

/// Finishes a save whose journal is on disk, after checking every entry,
/// then removes left-over staged files and a half-written journal.
/// O(files in the folder).
fn recover(root: &Path) -> Result<(), FolderError> {
    if fs::symlink_metadata(root.join(JOURNAL)).is_ok() {
        let journal = read_journal(root)?;
        let mut dirs = BTreeSet::from([root.to_path_buf()]);
        for p in &journal.replace {
            let target = full_path(root, p);
            if fs::symlink_metadata(staged(&target)).is_ok() {
                fs::rename(staged(&target), &target).map_err(io_at(p))?;
                dirs.insert(target.parent().unwrap_or(root).to_path_buf());
            }
        }
        for p in &journal.remove {
            let target = full_path(root, p);
            remove_if_there(&target, p)?;
            if let Some(dir) = target.parent().filter(|d| d.is_dir()) {
                dirs.insert(dir.to_path_buf());
            }
        }
        sync_dirs(&dirs)?;
        fs::remove_file(root.join(JOURNAL)).map_err(io_at(JOURNAL))?;
    }
    if fs::symlink_metadata(root.join(JOURNAL_TMP)).is_ok_and(|m| m.is_file()) {
        remove_if_there(&root.join(JOURNAL_TMP), JOURNAL_TMP)?;
    }
    sweep(root, ".", 0)?;
    sync_dirs(&BTreeSet::from([root.to_path_buf()]))
}

/// Removes every regular `*.aegis-new` file at the root and under `shared/`
/// and `systems/`, `depth` levels below the root; links are skipped.
fn sweep(dir: &Path, rel: &str, depth: usize) -> Result<(), FolderError> {
    for entry in fs::read_dir(dir).map_err(io_at(rel))? {
        let entry = entry.map_err(io_at(rel))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{rel}/{name}");
        let meta = fs::symlink_metadata(entry.path()).map_err(io_at(&path))?;
        if meta.is_file() && name.ends_with(STAGED) {
            remove_if_there(&entry.path(), &path)?;
        } else if meta.is_dir()
            && !name.starts_with('.')
            && depth < MAX_DEPTH
            && (depth > 0 || name == "shared" || name == "systems")
        {
            sweep(&entry.path(), &path, depth + 1)?;
        }
    }
    Ok(())
}

fn take_lock(root: &Path) -> Result<File, FolderError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(LOCK))
        .map_err(io_at(LOCK))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(FolderError::Locked { path: shown(root) }),
        Err(TryLockError::Error(e)) => Err(io_at(LOCK)(e)),
    }
}

fn skipped(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(STAGED)
}

/// The files `open` reads: (path, full path, length), sorted by path. The
/// three caps are checked here, from the metadata, before any file is read.
fn collect(root: &Path) -> Result<Vec<(String, PathBuf, u64)>, FolderError> {
    let mut found = Found::default();
    for entry in fs::read_dir(root).map_err(io_at(&shown(root)))? {
        let entry = entry.map_err(io_at(&shown(root)))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if skipped(&name) {
            continue;
        }
        let full = entry.path();
        let tree = name == "shared" || name == "systems";
        let file = name.ends_with(".json") && name != "manifest.json";
        if !tree && !file {
            continue;
        }
        let meta = fs::symlink_metadata(&full).map_err(io_at(&name))?;
        if meta.file_type().is_symlink() {
            return Err(FolderError::Symlink { path: cut(&name) });
        }
        if tree && meta.is_dir() {
            walk(&full, &name, 1, &mut found)?;
        } else if file && meta.is_file() {
            found.push(name, full, meta.len())?;
        }
    }
    found.files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found.files)
}

#[derive(Default)]
struct Found {
    files: Vec<(String, PathBuf, u64)>,
    total: u64,
}

impl Found {
    fn push(&mut self, rel: String, full: PathBuf, len: u64) -> Result<(), FolderError> {
        if len > MAX_WORKSPACE_FILE_BYTES {
            return Err(FolderError::TooLarge {
                path: cut(&rel),
                max: MAX_WORKSPACE_FILE_BYTES,
            });
        }
        if self.files.len() >= MAX_WORKSPACE_FILES {
            return Err(FolderError::TooManyFiles {
                max: MAX_WORKSPACE_FILES,
            });
        }
        self.total = self.total.saturating_add(len);
        if self.total > MAX_PROJECT_FILE_BYTES {
            return Err(FolderError::TooBig {
                max: MAX_PROJECT_FILE_BYTES,
            });
        }
        self.files.push((rel, full, len));
        Ok(())
    }
}

/// Adds every regular file under `dir` (`depth` levels below the root).
fn walk(dir: &Path, rel: &str, depth: usize, found: &mut Found) -> Result<(), FolderError> {
    for entry in fs::read_dir(dir).map_err(io_at(rel))? {
        let entry = entry.map_err(io_at(rel))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if skipped(&name) {
            continue;
        }
        let path = format!("{rel}/{name}");
        let full = entry.path();
        let meta = fs::symlink_metadata(&full).map_err(io_at(&path))?;
        if meta.file_type().is_symlink() {
            return Err(FolderError::Symlink { path: cut(&path) });
        }
        if meta.is_dir() {
            if depth >= MAX_DEPTH {
                return Err(FolderError::TooDeep { path: cut(&path) });
            }
            walk(&full, &path, depth + 1, found)?;
        } else if meta.is_file() {
            found.push(path, full, meta.len())?;
        }
    }
    Ok(())
}

/// Reads at most `cap` bytes of `path`; a longer file (it may have grown
/// since [`collect`]) is refused.
fn read_capped(path: &Path, rel: &str, cap: u64) -> Result<Vec<u8>, FolderError> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(cap + 1).read_to_end(&mut bytes))
        .map_err(io_at(rel))?;
    if bytes.len() as u64 > cap {
        return Err(FolderError::TooLarge {
            path: cut(rel),
            max: cap,
        });
    }
    Ok(bytes)
}
