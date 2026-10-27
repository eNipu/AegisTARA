//! Single-project folder storage for standalone mode.
//!
//! The canonical `ops.jsonl` file is an `aegis-oplog` v1 stream. The
//! `ops.meta` sidecar carries the actor, timestamp, and audit links that are
//! not part of that wire format. A small atomically replaced manifest is the
//! commit record: log and sidecar bytes beyond its `last_seq` are uncommitted
//! and are discarded when the folder is opened.
//!
//! One [`FolderStore`] (including its clones) may own a folder at a time. An
//! OS file lock rejects other processes rather than allowing two writers to
//! corrupt the log. Locks are not held across `.await`; all blocking work in
//! the async [`ProjectStore`] methods runs in `spawn_blocking`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::audit::{ChainReport, ChainVerifier, Hash, genesis, link};
use super::{
    BaselineMeta, LoadedProject, ProjectMeta, ProjectStore, StoreFault, StoredOp, StoredSnapshot,
};
use crate::ids::ProjectId;

const MANIFEST: &str = "manifest.json";
const OPLOG: &str = "ops.jsonl";
const SIDECAR: &str = "ops.meta";
const LOCK: &str = ".aegis.lock";
const SNAPSHOTS: &str = "snapshots";
const FORMAT: &str = "aegis-folder-store";
const VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
/// Most ops in one batch of [`FolderStore::stream_ops`].
pub const STREAM_BATCH_OPS: usize = 256;
/// Most op bytes in one batch of [`FolderStore::stream_ops`] (a batch may
/// pass it by one op).
pub const STREAM_BATCH_BYTES: usize = 4 * 1024 * 1024;

fn backend(error: impl std::fmt::Display) -> StoreFault {
    StoreFault::Backend(error.to_string())
}

fn corrupt(error: impl std::fmt::Display) -> StoreFault {
    StoreFault::Corrupt(error.to_string())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskManifest {
    format: String,
    version: u32,
    project: Option<DiskProject>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskProject {
    id: String,
    name: String,
    methodology_key: String,
    methodology_version: String,
    created_ms: u64,
    last_seq: u64,
    last_hash: String,
    snapshot_seq: u64,
    snapshot_id: String,
    snapshot_hash: String,
    baselines: Vec<DiskBaseline>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskBaseline {
    id: String,
    name: String,
    seq: u64,
    created_by: String,
    created_ms: u64,
    snapshot_bytes: u64,
    snapshot_id: String,
    snapshot_hash: String,
}

#[derive(Serialize)]
struct LogHeader<'a> {
    format: &'static str,
    version: u32,
    methodology: LogMethodology<'a>,
}

#[derive(Serialize)]
struct LogMethodology<'a> {
    key: &'a str,
    version: &'a str,
}

/// Byte lengths of the committed part of the log and the sidecar. Anything
/// past them is the debris of a failed or interrupted append.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Tail {
    log: u64,
    sidecar: u64,
}

struct Inner {
    root: PathBuf,
    gate: Mutex<Tail>,
    _lock: File,
}

