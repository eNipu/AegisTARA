//! PostgreSQL [`ProjectStore`].
//!
//! Two tables (`migrations/001_init.sql`): `aegis_project` (one row per
//! project: facts, the snapshot cache, the hash chain head) and
//! `aegis_project_op` (the append-only history, one row per accepted op).
//!
//! Concurrency and integrity:
//! - `append` is one transaction. It locks the project row (`FOR UPDATE`),
//!   checks that the batch continues `last_seq` exactly, inserts every op
//!   with its chain hash in a single `UNNEST` statement (one round trip for
//!   any batch size), and moves `last_seq` and `last_hash`. Two writers can
//!   never interleave; a failed commit leaves nothing behind.
//! - `load` reads the snapshot and the ops after it in one repeatable-read
//!   transaction, so the pair is consistent.
//! - `verify_chain` streams the history through the verifier; memory does
//!   not grow with the number of ops (R3).
//!
//! Bounds (R6): the pool waits at most 5 s for a connection, connects within
//! 5 s, and the server cancels any statement after 30 s.
//!
//! The connection is plain TCP. A database on another host should be
//! reached over a private network or a TLS-terminating proxy until native
//! TLS is added (tracked in the backlog).

use std::str::FromStr;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime, Timeouts};
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{IsolationLevel, NoTls, Row};
use tokio_stream::StreamExt;
use uuid::Uuid;

use super::audit::{ChainReport, ChainVerifier, Hash, genesis, link};
use super::{LoadedProject, ProjectMeta, ProjectStore, StoreFault, StoredOp, StoredSnapshot};
use crate::config::system_clock_ms;
use crate::ids::ProjectId;

/// Migrations in order. A database that has applied a version this build
/// does not know is refused (it was written by a newer server).
const MIGRATIONS: &[(i32, &str)] = &[(1, include_str!("../../migrations/001_init.sql"))];

/// Serializes concurrent migrators (several servers starting together).
const MIGRATION_LOCK: i64 = 0x4145_4749_5354_4152; // "AEGISTAR"

/// Tunables of the connection pool.
#[derive(Clone, Debug)]
pub struct PostgresOptions {
    /// Most connections held open.
    pub pool_size: usize,
    /// The organization this deployment serves (direction section 2). The
    /// nil UUID means "the only one".
    pub org_id: Uuid,
    /// Longest wait for a free connection.
    pub acquire_timeout: Duration,
    /// Statements running longer than this are cancelled by the server.
    pub statement_timeout: Duration,
}

impl Default for PostgresOptions {
    fn default() -> Self {
        PostgresOptions {
            pool_size: 16,
            org_id: Uuid::nil(),
            acquire_timeout: Duration::from_secs(5),
            statement_timeout: Duration::from_secs(30),
        }
    }
}

/// A [`ProjectStore`] backed by PostgreSQL.
#[derive(Clone)]
pub struct PostgresStore {
    pool: Pool,
    org: Uuid,
}

fn backend(e: impl std::fmt::Display) -> StoreFault {
    StoreFault::Backend(e.to_string())
}

fn to_i64(v: u64) -> Result<i64, StoreFault> {
    i64::try_from(v).map_err(|_| backend("value does not fit a bigint"))
}

fn to_u64(v: i64) -> Result<u64, StoreFault> {
    u64::try_from(v).map_err(|_| StoreFault::Corrupt("negative number in a counter column".into()))
}

fn hash_col(row: &Row, idx: usize) -> Result<Hash, StoreFault> {
    let bytes: Vec<u8> = row.try_get(idx).map_err(backend)?;
    Hash::try_from(bytes.as_slice()).map_err(|_| StoreFault::Corrupt("hash is not 32 bytes".into()))
}

fn stored_op(row: &Row) -> Result<StoredOp, StoreFault> {
    Ok(StoredOp {
        seq: to_u64(row.try_get(0).map_err(backend)?)?,
        actor: row.try_get(1).map_err(backend)?,
        timestamp_ms: to_u64(row.try_get(2).map_err(backend)?)?,
        op: row.try_get(3).map_err(backend)?,
    })
}

