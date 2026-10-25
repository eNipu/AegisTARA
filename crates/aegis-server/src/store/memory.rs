//! In-memory [`ProjectStore`]: tests, development, and the reference
//! behaviour every backend is compared against.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::{LoadedProject, ProjectMeta, ProjectStore, StoreFault, StoredOp, StoredSnapshot};
use crate::ids::ProjectId;

#[derive(Debug)]
struct Entry {
    meta: ProjectMeta,
    snapshot: StoredSnapshot,
    /// Every op ever accepted, `ops[i].seq == i + 1`.
    ops: Vec<StoredOp>,
}

/// A store that keeps everything in process memory.
///
/// Locks are short and never held across an await (C2): each method does
/// its work synchronously inside the guard.
#[derive(Debug, Default)]
pub struct MemoryStore {
    projects: Mutex<BTreeMap<ProjectId, Entry>>,
}

impl MemoryStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut BTreeMap<ProjectId, Entry>) -> R) -> R {
        // A poisoned lock means a panic while holding it; the map is still
        // structurally valid, and the panic is already logged.
        let mut guard = self
            .projects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    /// The full op history of a project, for audit-style assertions in tests.
    #[must_use]
    pub fn history(&self, id: ProjectId) -> Vec<StoredOp> {
        self.with(|m| m.get(&id).map(|e| e.ops.clone()).unwrap_or_default())
    }
}

impl ProjectStore for MemoryStore {
    async fn create(&self, meta: &ProjectMeta, snapshot: &[u8]) -> Result<(), StoreFault> {
        self.with(|m| {
            if m.contains_key(&meta.id) {
                return Err(StoreFault::Exists);
            }
            m.insert(
                meta.id,
                Entry {
                    meta: meta.clone(),
                    snapshot: StoredSnapshot {
                        seq: 0,
                        bytes: snapshot.to_vec(),
                    },
                    ops: Vec::new(),
                },
            );
            Ok(())
        })
    }

    async fn list(&self, limit: usize) -> Result<Vec<ProjectMeta>, StoreFault> {
        Ok(self.with(|m| m.values().take(limit).map(|e| e.meta.clone()).collect()))
    }

    async fn count(&self) -> Result<usize, StoreFault> {
        Ok(self.with(|m| m.len()))
    }

    async fn load(&self, id: ProjectId) -> Result<Option<LoadedProject>, StoreFault> {
        Ok(self.with(|m| {
            m.get(&id).map(|e| {
                let skip = usize::try_from(e.snapshot.seq).unwrap_or(usize::MAX);
                LoadedProject {
                    meta: e.meta.clone(),
                    snapshot: e.snapshot.clone(),
                    ops: e.ops.iter().skip(skip).cloned().collect(),
                }
            })
        }))
    }

    async fn append(&self, id: ProjectId, ops: &[StoredOp]) -> Result<(), StoreFault> {
        self.with(|m| {
            let entry = m.get_mut(&id).ok_or(StoreFault::Missing)?;
            let first = entry.ops.len() as u64 + 1;
            for (expected, op) in (first..).zip(ops) {
                if op.seq != expected {
                    return Err(StoreFault::Sequence {
                        expected,
                        found: op.seq,
                    });
                }
            }
            entry.ops.extend_from_slice(ops);
            Ok(())
        })
    }

    async fn save_snapshot(
        &self,
        id: ProjectId,
        snapshot: &StoredSnapshot,
    ) -> Result<(), StoreFault> {
        self.with(|m| {
            let entry = m.get_mut(&id).ok_or(StoreFault::Missing)?;
            let known = entry.ops.len() as u64;
            if snapshot.seq > known || snapshot.seq < entry.snapshot.seq {
                return Err(StoreFault::Sequence {
                    expected: known,
                    found: snapshot.seq,
                });
            }
            entry.snapshot = snapshot.clone();
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> ProjectMeta {
        ProjectMeta {
            id: ProjectId::new_v7(),
            name: "p".into(),
            methodology_key: "k".into(),
            methodology_version: "1".into(),
            created_ms: 1,
        }
    }

    fn op(seq: u64) -> StoredOp {
        StoredOp {
            seq,
            actor: "a".into(),
            timestamp_ms: seq,
            op: vec![],
        }
    }

    #[tokio::test]
    async fn create_load_append_snapshot() {
        let store = MemoryStore::new();
        let m = meta();
        store.create(&m, b"s0").await.expect("create");
        assert!(matches!(
            store.create(&m, b"s0").await,
            Err(StoreFault::Exists)
        ));

        store
            .append(m.id, &[op(1), op(2), op(3)])
            .await
            .expect("append");
        let loaded = store.load(m.id).await.expect("load").expect("exists");
        assert_eq!(loaded.snapshot.seq, 0);
        assert_eq!(loaded.ops.len(), 3);

        store
            .save_snapshot(
                m.id,
                &StoredSnapshot {
                    seq: 2,
                    bytes: b"s2".to_vec(),
                },
            )
            .await
            .expect("snapshot");
        let loaded = store.load(m.id).await.expect("load").expect("exists");
        assert_eq!(loaded.snapshot.seq, 2);
        assert_eq!(
            loaded.ops.iter().map(|o| o.seq).collect::<Vec<_>>(),
            vec![3]
        );
        assert_eq!(store.history(m.id).len(), 3);
    }

    #[tokio::test]
    async fn append_is_all_or_nothing_and_gap_free() {
        let store = MemoryStore::new();
        let m = meta();
        store.create(&m, b"s0").await.expect("create");
        let gap = store.append(m.id, &[op(1), op(3)]).await;
        assert!(matches!(
            gap,
            Err(StoreFault::Sequence {
                expected: 2,
                found: 3
            })
        ));
        assert!(store.history(m.id).is_empty());
        store.append(m.id, &[op(1)]).await.expect("append");
        let dup = store.append(m.id, &[op(1)]).await;
        assert!(matches!(
            dup,
            Err(StoreFault::Sequence {
                expected: 2,
                found: 1
            })
        ));
    }

    #[tokio::test]
    async fn snapshot_cannot_run_ahead_or_back() {
        let store = MemoryStore::new();
        let m = meta();
        store.create(&m, b"s0").await.expect("create");
        store.append(m.id, &[op(1)]).await.expect("append");
        let ahead = StoredSnapshot {
            seq: 2,
            bytes: vec![],
        };
        assert!(store.save_snapshot(m.id, &ahead).await.is_err());
        store
            .save_snapshot(
                m.id,
                &StoredSnapshot {
                    seq: 1,
                    bytes: vec![],
                },
            )
            .await
            .expect("ok");
        let back = StoredSnapshot {
            seq: 0,
            bytes: vec![],
        };
        assert!(store.save_snapshot(m.id, &back).await.is_err());
    }

    #[tokio::test]
    async fn missing_project_is_reported() {
        let store = MemoryStore::new();
        let id = ProjectId::new_v7();
        assert!(store.load(id).await.expect("load").is_none());
        assert!(matches!(
            store.append(id, &[op(1)]).await,
            Err(StoreFault::Missing)
        ));
    }

    #[tokio::test]
    async fn list_is_ordered_and_limited() {
        let store = MemoryStore::new();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let m = meta();
            ids.push(m.id);
            store.create(&m, b"s").await.expect("create");
        }
        ids.sort();
        let listed: Vec<_> = store
            .list(2)
            .await
            .expect("list")
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(listed, ids[..2]);
        assert_eq!(store.count().await.expect("count"), 3);
    }
}