impl Inner {
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn lock(&self) -> MutexGuard<'_, Tail> {
        self.gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A [`ProjectStore`] over one selected project folder.
///
/// Opening an empty folder creates no project. At most one project exists in
/// the folder; `create` after `delete` starts a new one. Clones share the same
/// writer lock. A second `open` of the same folder fails while the first store
/// is alive, including from another process.
///
/// Reads that walk the history (`load`, `recent_ops`, `verify_chain`,
/// `active_editors`) stream the log once: time O(history), space O(result).
/// An append is O(batch). `ponytail:` a byte-offset index would make
/// `recent_ops` O(limit); add it if a standalone log ever grows past what a
/// linear scan answers quickly.
#[derive(Clone)]
pub struct FolderStore {
    inner: Arc<Inner>,
}

impl FolderStore {
    /// Opens or creates a project folder synchronously.
    ///
    /// This constructor performs blocking filesystem I/O. Async callers
    /// should invoke it through `tokio::task::spawn_blocking`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreFault::Backend`] for filesystem errors or if the folder
    /// is already open, and [`StoreFault::Corrupt`] for invalid committed
    /// state. Uncommitted log tails and unreferenced files are discarded
    /// during recovery.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreFault> {
        std::fs::create_dir_all(path.as_ref()).map_err(backend)?;
        let root = path.as_ref().canonicalize().map_err(backend)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(LOCK))
            .map_err(backend)?;
        lock.try_lock().map_err(|error| {
            backend(format!(
                "project folder is already open or cannot be locked: {error}"
            ))
        })?;
        let inner = Arc::new(Inner {
            root,
            gate: Mutex::new(Tail::default()),
            _lock: lock,
        });
        {
            let mut tail = inner.lock();
            *tail = match read_state(&inner.root)? {
                Some(project) => recover(&inner.root, &project)?,
                None => {
                    clean_project_files(&inner.root)?;
                    Tail::default()
                }
            };
        }
        Ok(Self { inner })
    }

    /// Opens a folder that already holds an AegisTARA project, without
    /// creating anything for a path that does not (recovery of an interrupted
    /// write still applies, as in [`open`](Self::open)).
    ///
    /// # Errors
    ///
    /// [`StoreFault::Backend`] if `path` has no project manifest, otherwise
    /// as [`open`](Self::open).
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self, StoreFault> {
        if !path.as_ref().join(MANIFEST).is_file() {
            return Err(backend(format!(
                "{} is not an AegisTARA project folder",
                path.as_ref().display()
            )));
        }
        Self::open(path)
    }

    /// Streams every committed op of project `id`, oldest first, in batches
    /// of at most [`STREAM_BATCH_OPS`] ops or [`STREAM_BATCH_BYTES`] bytes.
    ///
    /// The channel holds two batches, so memory stays O(batch) however long
    /// the history is. A failure arrives as the last item. Dropping the
    /// receiver stops the read. The folder is busy until then: other calls on
    /// this store wait.
    #[must_use]
    pub fn stream_ops(&self, id: ProjectId) -> mpsc::Receiver<Result<Vec<StoredOp>, StoreFault>> {
        let (tx, rx) = mpsc::channel(2);
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let _tail = inner.lock();
            let gone = || backend("the reader of the op stream went away");
            let result = current(&inner.root, id).and_then(|project| {
                let (mut batch, mut bytes) = (Vec::new(), 0);
                scan(&inner.root, &project, |op, _, _| {
                    bytes += op.op.len();
                    batch.push(op);
                    if batch.len() >= STREAM_BATCH_OPS || bytes >= STREAM_BATCH_BYTES {
                        bytes = 0;
                        tx.blocking_send(Ok(std::mem::take(&mut batch)))
                            .map_err(|_| gone())?;
                    }
                    Ok(())
                })?;
                if batch.is_empty() {
                    return Ok(());
                }
                tx.blocking_send(Ok(batch)).map_err(|_| gone())
            });
            if let Err(error) = result {
                let _ = tx.blocking_send(Err(error));
            }
        });
        rx
    }

    async fn blocking<R, F>(&self, work: F) -> Result<R, StoreFault>
    where
        R: Send + 'static,
        F: FnOnce(&Inner, &mut Tail) -> Result<R, StoreFault> + Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut tail = inner.lock();
            work(&inner, &mut tail)
        })
        .await
        .map_err(|error| backend(format!("folder store task failed: {error}")))?
    }
}

fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, StoreFault> {
    let file = File::open(path).map_err(backend)?;
    let length = file.metadata().map_err(backend)?.len();
    if length > cap {
        return Err(corrupt(format!("{} exceeds its size cap", path.display())));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(backend)?;
    if bytes.len() as u64 > cap {
        return Err(corrupt(format!("{} exceeds its size cap", path.display())));
    }
    Ok(bytes)
}

fn read_state(root: &Path) -> Result<Option<DiskProject>, StoreFault> {
    let path = root.join(MANIFEST);
    let bytes = match read_capped(&path, MAX_MANIFEST_BYTES) {
        Ok(bytes) => bytes,
        Err(StoreFault::Backend(_)) if !path.exists() => return Ok(None),
        Err(error) => return Err(error),
    };
    let manifest: DiskManifest = serde_json::from_slice(&bytes).map_err(corrupt)?;
    if manifest.format != FORMAT || manifest.version != VERSION {
        return Err(corrupt("unsupported folder-store manifest"));
    }
    if let Some(project) = &manifest.project {
        validate_project(project)?;
    }
    Ok(manifest.project)
}

fn validate_project(project: &DiskProject) -> Result<(), StoreFault> {
    ProjectId::from_str(&project.id).map_err(corrupt)?;
    decode_hash(&project.last_hash)?;
    decode_hash(&project.snapshot_hash)?;
    Uuid::parse_str(&project.snapshot_id).map_err(corrupt)?;
    if project.snapshot_seq > project.last_seq {
        return Err(corrupt("snapshot runs beyond the op history"));
    }
    let mut ids = BTreeSet::new();
    for baseline in &project.baselines {
        if !ids.insert(&baseline.id) || Uuid::parse_str(&baseline.id).is_err() {
            return Err(corrupt("invalid or duplicate baseline id"));
        }
        if baseline.seq > project.last_seq {
            return Err(corrupt("baseline runs beyond the op history"));
        }
        Uuid::parse_str(&baseline.snapshot_id).map_err(corrupt)?;
        decode_hash(&baseline.snapshot_hash)?;
    }
    Ok(())
}

fn decode_hash(text: &str) -> Result<Hash, StoreFault> {
    if text.len() != 64 {
        return Err(corrupt("hash is not 32 bytes"));
    }
    let mut hash = [0; 32];
    for (index, byte) in hash.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).map_err(corrupt)?;
    }
    Ok(hash)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn meta_of(project: &DiskProject) -> Result<ProjectMeta, StoreFault> {
    Ok(ProjectMeta {
        id: ProjectId::from_str(&project.id).map_err(corrupt)?,
        name: project.name.clone(),
        methodology_key: project.methodology_key.clone(),
        methodology_version: project.methodology_version.clone(),
        created_ms: project.created_ms,
    })
}

fn snapshot_path(root: &Path, id: &str) -> PathBuf {
    root.join(SNAPSHOTS).join(format!("{id}.aegis"))
}

fn read_snapshot(root: &Path, id: &str, expected_hash: &str) -> Result<Vec<u8>, StoreFault> {
    let bytes = read_capped(
        &snapshot_path(root, id),
        aegis_model::MAX_PROJECT_FILE_BYTES,
    )?;
    if digest(&bytes) != expected_hash {
        return Err(corrupt("snapshot digest does not match the manifest"));
    }
    Ok(bytes)
}

fn sync_dir(path: &Path) -> Result<(), StoreFault> {
    File::open(path)
        .map_err(backend)?
        .sync_all()
        .map_err(backend)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), StoreFault> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(backend)?;
    file.write_all(bytes).map_err(backend)?;
    file.sync_all().map_err(backend)
}

fn commit_state(root: &Path, project: Option<&DiskProject>) -> Result<(), StoreFault> {
    let manifest = DiskManifest {
        format: FORMAT.into(),
        version: VERSION,
        project: project.cloned(),
    };
    let bytes = serde_json::to_vec(&manifest).map_err(backend)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(backend("folder-store manifest exceeds its size cap"));
    }
    let temporary = root.join(format!(".manifest-{}.tmp", Uuid::new_v4()));
    write_new(&temporary, &bytes)?;
    std::fs::rename(&temporary, root.join(MANIFEST)).map_err(backend)?;
    sync_dir(root)
}

fn remove_if_exists(path: &Path) -> Result<(), StoreFault> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(backend(error)),
    }
}

fn clean_project_files(root: &Path) -> Result<(), StoreFault> {
    remove_if_exists(&root.join(OPLOG))?;
    remove_if_exists(&root.join(SIDECAR))?;
    match std::fs::remove_dir_all(root.join(SNAPSHOTS)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(backend(error)),
    }
}

fn initial_log(project: &DiskProject) -> Result<Vec<u8>, StoreFault> {
    let mut bytes = serde_json::to_vec(&LogHeader {
        format: "aegis-oplog",
        version: 1,
        methodology: LogMethodology {
            key: &project.methodology_key,
            version: &project.methodology_version,
        },
    })
    .map_err(backend)?;
    bytes.push(b'\n');
    Ok(bytes)
}