impl PostgresStore {
    /// Connects, builds the pool, and brings the schema up to date.
    ///
    /// `url` is a libpq-style connection string
    /// (`postgres://user:password@host/db`).
    ///
    /// # Errors
    ///
    /// [`StoreFault::Backend`] for a bad URL, an unreachable database, a
    /// failed migration, or a database written by a newer server.
    pub async fn connect(url: &str, options: &PostgresOptions) -> Result<Self, StoreFault> {
        let config = tokio_postgres::Config::from_str(url).map_err(backend)?;
        Self::connect_config(config, options).await
    }

    /// Like [`connect`](Self::connect), from an already built configuration.
    ///
    /// # Errors
    ///
    /// As [`connect`](Self::connect).
    pub async fn connect_config(
        mut config: tokio_postgres::Config,
        options: &PostgresOptions,
    ) -> Result<Self, StoreFault> {
        config.connect_timeout(Duration::from_secs(5));
        config.application_name("aegis-server");
        let timeout_ms = options.statement_timeout.as_millis();
        let extra = format!("-c statement_timeout={timeout_ms}");
        let merged = match config.get_options() {
            Some(existing) => format!("{existing} {extra}"),
            None => extra,
        };
        config.options(merged);

        let manager = Manager::from_config(
            config,
            NoTls,
            ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
            },
        );
        let pool = Pool::builder(manager)
            .max_size(options.pool_size.max(1))
            .runtime(Runtime::Tokio1)
            .timeouts(Timeouts {
                wait: Some(options.acquire_timeout),
                create: Some(Duration::from_secs(5)),
                recycle: Some(Duration::from_secs(5)),
            })
            .build()
            .map_err(backend)?;
        let store = PostgresStore {
            pool,
            org: options.org_id,
        };
        store.migrate().await?;
        Ok(store)
    }

    async fn client(&self) -> Result<deadpool_postgres::Object, StoreFault> {
        self.pool.get().await.map_err(backend)
    }

    /// Applies the migrations this build knows and the database lacks.
    async fn migrate(&self) -> Result<(), StoreFault> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(backend)?;
        tx.execute("SELECT pg_advisory_xact_lock($1)", &[&MIGRATION_LOCK])
            .await
            .map_err(backend)?;
        tx.batch_execute(
            "CREATE TABLE IF NOT EXISTS aegis_schema_migration (
                version    integer PRIMARY KEY,
                applied_ms bigint  NOT NULL
            )",
        )
        .await
        .map_err(backend)?;
        let applied: Vec<i32> = tx
            .query(
                "SELECT version FROM aegis_schema_migration ORDER BY version",
                &[],
            )
            .await
            .map_err(backend)?
            .iter()
            .map(|r| r.get(0))
            .collect();
        let known = MIGRATIONS.iter().map(|(v, _)| *v).max().unwrap_or(0);
        if applied.iter().any(|v| *v > known) {
            return Err(backend("the database was migrated by a newer server"));
        }
        for (version, sql) in MIGRATIONS {
            if applied.contains(version) {
                continue;
            }
            tx.batch_execute(sql).await.map_err(backend)?;
            let now = to_i64(system_clock_ms())?;
            tx.execute(
                "INSERT INTO aegis_schema_migration (version, applied_ms) VALUES ($1, $2)",
                &[version, &now],
            )
            .await
            .map_err(backend)?;
            tracing::info!(version, "applied database migration");
        }
        tx.commit().await.map_err(backend)
    }
}

