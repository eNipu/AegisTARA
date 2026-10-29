//! In-memory [`ProjectStore`]: tests, development, and the reference
//! behaviour every backend is compared against.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::audit::{ChainReport, ChainVerifier, Hash, genesis, link};
use super::{
    BaselineMeta, FigureMeta, LoadedProject, MAX_PROJECT_MEMBERSHIPS, ProjectMembership,
    ProjectMeta, ProjectStore, StoreFault, StoredOp, StoredSnapshot,
};
use crate::ids::ProjectId;

#[derive(Debug)]
struct Entry {
    meta: ProjectMeta,
    snapshot: StoredSnapshot,
    /// Every op ever accepted, `ops[i].seq == i + 1`.
    ops: Vec<StoredOp>,
    /// `links[i]` is `(prev, hash)` of `ops[i]` in the audit chain.
    links: Vec<(Hash, Hash)>,
    head: Hash,
    /// Oldest first.
    baselines: Vec<(BaselineMeta, Vec<u8>)>,
    /// Oldest first.
    figures: Vec<(FigureMeta, Vec<u8>)>,
    /// Ordered by stable member key.
    memberships: BTreeMap<String, ProjectMembership>,
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
                    links: Vec::new(),
                    head: genesis(meta.id),
                    baselines: Vec::new(),
                    figures: Vec::new(),
                    memberships: BTreeMap::new(),
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
            for op in ops {
                let prev = entry.head;
                entry.head = link(&prev, op);
                entry.links.push((prev, entry.head));
            }
            entry.ops.extend_from_slice(ops);
            Ok(())
        })
    }

    async fn verify_chain(&self, id: ProjectId) -> Result<ChainReport, StoreFault> {
        self.with(|m| {
            let entry = m.get(&id).ok_or(StoreFault::Missing)?;
            let mut verifier = ChainVerifier::new(id);
            for (op, (prev, hash)) in entry.ops.iter().zip(&entry.links) {
                verifier.push(op, prev, hash);
            }
            Ok(verifier.finish(&entry.head))
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

    async fn rename(&self, id: ProjectId, name: &str) -> Result<(), StoreFault> {
        self.with(|m| {
            m.get_mut(&id).ok_or(StoreFault::Missing)?.meta.name = name.to_owned();
            Ok(())
        })
    }

    async fn delete(&self, id: ProjectId) -> Result<(), StoreFault> {
        self.with(|m| m.remove(&id).map(|_| ()).ok_or(StoreFault::Missing))
    }

    async fn recent_ops(&self, id: ProjectId, limit: usize) -> Result<Vec<StoredOp>, StoreFault> {
        self.with(|m| {
            let entry = m.get(&id).ok_or(StoreFault::Missing)?;
            Ok(entry.ops.iter().rev().take(limit).cloned().collect())
        })
    }

    async fn active_editors(&self, since_ms: u64) -> Result<Vec<(String, u64)>, StoreFault> {
        Ok(self.with(|m| {
            let mut latest: BTreeMap<String, u64> = BTreeMap::new();
            for op in m.values().flat_map(|e| &e.ops) {
                if op.timestamp_ms >= since_ms {
                    let t = latest.entry(op.actor.clone()).or_default();
                    *t = (*t).max(op.timestamp_ms);
                }
            }
            latest.into_iter().collect()
        }))
    }

    async fn create_baseline(
        &self,
        meta: &BaselineMeta,
        snapshot: &[u8],
    ) -> Result<(), StoreFault> {
        self.with(|m| {
            if m.values()
                .any(|e| e.baselines.iter().any(|(b, _)| b.id == meta.id))
            {
                return Err(StoreFault::Exists);
            }
            let entry = m.get_mut(&meta.project).ok_or(StoreFault::Missing)?;
            entry.baselines.push((meta.clone(), snapshot.to_vec()));
            Ok(())
        })
    }

    async fn list_baselines(&self, project: ProjectId) -> Result<Vec<BaselineMeta>, StoreFault> {
        self.with(|m| {
            let entry = m.get(&project).ok_or(StoreFault::Missing)?;
            Ok(entry.baselines.iter().map(|(b, _)| b.clone()).collect())
        })
    }

    async fn load_baseline(
        &self,
        project: ProjectId,
        id: uuid::Uuid,
    ) -> Result<Option<(BaselineMeta, Vec<u8>)>, StoreFault> {
        Ok(self.with(|m| {
            m.get(&project)
                .and_then(|e| e.baselines.iter().find(|(b, _)| b.id == id).cloned())
        }))
    }

    async fn all_memberships(&self) -> Result<Vec<ProjectMembership>, StoreFault> {
        Ok(self.with(|projects| {
            projects
                .values()
                .flat_map(|entry| entry.memberships.values())
                .cloned()
                .collect()
        }))
    }

    async fn project_memberships(
        &self,
        project: ProjectId,
    ) -> Result<Vec<ProjectMembership>, StoreFault> {
        self.with(|m| {
            let entry = m.get(&project).ok_or(StoreFault::Missing)?;
            Ok(entry.memberships.values().cloned().collect())
        })
    }

    async fn memberships_for(
        &self,
        members: &[String],
    ) -> Result<Vec<ProjectMembership>, StoreFault> {
        Ok(self.with(|projects| {
            projects
                .values()
                .flat_map(|entry| entry.memberships.values())
                .filter(|row| members.binary_search(&row.member).is_ok())
                .cloned()
                .collect()
        }))
    }

    async fn set_membership(&self, membership: &ProjectMembership) -> Result<(), StoreFault> {
        self.with(|projects| {
            let entry = projects
                .get_mut(&membership.project)
                .ok_or(StoreFault::Missing)?;
            if !entry.memberships.contains_key(&membership.member)
                && entry.memberships.len() >= MAX_PROJECT_MEMBERSHIPS
            {
                return Err(StoreFault::QuotaExceeded(
                    "project membership limit reached".into(),
                ));
            }
            entry
                .memberships
                .insert(membership.member.clone(), membership.clone());
            Ok(())
        })
    }

    async fn remove_membership(
        &self,
        project: ProjectId,
        member: &str,
    ) -> Result<bool, StoreFault> {
        self.with(|projects| {
            let entry = projects.get_mut(&project).ok_or(StoreFault::Missing)?;
            Ok(entry.memberships.remove(member).is_some())
        })
    }

    async fn put_figure(
        &self,
        meta: &FigureMeta,
        data: &[u8],
        max_count: usize,
        max_total_bytes: u64,
    ) -> Result<(), StoreFault> {
        self.with(|m| {
            if m.values()
                .any(|e| e.figures.iter().any(|(f, _)| f.id == meta.id))
            {
                return Err(StoreFault::Exists);
            }
            let entry = m.get_mut(&meta.project).ok_or(StoreFault::Missing)?;
            if entry.figures.len() >= max_count {
                return Err(StoreFault::QuotaExceeded(format!(
                    "project already has {} figures (limit {max_count})",
                    entry.figures.len()
                )));
            }
            let used_bytes: u64 = entry.figures.iter().map(|(f, _)| f.byte_len).sum();
            let new_total = used_bytes.saturating_add(meta.byte_len);
            if new_total > max_total_bytes {
                return Err(StoreFault::QuotaExceeded(format!(
                    "project figure bytes ({new_total}) would exceed limit ({max_total_bytes})"
                )));
            }
            entry.figures.push((meta.clone(), data.to_vec()));
            Ok(())
        })
    }

    async fn list_figures(&self, project: ProjectId) -> Result<Vec<FigureMeta>, StoreFault> {
        self.with(|m| {
            let entry = m.get(&project).ok_or(StoreFault::Missing)?;
            Ok(entry.figures.iter().map(|(f, _)| f.clone()).collect())
        })
    }

    async fn load_figure(
        &self,
        project: ProjectId,
        id: uuid::Uuid,
    ) -> Result<Option<(FigureMeta, Vec<u8>)>, StoreFault> {
        Ok(self.with(|m| {
            m.get(&project)
                .and_then(|e| e.figures.iter().find(|(f, _)| f.id == id).cloned())
        }))
    }

    async fn update_figure(
        &self,
        project: ProjectId,
        id: uuid::Uuid,
        caption: &str,
        target: &str,
    ) -> Result<Option<FigureMeta>, StoreFault> {
        self.with(|m| {
            let entry = m.get_mut(&project).ok_or(StoreFault::Missing)?;
            let Some((fig, _)) = entry.figures.iter_mut().find(|(f, _)| f.id == id) else {
                return Ok(None);
            };
            fig.caption = caption.to_owned();
            fig.target = target.to_owned();
            Ok(Some(fig.clone()))
        })
    }

    async fn delete_figure(&self, project: ProjectId, id: uuid::Uuid) -> Result<bool, StoreFault> {
        self.with(|m| {
            let entry = m.get_mut(&project).ok_or(StoreFault::Missing)?;
            let before = entry.figures.len();
            entry.figures.retain(|(f, _)| f.id != id);
            Ok(entry.figures.len() != before)
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
    async fn the_audit_chain_verifies_and_catches_edits() {
        let store = MemoryStore::new();
        let m = meta();
        store.create(&m, b"s0").await.expect("create");
        let empty = store.verify_chain(m.id).await.expect("verify");
        assert!(empty.is_intact());
        assert_eq!(empty.verified, 0);

        store.append(m.id, &[op(1), op(2)]).await.expect("append");
        store.append(m.id, &[op(3)]).await.expect("append");
        let report = store.verify_chain(m.id).await.expect("verify");
        assert!(report.is_intact());
        assert_eq!(report.verified, 3);

        // Someone edits an op behind the store's back.
        store.with(|map| {
            let entry = map.get_mut(&m.id).expect("exists");
            entry.ops[1].actor = "mallory".into();
        });
        let report = store.verify_chain(m.id).await.expect("verify");
        assert_eq!(report.first_bad, Some(2));
        assert_eq!(report.verified, 1);
        assert!(matches!(
            store.verify_chain(ProjectId::new_v7()).await,
            Err(StoreFault::Missing)
        ));
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