const SIDECAR_MAGIC: &[u8; 8] = b"AEGISOP1";
const MAX_ACTOR_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadHeader {
    format: String,
    version: u32,
    methodology: ReadMethodology,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadMethodology {
    key: String,
    version: String,
}

fn read_exact<const N: usize>(reader: &mut impl Read) -> Result<[u8; N], StoreFault> {
    let mut bytes = [0; N];
    reader.read_exact(&mut bytes).map_err(corrupt)?;
    Ok(bytes)
}

fn read_log_line(reader: &mut BufReader<File>) -> Result<Option<Vec<u8>>, StoreFault> {
    let mut bytes = Vec::new();
    let read = reader
        .by_ref()
        .take(aegis_model::MAX_OP_RECORD_BYTES as u64 + 2)
        .read_until(b'\n', &mut bytes)
        .map_err(backend)?;
    if read == 0 {
        return Ok(None);
    }
    if bytes.len() > aegis_model::MAX_OP_RECORD_BYTES + 1 || bytes.last() != Some(&b'\n') {
        return Err(corrupt("truncated or oversized op-log line"));
    }
    bytes.pop();
    if bytes.contains(&b'\r') {
        return Err(corrupt("carriage return in op log"));
    }
    Ok(Some(bytes))
}

fn parse_op_line(line: &[u8]) -> Result<(u64, Vec<u8>), StoreFault> {
    let rest = line
        .strip_prefix(b"{\"seq\":")
        .ok_or_else(|| corrupt("invalid op-log record"))?;
    let split = rest
        .windows(6)
        .position(|window| window == b",\"op\":")
        .ok_or_else(|| corrupt("invalid op-log record"))?;
    let seq_text = std::str::from_utf8(&rest[..split]).map_err(corrupt)?;
    if seq_text.is_empty()
        || !seq_text.bytes().all(|byte| byte.is_ascii_digit())
        || (seq_text.len() > 1 && seq_text.starts_with('0'))
    {
        return Err(corrupt("invalid op-log sequence"));
    }
    let seq = seq_text.parse::<u64>().map_err(corrupt)?;
    let body = &rest[split + 6..];
    let op = body
        .strip_suffix(b"}")
        .ok_or_else(|| corrupt("invalid op-log record"))?;
    let _: serde_json::Value = serde_json::from_slice(op).map_err(corrupt)?;
    Ok((seq, op.to_vec()))
}

struct HistoryReader {
    log: BufReader<File>,
    sidecar: BufReader<File>,
    log_position: u64,
    sidecar_position: u64,
}

impl HistoryReader {
    fn open(root: &Path, project: &DiskProject) -> Result<Self, StoreFault> {
        let mut log = BufReader::new(File::open(root.join(OPLOG)).map_err(backend)?);
        let header_line =
            read_log_line(&mut log)?.ok_or_else(|| corrupt("missing op-log header"))?;
        let header: ReadHeader = serde_json::from_slice(&header_line).map_err(corrupt)?;
        if header.format != "aegis-oplog"
            || header.version != 1
            || header.methodology.key != project.methodology_key
            || header.methodology.version != project.methodology_version
        {
            return Err(corrupt("op-log header does not match the project"));
        }
        let mut sidecar = BufReader::new(File::open(root.join(SIDECAR)).map_err(backend)?);
        if read_exact::<8>(&mut sidecar)? != *SIDECAR_MAGIC {
            return Err(corrupt("invalid op sidecar header"));
        }
        Ok(Self {
            log,
            sidecar,
            log_position: header_line.len() as u64 + 1,
            sidecar_position: SIDECAR_MAGIC.len() as u64,
        })
    }

    fn next(&mut self) -> Result<Option<(StoredOp, Hash, Hash)>, StoreFault> {
        let Some(line) = read_log_line(&mut self.log)? else {
            return Ok(None);
        };
        self.log_position = self
            .log_position
            .checked_add(line.len() as u64 + 1)
            .ok_or_else(|| corrupt("op log is too large"))?;
        let (seq, op_bytes) = parse_op_line(&line)?;
        let fixed = read_exact::<84>(&mut self.sidecar)?;
        let sidecar_seq = u64::from_be_bytes(fixed[..8].try_into().expect("fixed slice"));
        let timestamp_ms = u64::from_be_bytes(fixed[8..16].try_into().expect("fixed slice"));
        let actor_len = u32::from_be_bytes(fixed[16..20].try_into().expect("fixed slice")) as usize;
        if actor_len > MAX_ACTOR_BYTES || sidecar_seq != seq {
            return Err(corrupt("op sidecar does not match the op log"));
        }
        let mut actor = vec![0; actor_len];
        self.sidecar.read_exact(&mut actor).map_err(corrupt)?;
        self.sidecar_position = self
            .sidecar_position
            .checked_add(84 + actor_len as u64)
            .ok_or_else(|| corrupt("op sidecar is too large"))?;
        let actor = String::from_utf8(actor).map_err(corrupt)?;
        let prev = fixed[20..52].try_into().expect("fixed slice");
        let hash = fixed[52..84].try_into().expect("fixed slice");
        Ok(Some((
            StoredOp {
                seq,
                actor,
                timestamp_ms,
                op: op_bytes,
            },
            prev,
            hash,
        )))
    }
}

/// Streams the committed history in order, one op at a time, and returns the
/// byte lengths that make it up. Space O(one op).
fn scan(
    root: &Path,
    project: &DiskProject,
    mut visit: impl FnMut(StoredOp, Hash, Hash) -> Result<(), StoreFault>,
) -> Result<Tail, StoreFault> {
    let mut reader = HistoryReader::open(root, project)?;
    for expected in 1..=project.last_seq {
        let (op, prev, hash) = reader
            .next()?
            .ok_or_else(|| corrupt("committed op history is truncated"))?;
        if op.seq != expected {
            return Err(corrupt(format!(
                "op history gap: expected {expected}, found {}",
                op.seq
            )));
        }
        visit(op, prev, hash)?;
    }
    Ok(Tail {
        log: reader.log_position,
        sidecar: reader.sidecar_position,
    })
}

/// Cuts `path` back to `committed` bytes. Shorter than that means committed
/// data is gone, which is damage, not a crash.
fn trim(path: &Path, committed: u64) -> Result<(), StoreFault> {
    let file = OpenOptions::new().write(true).open(path).map_err(backend)?;
    let length = file.metadata().map_err(backend)?.len();
    if length < committed {
        return Err(corrupt("committed data is truncated"));
    }
    if length > committed {
        file.set_len(committed).map_err(backend)?;
        file.sync_all().map_err(backend)?;
    }
    Ok(())
}

fn trim_tail(root: &Path, tail: Tail) -> Result<(), StoreFault> {
    trim(&root.join(OPLOG), tail.log)?;
    trim(&root.join(SIDECAR), tail.sidecar)
}

/// Brings a folder to its last committed state: reads the committed history
/// (so damage is found at open), drops bytes after it, and deletes files the
/// manifest does not name (an interrupted snapshot, baseline or manifest
/// write). The manifest rename is the commit point, so none of them is state.
fn recover(root: &Path, project: &DiskProject) -> Result<Tail, StoreFault> {
    let tail = scan(root, project, |_, _, _| Ok(()))?;
    trim_tail(root, tail)?;
    let mut named: BTreeSet<String> = project
        .baselines
        .iter()
        .map(|b| format!("{}.aegis", b.snapshot_id))
        .collect();
    named.insert(format!("{}.aegis", project.snapshot_id));
    for entry in std::fs::read_dir(root.join(SNAPSHOTS)).map_err(backend)? {
        let entry = entry.map_err(backend)?;
        if !named.contains(entry.file_name().to_string_lossy().as_ref()) {
            remove_if_exists(&entry.path())?;
        }
    }
    for entry in std::fs::read_dir(root).map_err(backend)? {
        let entry = entry.map_err(backend)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".manifest-") && name.ends_with(".tmp") {
            remove_if_exists(&entry.path())?;
        }
    }
    Ok(tail)
}