impl ProjectStore for PostgresStore {
    async fn create(&self, meta: &ProjectMeta, snapshot: &[u8]) -> Result<(), StoreFault> {
        let client = self.client().await?;
        let created = to_i64(meta.created_ms)?;
        let head = genesis(meta.id).to_vec();
        let n = client
            .execute(
                "INSERT INTO aegis_project
                     (project_id, org_id, name, methodology_key, methodology_version,
                      created_ms, last_seq, last_hash, snapshot_seq, snapshot)
                 VALUES ($1, $2, $3, $4, $5, $6, 0, $7, 0, $8)
                 ON CONFLICT (project_id) DO NOTHING",
                &[
                    &meta.id.as_uuid(),
                    &self.org,
                    &meta.name,
                    &meta.methodology_key,
                    &meta.methodology_version,
                    &created,
                    &head,
                    &snapshot,
                ],
            )
            .await
            .map_err(backend)?;
        if n == 0 {
            Err(StoreFault::Exists)
        } else {
            Ok(())
        }
    }

    async fn list(&self, limit: usize) -> Result<Vec<ProjectMeta>, StoreFault> {
        let client = self.client().await?;
        let limit = to_i64(limit as u64)?;
        let rows = client
            .query(
                "SELECT project_id, name, methodology_key, methodology_version, created_ms
                 FROM aegis_project WHERE org_id = $1
                 ORDER BY project_id LIMIT $2",
                &[&self.org, &limit],
            )
            .await
            .map_err(backend)?;
        rows.iter().map(meta_of).collect()
    }

    async fn count(&self) -> Result<usize, StoreFault> {
        let client = self.client().await?;
        let row = client
            .query_one(
                "SELECT count(*) FROM aegis_project WHERE org_id = $1",
                &[&self.org],
            )
            .await
            .map_err(backend)?;
        let n: i64 = row.get(0);
        usize::try_from(n).map_err(|_| backend("count out of range"))
    }

