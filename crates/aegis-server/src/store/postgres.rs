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
//! Transport (backlog 037): plain TCP by default, for a database on the
//! same host or a private network (the Compose file and Railway's private
//! network). [`DbTls::Verify`] encrypts and checks the server certificate
//! against the OS trust store and the host name (libpq `verify-full`);
//! [`DbTls::Encrypt`] encrypts without checking the certificate (libpq
//! `require`), for managed databases with self-signed certificates. Both
//! use rustls with ring (`aegis_auth::tls`).

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime, Timeouts};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject as _;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{IsolationLevel, NoTls, Row};
use tokio_stream::StreamExt;
use uuid::Uuid;

use super::audit::{ChainReport, ChainVerifier, Hash, genesis, link};
use super::{
    BaselineMeta, LoadedProject, ProjectMeta, ProjectStore, StoreFault, StoredOp, StoredSnapshot,
};
use crate::config::system_clock_ms;
use crate::ids::ProjectId;

/// Largest CA bundle read from `AEGIS_DB_CA_FILE` (1 MiB, R3).
const MAX_CA_FILE_BYTES: u64 = 1024 * 1024;

/// Migrations in order. A database that has applied a version this build
/// does not know is refused (it was written by a newer server).
const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("../../migrations/001_init.sql")),
    (
        2,
        include_str!("../../migrations/002_accounts_baselines.sql"),
    ),
    (3, include_str!("../../migrations/003_settings.sql")),
    (4, include_str!("../../migrations/004_event_chain.sql")),
];

/// How the server talks to PostgreSQL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DbTls {
    /// Plain TCP. Only for a database on a private network.
    #[default]
    Off,
    /// TLS with the server certificate and host name checked against the OS
    /// trust store or the configured [`PostgresOptions::ca_file`] (SEC-05).
    Encrypt,
    /// TLS with the server certificate and host name checked against the OS
    /// trust store or the configured [`PostgresOptions::ca_file`] (libpq
    /// `sslmode=verify-full`).
    Verify,
}

impl std::str::FromStr for DbTls {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "off" | "disable" => Ok(DbTls::Off),
            "encrypt" | "require" => Ok(DbTls::Encrypt),
            "verify" | "verify-full" => Ok(DbTls::Verify),
            _ => Err("expected off, encrypt or verify".to_owned()),
        }
    }
}