fn encode_sidecar(op: &StoredOp, prev: &Hash, hash: &Hash) -> Result<Vec<u8>, StoreFault> {
    if op.actor.len() > MAX_ACTOR_BYTES {
        return Err(backend("actor exceeds the folder-store sidecar cap"));
    }
    let actor_len = u32::try_from(op.actor.len()).map_err(backend)?;
    let mut bytes = Vec::with_capacity(84 + op.actor.len());
    bytes.extend_from_slice(&op.seq.to_be_bytes());
    bytes.extend_from_slice(&op.timestamp_ms.to_be_bytes());
    bytes.extend_from_slice(&actor_len.to_be_bytes());
    bytes.extend_from_slice(prev);
    bytes.extend_from_slice(hash);
    bytes.extend_from_slice(op.actor.as_bytes());
    Ok(bytes)
}

fn encode_record(op: &StoredOp) -> Result<Vec<u8>, StoreFault> {
    if op.op.len() > aegis_model::MAX_OP_RECORD_BYTES {
        return Err(backend("op exceeds the aegis-oplog v1 record cap"));
    }
    let _: serde_json::Value = serde_json::from_slice(&op.op).map_err(backend)?;
    let mut bytes = format!("{{\"seq\":{},\"op\":", op.seq).into_bytes();
    bytes.extend_from_slice(&op.op);
    bytes.extend_from_slice(b"}\n");
    if bytes.len() - 1 > aegis_model::MAX_OP_RECORD_BYTES {
        return Err(backend("op exceeds the aegis-oplog v1 record cap"));
    }
    Ok(bytes)
}