    async fn load(&self, id: ProjectId) -> Result<Option<LoadedProject>, StoreFault> {
        let mut client = self.client().await?;
        let tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .await
            .map_err(backend)?;
        let Some(row) = tx
            .query_opt(
                "SELECT project_id, name, methodology_key, methodology_version, created_ms,
                        snapshot_seq, snapshot
                 FROM aegis_project WHERE project_id = $1 AND org_id = $2",
                &[&id.as_uuid(), &self.org],
            )
            .await
            .map_err(backend)?
        else {
            return Ok(None);
        };
        let meta = meta_of(&row)?;
        let snapshot_seq = to_u64(row.try_get(5).map_err(backend)?)?;
        let snapshot: Vec<u8> = row.try_get(6).map_err(backend)?;
        let after = to_i64(snapshot_seq)?;
        let ops = tx
            .query(
                "SELECT seq, actor, timestamp_ms, op FROM aegis_project_op
                 WHERE project_id = $1 AND seq > $2 ORDER BY seq",
                &[&id.as_uuid(), &after],
            )
            .await
            .map_err(backend)?
            .iter()
            .map(stored_op)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(LoadedProject {
            meta,
            snapshot: StoredSnapshot {
                seq: snapshot_seq,
                bytes: snapshot,
            },
            ops,
        }))
    }

    async fn append(&self, id: ProjectId, ops: &[StoredOp]) -> Result<(), StoreFault> {
        if ops.is_empty() {
            return Ok(());
        }
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(backend)?;
        let row = tx
            .query_opt(
                "SELECT last_seq, last_hash FROM aegis_project
                 WHERE project_id = $1 AND org_id = $2 FOR UPDATE",
                &[&id.as_uuid(), &self.org],
            )
            .await
            .map_err(backend)?
            .ok_or(StoreFault::Missing)?;
        let last_seq = to_u64(row.try_get(0).map_err(backend)?)?;
        let mut prev = hash_col(&row, 1)?;

        let n = ops.len();
        let (mut seqs, mut actors, mut times) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        let (mut bodies, mut prevs, mut hashes) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        for (expected, op) in (last_seq + 1..).zip(ops) {
            if op.seq != expected {
                return Err(StoreFault::Sequence {
                    expected,
                    found: op.seq,
                });
            }
            let hash = link(&prev, op);
            seqs.push(to_i64(op.seq)?);
            actors.push(op.actor.as_str());
            times.push(to_i64(op.timestamp_ms)?);
            bodies.push(op.op.as_slice());
            prevs.push(prev.to_vec());
            hashes.push(hash.to_vec());
            prev = hash;
        }
        let new_last = to_i64(last_seq + n as u64)?;
        tx.execute(
            "INSERT INTO aegis_project_op
                 (project_id, seq, actor, timestamp_ms, op, prev_hash, hash)
             SELECT $1::uuid, u.seq, u.actor, u.ts, u.op, u.prev, u.hash
             FROM UNNEST($2::bigint[], $3::text[], $4::bigint[], $5::bytea[], $6::bytea[], $7::bytea[])
                  AS u(seq, actor, ts, op, prev, hash)",
            &[&id.as_uuid(), &seqs, &actors, &times, &bodies, &prevs, &hashes],
        )
        .await
        .map_err(backend)?;
        tx.execute(
            "UPDATE aegis_project SET last_seq = $2, last_hash = $3 WHERE project_id = $1",
            &[&id.as_uuid(), &new_last, &prev.to_vec()],
        )
        .await
        .map_err(backend)?;
        tx.commit().await.map_err(backend)
    }

    async fn save_snapshot(
        &self,
        id: ProjectId,
        snapshot: &StoredSnapshot,
    ) -> Result<(), StoreFault> {
        let client = self.client().await?;
        let seq = to_i64(snapshot.seq)?;
        let n = client
            .execute(
                "UPDATE aegis_project SET snapshot = $3, snapshot_seq = $2
                 WHERE project_id = $1 AND org_id = $4
                   AND snapshot_seq <= $2 AND last_seq >= $2",
                &[&id.as_uuid(), &seq, &snapshot.bytes, &self.org],
            )
            .await
            .map_err(backend)?;
        if n == 1 {
            return Ok(());
        }
        let row = client
            .query_opt(
                "SELECT last_seq FROM aegis_project WHERE project_id = $1 AND org_id = $2",
                &[&id.as_uuid(), &self.org],
            )
            .await
            .map_err(backend)?
            .ok_or(StoreFault::Missing)?;
        Err(StoreFault::Sequence {
            expected: to_u64(row.try_get(0).map_err(backend)?)?,
            found: snapshot.seq,
        })
    }

    async fn verify_chain(&self, id: ProjectId) -> Result<ChainReport, StoreFault> {
        let mut client = self.client().await?;
        let tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .await
            .map_err(backend)?;
        let head_row = tx
            .query_opt(
                "SELECT last_hash FROM aegis_project WHERE project_id = $1 AND org_id = $2",
                &[&id.as_uuid(), &self.org],
            )
            .await
            .map_err(backend)?
            .ok_or(StoreFault::Missing)?;
        let claimed = hash_col(&head_row, 0)?;

        let statement = tx
            .prepare_typed(
                "SELECT seq, actor, timestamp_ms, op, prev_hash, hash FROM aegis_project_op
                 WHERE project_id = $1 ORDER BY seq",
                &[Type::UUID],
            )
            .await
            .map_err(backend)?;
        let uuid = id.as_uuid();
        let params: [&(dyn ToSql + Sync); 1] = [&uuid];
        let mut rows = std::pin::pin!(tx.query_raw(&statement, params).await.map_err(backend)?);
        let mut verifier = ChainVerifier::new(id);
        while let Some(row) = rows.next().await {
            let row = row.map_err(backend)?;
            verifier.push(&stored_op(&row)?, &hash_col(&row, 4)?, &hash_col(&row, 5)?);
        }
        Ok(verifier.finish(&claimed))
    }
}

fn meta_of(row: &Row) -> Result<ProjectMeta, StoreFault> {
    let uuid: Uuid = row.try_get(0).map_err(backend)?;
    Ok(ProjectMeta {
        id: ProjectId::from_uuid(uuid),
        name: row.try_get(1).map_err(backend)?,
        methodology_key: row.try_get(2).map_err(backend)?,
        methodology_version: row.try_get(3).map_err(backend)?,
        created_ms: to_u64(row.try_get(4).map_err(backend)?)?,
    })
}