fn load_ca_roots(path: &Path) -> Result<rustls::RootCertStore, StoreFault> {
    let file = std::fs::File::open(path)
        .map_err(|e| backend(format!("cannot open CA file {}: {e}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(MAX_CA_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| backend(format!("cannot read CA file {}: {e}", path.display())))?;
    if bytes.len() as u64 > MAX_CA_FILE_BYTES {
        return Err(backend("CA file exceeds 1 MiB"));
    }
    let certs = CertificateDer::pem_slice_iter(&bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| backend(format!("invalid PEM in CA file {}: {e}", path.display())))?;
    if certs.is_empty() {
        return Err(backend(format!(
            "CA file {} contains no certificates",
            path.display()
        )));
    }
    let mut roots = rustls::RootCertStore::empty();
    let (added, _) = roots.add_parsable_certificates(certs);
    if added == 0 {
        return Err(backend(format!(
            "CA file {} contains no usable root certificates",
            path.display()
        )));
    }
    Ok(roots)
}

fn rustls_config(tls: DbTls, ca_file: Option<&Path>) -> Result<rustls::ClientConfig, StoreFault> {
    match tls {
        DbTls::Off => Err(backend("TLS is off")),
        DbTls::Encrypt | DbTls::Verify => {
            if let Some(path) = ca_file {
                let roots = load_ca_roots(path)?;
                aegis_auth::tls::config_with_roots(roots)
                    .map(|c| (*c).clone())
                    .map_err(backend)
            } else {
                aegis_auth::tls::client_config()
                    .map(|c| (*c).clone())
                    .map_err(backend)
            }
        }
    }
}

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
    /// Transport security.
    pub tls: DbTls,
    /// Optional PEM certificate authority bundle (`AEGIS_DB_CA_FILE`) used to
    /// verify the PostgreSQL server certificate when TLS is enabled (SEC-05).
    /// When `None`, the operating system root store is used.
    pub ca_file: Option<PathBuf>,
}

impl Default for PostgresOptions {
    fn default() -> Self {
        PostgresOptions {
            pool_size: 16,
            org_id: Uuid::nil(),
            acquire_timeout: Duration::from_secs(5),
            statement_timeout: Duration::from_secs(30),
            tls: DbTls::Off,
            ca_file: None,
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

        let manager_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let manager = match options.tls {
            DbTls::Off => Manager::from_config(config, NoTls, manager_config),
            tls => {
                config.ssl_mode(tokio_postgres::config::SslMode::Require);
                let connector = tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config(
                    tls,
                    options.ca_file.as_deref(),
                )?);
                Manager::from_config(config, connector, manager_config)
            }
        };
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

    /// The connection pool, shared with the account store.
    #[must_use]
    pub fn pool(&self) -> Pool {
        self.pool.clone()
    }

    /// The organization this store serves.
    #[must_use]
    pub fn org(&self) -> Uuid {
        self.org
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

    async fn rename(&self, id: ProjectId, name: &str) -> Result<(), StoreFault> {
        let client = self.client().await?;
        let n = client
            .execute(
                "UPDATE aegis_project SET name = $3 WHERE project_id = $1 AND org_id = $2",
                &[&id.as_uuid(), &self.org, &name],
            )
            .await
            .map_err(backend)?;
        if n == 1 {
            Ok(())
        } else {
            Err(StoreFault::Missing)
        }
    }

    async fn delete(&self, id: ProjectId) -> Result<(), StoreFault> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(backend)?;
        let found = tx
            .query_opt(
                "SELECT 1 FROM aegis_project WHERE project_id = $1 AND org_id = $2 FOR UPDATE",
                &[&id.as_uuid(), &self.org],
            )
            .await
            .map_err(backend)?;
        if found.is_none() {
            return Err(StoreFault::Missing);
        }
        for sql in [
            "DELETE FROM aegis_baseline WHERE project_id = $1",
            "DELETE FROM aegis_project_op WHERE project_id = $1",
            "DELETE FROM aegis_project WHERE project_id = $1",
        ] {
            tx.execute(sql, &[&id.as_uuid()]).await.map_err(backend)?;
        }
        tx.commit().await.map_err(backend)
    }

    async fn recent_ops(&self, id: ProjectId, limit: usize) -> Result<Vec<StoredOp>, StoreFault> {
        let client = self.client().await?;
        let exists = client
            .query_opt(
                "SELECT 1 FROM aegis_project WHERE project_id = $1 AND org_id = $2",
                &[&id.as_uuid(), &self.org],
            )
            .await
            .map_err(backend)?;
        if exists.is_none() {
            return Err(StoreFault::Missing);
        }
        let limit = to_i64(limit as u64)?;
        client
            .query(
                "SELECT seq, actor, timestamp_ms, op FROM aegis_project_op
                 WHERE project_id = $1 ORDER BY seq DESC LIMIT $2",
                &[&id.as_uuid(), &limit],
            )
            .await
            .map_err(backend)?
            .iter()
            .map(stored_op)
            .collect()
    }

    async fn active_editors(&self, since_ms: u64) -> Result<Vec<(String, u64)>, StoreFault> {
        let client = self.client().await?;
        let since = to_i64(since_ms)?;
        client
            .query(
                "SELECT o.actor, max(o.timestamp_ms) FROM aegis_project_op o
                 JOIN aegis_project p ON p.project_id = o.project_id
                 WHERE p.org_id = $1 AND o.timestamp_ms >= $2
                 GROUP BY o.actor ORDER BY o.actor",
                &[&self.org, &since],
            )
            .await
            .map_err(backend)?
            .iter()
            .map(|row| {
                Ok((
                    row.try_get(0).map_err(backend)?,
                    to_u64(row.try_get(1).map_err(backend)?)?,
                ))
            })
            .collect()
    }

    async fn create_baseline(
        &self,
        meta: &BaselineMeta,
        snapshot: &[u8],
    ) -> Result<(), StoreFault> {
        let client = self.client().await?;
        let exists = client
            .query_opt(
                "SELECT 1 FROM aegis_project WHERE project_id = $1 AND org_id = $2",
                &[&meta.project.as_uuid(), &self.org],
            )
            .await
            .map_err(backend)?;
        if exists.is_none() {
            return Err(StoreFault::Missing);
        }
        let n = client
            .execute(
                "INSERT INTO aegis_baseline
                     (baseline_id, project_id, name, seq, created_by, created_ms, snapshot)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (baseline_id) DO NOTHING",
                &[
                    &meta.id,
                    &meta.project.as_uuid(),
                    &meta.name,
                    &to_i64(meta.seq)?,
                    &meta.created_by,
                    &to_i64(meta.created_ms)?,
                    &snapshot,
                ],
            )
            .await
            .map_err(backend)?;
        if n == 1 {
            Ok(())
        } else {
            Err(StoreFault::Exists)
        }
    }

    async fn list_baselines(&self, project: ProjectId) -> Result<Vec<BaselineMeta>, StoreFault> {
        let client = self.client().await?;
        let exists = client
            .query_opt(
                "SELECT 1 FROM aegis_project WHERE project_id = $1 AND org_id = $2",
                &[&project.as_uuid(), &self.org],
            )
            .await
            .map_err(backend)?;
        if exists.is_none() {
            return Err(StoreFault::Missing);
        }
        client
            .query(
                "SELECT baseline_id, project_id, name, seq, created_by, created_ms,
                        octet_length(snapshot)::bigint
                 FROM aegis_baseline WHERE project_id = $1 ORDER BY baseline_id",
                &[&project.as_uuid()],
            )
            .await
            .map_err(backend)?
            .iter()
            .map(baseline_of)
            .collect()
    }

    async fn load_baseline(
        &self,
        project: ProjectId,
        id: Uuid,
    ) -> Result<Option<(BaselineMeta, Vec<u8>)>, StoreFault> {
        let client = self.client().await?;
        let row = client
            .query_opt(
                "SELECT b.baseline_id, b.project_id, b.name, b.seq, b.created_by, b.created_ms,
                        octet_length(b.snapshot)::bigint, b.snapshot
                 FROM aegis_baseline b JOIN aegis_project p ON p.project_id = b.project_id
                 WHERE b.project_id = $1 AND b.baseline_id = $2 AND p.org_id = $3",
                &[&project.as_uuid(), &id, &self.org],
            )
            .await
            .map_err(backend)?;
        row.map(|row| Ok((baseline_of(&row)?, row.try_get(7).map_err(backend)?)))
            .transpose()
    }
}

fn baseline_of(row: &Row) -> Result<BaselineMeta, StoreFault> {
    let project: Uuid = row.try_get(1).map_err(backend)?;
    Ok(BaselineMeta {
        id: row.try_get(0).map_err(backend)?,
        project: ProjectId::from_uuid(project),
        name: row.try_get(2).map_err(backend)?,
        seq: to_u64(row.try_get(3).map_err(backend)?)?,
        created_by: row.try_get(4).map_err(backend)?,
        created_ms: to_u64(row.try_get(5).map_err(backend)?)?,
        snapshot_bytes: to_u64(row.try_get(6).map_err(backend)?)?,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_encrypt_and_verify_both_require_verified_roots_or_configured_ca_file() {
        assert!(rustls_config(DbTls::Off, None).is_err());
        assert!(rustls_config(DbTls::Encrypt, None).is_ok());
        assert!(rustls_config(DbTls::Verify, None).is_ok());

        let dir = std::env::temp_dir().join(format!("aegis-pg-ca-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let empty = dir.join("empty.pem");
        std::fs::write(&empty, b"").expect("write");
        assert!(rustls_config(DbTls::Encrypt, Some(&empty)).is_err());
        assert!(rustls_config(DbTls::Verify, Some(&empty)).is_err());

        let invalid = dir.join("invalid.pem");
        std::fs::write(
            &invalid,
            b"-----BEGIN CERTIFICATE-----\nnot-base64\n-----END CERTIFICATE-----\n",
        )
        .expect("write");
        assert!(rustls_config(DbTls::Encrypt, Some(&invalid)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