/// The project in the folder, if it is `id`.
fn current(root: &Path, id: ProjectId) -> Result<DiskProject, StoreFault> {
    match read_state(root)? {
        Some(project) if project.id == id.to_string() => Ok(project),
        _ => Err(StoreFault::Missing),
    }
}

/// Writes a snapshot file under a fresh name and returns `(file id, digest)`.
/// The file belongs to no state until a manifest names it.
fn write_snapshot_file(root: &Path, bytes: &[u8]) -> Result<(String, String), StoreFault> {
    if bytes.len() as u64 > aegis_model::MAX_PROJECT_FILE_BYTES {
        return Err(backend("snapshot exceeds its size cap"));
    }
    std::fs::create_dir_all(root.join(SNAPSHOTS)).map_err(backend)?;
    let id = Uuid::new_v4().to_string();
    write_new(&snapshot_path(root, &id), bytes)?;
    sync_dir(&root.join(SNAPSHOTS))?;
    Ok((id, digest(bytes)))
}

/// Commits `project`; if that fails, removes the file written for it.
fn commit_with_file(root: &Path, project: &DiskProject, new_file: &str) -> Result<(), StoreFault> {
    commit_state(root, Some(project)).inspect_err(|_| {
        let _ = remove_if_exists(&snapshot_path(root, new_file));
    })
}

impl ProjectStore for FolderStore {
    async fn create(&self, meta: &ProjectMeta, snapshot: &[u8]) -> Result<(), StoreFault> {
        if snapshot.len() as u64 > aegis_model::MAX_PROJECT_FILE_BYTES {
            return Err(backend("snapshot exceeds its size cap"));
        }
        let meta = meta.clone();
        let snapshot = snapshot.to_vec();
        self.blocking(move |inner, tail| {
            if read_state(&inner.root)?.is_some() {
                return Err(StoreFault::Exists);
            }
            clean_project_files(&inner.root)?;
            let (snapshot_id, snapshot_hash) = write_snapshot_file(&inner.root, &snapshot)?;
            let project = DiskProject {
                id: meta.id.to_string(),
                name: meta.name,
                methodology_key: meta.methodology_key,
                methodology_version: meta.methodology_version,
                created_ms: meta.created_ms,
                last_seq: 0,
                last_hash: hex(&genesis(meta.id)),
                snapshot_seq: 0,
                snapshot_id,
                snapshot_hash,
                baselines: Vec::new(),
            };
            let header = initial_log(&project)?;
            write_new(&inner.path(OPLOG), &header)?;
            write_new(&inner.path(SIDECAR), SIDECAR_MAGIC)?;
            sync_dir(&inner.root)?;
            commit_state(&inner.root, Some(&project))?;
            *tail = Tail {
                log: header.len() as u64,
                sidecar: SIDECAR_MAGIC.len() as u64,
            };
            Ok(())
        })
        .await
    }

    async fn list(&self, limit: usize) -> Result<Vec<ProjectMeta>, StoreFault> {
        self.blocking(move |inner, _| match read_state(&inner.root)? {
            Some(project) if limit > 0 => Ok(vec![meta_of(&project)?]),
            _ => Ok(Vec::new()),
        })
        .await
    }

    async fn count(&self) -> Result<usize, StoreFault> {
        self.blocking(|inner, _| Ok(usize::from(read_state(&inner.root)?.is_some())))
            .await
    }

    async fn load(&self, id: ProjectId) -> Result<Option<LoadedProject>, StoreFault> {
        self.blocking(move |inner, _| {
            let project = match current(&inner.root, id) {
                Ok(project) => project,
                Err(StoreFault::Missing) => return Ok(None),
                Err(error) => return Err(error),
            };
            let snapshot = StoredSnapshot {
                seq: project.snapshot_seq,
                bytes: read_snapshot(&inner.root, &project.snapshot_id, &project.snapshot_hash)?,
            };
            let mut ops = Vec::new();
            scan(&inner.root, &project, |op, _, _| {
                if op.seq > project.snapshot_seq {
                    ops.push(op);
                }
                Ok(())
            })?;
            Ok(Some(LoadedProject {
                meta: meta_of(&project)?,
                snapshot,
                ops,
            }))
        })
        .await
    }

    async fn append(&self, id: ProjectId, ops: &[StoredOp]) -> Result<(), StoreFault> {
        let ops = ops.to_vec();
        self.blocking(move |inner, tail| {
            let mut project = current(&inner.root, id)?;
            let mut expected = project
                .last_seq
                .checked_add(1)
                .ok_or_else(|| backend("sequence overflow"))?;
            for op in &ops {
                if op.seq != expected {
                    return Err(StoreFault::Sequence {
                        expected,
                        found: op.seq,
                    });
                }
                expected = expected
                    .checked_add(1)
                    .ok_or_else(|| backend("sequence overflow"))?;
            }
            if ops.is_empty() {
                return Ok(());
            }
            let mut log_bytes = Vec::new();
            let mut sidecar_bytes = Vec::new();
            let mut prev = decode_hash(&project.last_hash)?;
            for op in &ops {
                let hash = link(&prev, op);
                log_bytes.extend_from_slice(&encode_record(op)?);
                sidecar_bytes.extend_from_slice(&encode_sidecar(op, &prev, &hash)?);
                prev = hash;
            }
            project.last_seq = expected - 1;
            project.last_hash = hex(&prev);

            // Drop what an earlier failed append left behind, then extend
            // both files; the manifest makes the new ops visible.
            trim_tail(&inner.root, *tail)?;
            let written = (|| {
                let mut log = OpenOptions::new()
                    .append(true)
                    .open(inner.path(OPLOG))
                    .map_err(backend)?;
                log.write_all(&log_bytes).map_err(backend)?;
                log.sync_all().map_err(backend)?;
                let mut sidecar = OpenOptions::new()
                    .append(true)
                    .open(inner.path(SIDECAR))
                    .map_err(backend)?;
                sidecar.write_all(&sidecar_bytes).map_err(backend)?;
                sidecar.sync_all().map_err(backend)?;
                commit_state(&inner.root, Some(&project))
            })();
            match written {
                Ok(()) => {
                    tail.log += log_bytes.len() as u64;
                    tail.sidecar += sidecar_bytes.len() as u64;
                    Ok(())
                }
                Err(error) => {
                    let _ = trim_tail(&inner.root, *tail);
                    Err(error)
                }
            }
        })
        .await
    }

    async fn verify_chain(&self, id: ProjectId) -> Result<ChainReport, StoreFault> {
        self.blocking(move |inner, _| {
            let project = current(&inner.root, id)?;
            let mut verifier = ChainVerifier::new(id);
            scan(&inner.root, &project, |op, prev, hash| {
                verifier.push(&op, &prev, &hash);
                Ok(())
            })?;
            Ok(verifier.finish(&decode_hash(&project.last_hash)?))
        })
        .await
    }

    async fn save_snapshot(
        &self,
        id: ProjectId,
        snapshot: &StoredSnapshot,
    ) -> Result<(), StoreFault> {
        let snapshot = snapshot.clone();
        self.blocking(move |inner, _| {
            let mut project = current(&inner.root, id)?;
            if snapshot.seq > project.last_seq || snapshot.seq < project.snapshot_seq {
                return Err(StoreFault::Sequence {
                    expected: project.last_seq,
                    found: snapshot.seq,
                });
            }
            let (file, hash) = write_snapshot_file(&inner.root, &snapshot.bytes)?;
            let old = std::mem::replace(&mut project.snapshot_id, file.clone());
            project.snapshot_hash = hash;
            project.snapshot_seq = snapshot.seq;
            commit_with_file(&inner.root, &project, &file)?;
            // The old file is unnamed now; a failure leaves an orphan that
            // the next open sweeps.
            let _ = remove_if_exists(&snapshot_path(&inner.root, &old));
            Ok(())
        })
        .await
    }

    async fn rename(&self, id: ProjectId, name: &str) -> Result<(), StoreFault> {
        let name = name.to_owned();
        self.blocking(move |inner, _| {
            let mut project = current(&inner.root, id)?;
            project.name = name;
            commit_state(&inner.root, Some(&project))
        })
        .await
    }

    async fn delete(&self, id: ProjectId) -> Result<(), StoreFault> {
        self.blocking(move |inner, tail| {
            current(&inner.root, id)?;
            // Once the manifest names no project the history is gone; the
            // files are then removed, and a crash before that is swept at open.
            commit_state(&inner.root, None)?;
            *tail = Tail::default();
            clean_project_files(&inner.root)
        })
        .await
    }

    async fn recent_ops(&self, id: ProjectId, limit: usize) -> Result<Vec<StoredOp>, StoreFault> {
        self.blocking(move |inner, _| {
            let project = current(&inner.root, id)?;
            let room = limit.min(usize::try_from(project.last_seq).unwrap_or(usize::MAX));
            let mut newest = VecDeque::with_capacity(room);
            if room > 0 {
                scan(&inner.root, &project, |op, _, _| {
                    if newest.len() == room {
                        newest.pop_front();
                    }
                    newest.push_back(op);
                    Ok(())
                })?;
            }
            Ok(newest.into_iter().rev().collect())
        })
        .await
    }

    async fn active_editors(&self, since_ms: u64) -> Result<Vec<(String, u64)>, StoreFault> {
        self.blocking(move |inner, _| {
            let Some(project) = read_state(&inner.root)? else {
                return Ok(Vec::new());
            };
            let mut latest = BTreeMap::<String, u64>::new();
            scan(&inner.root, &project, |op, _, _| {
                if op.timestamp_ms >= since_ms {
                    let timestamp = latest.entry(op.actor).or_default();
                    *timestamp = (*timestamp).max(op.timestamp_ms);
                }
                Ok(())
            })?;
            Ok(latest.into_iter().collect())
        })
        .await
    }

    async fn create_baseline(
        &self,
        meta: &BaselineMeta,
        snapshot: &[u8],
    ) -> Result<(), StoreFault> {
        let meta = meta.clone();
        let snapshot = snapshot.to_vec();
        self.blocking(move |inner, _| {
            let mut project = current(&inner.root, meta.project)?;
            let wanted = meta.id.to_string();
            if project.baselines.iter().any(|b| b.id == wanted) {
                return Err(StoreFault::Exists);
            }
            if meta.seq > project.last_seq {
                return Err(StoreFault::Sequence {
                    expected: project.last_seq,
                    found: meta.seq,
                });
            }
            let (file, hash) = write_snapshot_file(&inner.root, &snapshot)?;
            project.baselines.push(DiskBaseline {
                id: wanted,
                name: meta.name,
                seq: meta.seq,
                created_by: meta.created_by,
                created_ms: meta.created_ms,
                snapshot_bytes: meta.snapshot_bytes,
                snapshot_id: file.clone(),
                snapshot_hash: hash,
            });
            commit_with_file(&inner.root, &project, &file)
        })
        .await
    }

    async fn list_baselines(&self, project: ProjectId) -> Result<Vec<BaselineMeta>, StoreFault> {
        self.blocking(move |inner, _| {
            let disk = current(&inner.root, project)?;
            disk.baselines
                .iter()
                .map(|b| baseline_meta(project, b))
                .collect()
        })
        .await
    }

    async fn load_baseline(
        &self,
        project: ProjectId,
        id: Uuid,
    ) -> Result<Option<(BaselineMeta, Vec<u8>)>, StoreFault> {
        self.blocking(move |inner, _| {
            let disk = match current(&inner.root, project) {
                Ok(disk) => disk,
                Err(StoreFault::Missing) => return Ok(None),
                Err(error) => return Err(error),
            };
            let wanted = id.to_string();
            let Some(found) = disk.baselines.iter().find(|b| b.id == wanted) else {
                return Ok(None);
            };
            let bytes = read_snapshot(&inner.root, &found.snapshot_id, &found.snapshot_hash)?;
            Ok(Some((baseline_meta(project, found)?, bytes)))
        })
        .await
    }
}

fn baseline_meta(project: ProjectId, b: &DiskBaseline) -> Result<BaselineMeta, StoreFault> {
    Ok(BaselineMeta {
        id: Uuid::parse_str(&b.id).map_err(corrupt)?,
        project,
        name: b.name.clone(),
        seq: b.seq,
        created_by: b.created_by.clone(),
        created_ms: b.created_ms,
        snapshot_bytes: b.snapshot_bytes,
    })
}
