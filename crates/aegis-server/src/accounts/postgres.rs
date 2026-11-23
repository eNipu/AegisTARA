//! PostgreSQL account storage (`migrations/002_accounts_baselines.sql`).
//!
//! Shares the connection pool of [`PostgresStore`](crate::PostgresStore),
//! which also runs the migrations. Every write is one statement, so each is
//! atomic on its own; [`Accounts`](super::Accounts) orders them so a crash
//! between two leaves at worst an orphan session row, which is refused
//! (its user check fails) and purged on expiry.

use deadpool_postgres::{GenericClient, Pool};
use tokio_postgres::Row;
use uuid::Uuid;

use super::{AuditEvent, AuditKind, LoadedAccounts, SessionKey, SessionRecord, UserRecord};
use crate::store::StoreFault;
use crate::store::audit::{ChainReport, EventChainVerifier, Hash, event_genesis, event_link};

/// Rows one trim statement deletes at most.
const TRIM_BATCH: i64 = 5000;

/// Accounts in PostgreSQL.
#[derive(Clone)]
pub struct PostgresAccounts {
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
    u64::try_from(v).map_err(|_| StoreFault::Corrupt("negative time".into()))
}

fn hash_of(bytes: Vec<u8>) -> Result<Hash, StoreFault> {
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| StoreFault::Corrupt("event hash is not 32 bytes".into()))
}

fn user_of(row: &Row) -> Result<UserRecord, StoreFault> {
    Ok(UserRecord {
        username: row.try_get(0).map_err(backend)?,
        display_name: row.try_get(1).map_err(backend)?,
        password_hash: row.try_get(2).map_err(backend)?,
        admin: row.try_get(3).map_err(backend)?,
        disabled: row.try_get(4).map_err(backend)?,
        created_ms: to_u64(row.try_get(5).map_err(backend)?)?,
        last_login_ms: to_u64(row.try_get(6).map_err(backend)?)?,
    })
}

fn session_of(row: &Row) -> Result<SessionRecord, StoreFault> {
    let hash: Vec<u8> = row.try_get(0).map_err(backend)?;
    let key: [u8; 32] = hash
        .as_slice()
        .try_into()
        .map_err(|_| StoreFault::Corrupt("session hash is not 32 bytes".into()))?;
    Ok(SessionRecord {
        key: SessionKey(key),
        username: row.try_get(1).map_err(backend)?,
        created_ms: to_u64(row.try_get(2).map_err(backend)?)?,
        expires_ms: to_u64(row.try_get(3).map_err(backend)?)?,
        client_name: row.try_get(4).map_err(backend)?,
    })
}

fn kind_of(text: &str) -> Option<AuditKind> {
    AuditKind::from_name(text)
}

impl PostgresAccounts {
    /// Accounts of organization `org` in the database behind `pool`. The
    /// schema must be migrated (connect a [`PostgresStore`](crate::PostgresStore) first).
    #[must_use]
    pub fn new(pool: Pool, org: Uuid) -> Self {
        PostgresAccounts { pool, org }
    }

    async fn client(&self) -> Result<deadpool_postgres::Object, StoreFault> {
        self.pool.get().await.map_err(backend)
    }

    pub(super) async fn load(&self, now_ms: u64) -> Result<LoadedAccounts, StoreFault> {
        let client = self.client().await?;
        let users = client
            .query(
                "SELECT username, display_name, password_hash, is_admin, disabled,
                        created_ms, last_login_ms
                 FROM aegis_user WHERE org_id = $1 ORDER BY username",
                &[&self.org],
            )
            .await
            .map_err(backend)?
            .iter()
            .map(user_of)
            .collect::<Result<Vec<_>, _>>()?;
        let now = to_i64(now_ms)?;
        let sessions = client
            .query(
                "SELECT token_hash, username, created_ms, expires_ms, client_name
                 FROM aegis_session WHERE org_id = $1 AND expires_ms > $2",
                &[&self.org, &now],
            )
            .await
            .map_err(backend)?
            .iter()
            .map(session_of)
            .collect::<Result<Vec<_>, _>>()?;
        let password_login = client
            .query_opt(
                "SELECT value FROM aegis_setting WHERE org_id = $1 AND key = 'password_login'",
                &[&self.org],
            )
            .await
            .map_err(backend)?
            .map(|row| row.get::<_, String>(0) != "off");
        Ok(LoadedAccounts {
            users,
            sessions,
            password_login,
        })
    }

    pub(super) async fn put_password_login(&self, enabled: bool) -> Result<(), StoreFault> {
        let client = self.client().await?;
        Self::exec_put_password_login(&client, self.org, enabled).await
    }

    pub(super) async fn put_password_login_and_delete_sessions(
        &self,
        enabled: bool,
        keys: &[SessionKey],
    ) -> Result<(), StoreFault> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(backend)?;
        Self::exec_put_password_login(&tx, self.org, enabled).await?;
        Self::exec_delete_sessions(&tx, self.org, keys).await?;
        tx.commit().await.map_err(backend)
    }

    pub(super) async fn put_user(&self, user: &UserRecord) -> Result<(), StoreFault> {
        let client = self.client().await?;
        Self::exec_put_user(&client, self.org, user).await
    }

    pub(super) async fn put_login_session(
        &self,
        user: &UserRecord,
        session: &SessionRecord,
        evicted: &[SessionKey],
    ) -> Result<(), StoreFault> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(backend)?;
        Self::exec_put_user(&tx, self.org, user).await?;
        Self::exec_put_session(&tx, self.org, session).await?;
        Self::exec_delete_sessions(&tx, self.org, evicted).await?;
        tx.commit().await.map_err(backend)
    }

    pub(super) async fn put_user_and_delete_sessions(
        &self,
        user: &UserRecord,
        keys: &[SessionKey],
    ) -> Result<(), StoreFault> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(backend)?;
        Self::exec_put_user(&tx, self.org, user).await?;
        Self::exec_delete_sessions(&tx, self.org, keys).await?;
        tx.commit().await.map_err(backend)
    }

    pub(super) async fn delete_user(&self, username: &str) -> Result<(), StoreFault> {
        // Sessions go with the user (ON DELETE CASCADE).
        let client = self.client().await?;
        client
            .execute(
                "DELETE FROM aegis_user WHERE org_id = $1 AND username = $2",
                &[&self.org, &username],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    pub(super) async fn put_session(&self, session: &SessionRecord) -> Result<(), StoreFault> {
        let client = self.client().await?;
        Self::exec_put_session(&client, self.org, session).await
    }

    pub(super) async fn delete_sessions(&self, keys: &[SessionKey]) -> Result<(), StoreFault> {
        if keys.is_empty() {
            return Ok(());
        }
        let client = self.client().await?;
        Self::exec_delete_sessions(&client, self.org, keys).await
    }

    async fn exec_put_password_login(
        conn: &impl GenericClient,
        org: Uuid,
        enabled: bool,
    ) -> Result<(), StoreFault> {
        conn.execute(
            "INSERT INTO aegis_setting (org_id, key, value) VALUES ($1, 'password_login', $2)
             ON CONFLICT (org_id, key) DO UPDATE SET value = EXCLUDED.value",
            &[&org, &if enabled { "on" } else { "off" }],
        )
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn exec_put_user(
        conn: &impl GenericClient,
        org: Uuid,
        user: &UserRecord,
    ) -> Result<(), StoreFault> {
        conn.execute(
            "INSERT INTO aegis_user
                 (org_id, username, display_name, password_hash, is_admin, disabled,
                  created_ms, last_login_ms)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (org_id, username) DO UPDATE SET
                 display_name = EXCLUDED.display_name,
                 password_hash = EXCLUDED.password_hash,
                 is_admin = EXCLUDED.is_admin,
                 disabled = EXCLUDED.disabled,
                 last_login_ms = EXCLUDED.last_login_ms",
            &[
                &org,
                &user.username,
                &user.display_name,
                &user.password_hash,
                &user.admin,
                &user.disabled,
                &to_i64(user.created_ms)?,
                &to_i64(user.last_login_ms)?,
            ],
        )
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn exec_put_session(
        conn: &impl GenericClient,
        org: Uuid,
        session: &SessionRecord,
    ) -> Result<(), StoreFault> {
        conn.execute(
            "INSERT INTO aegis_session
                 (token_hash, org_id, username, created_ms, expires_ms, client_name)
             VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                &session.key.0.as_slice(),
                &org,
                &session.username,
                &to_i64(session.created_ms)?,
                &to_i64(session.expires_ms)?,
                &session.client_name,
            ],
        )
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn exec_delete_sessions(
        conn: &impl GenericClient,
        org: Uuid,
        keys: &[SessionKey],
    ) -> Result<(), StoreFault> {
        if keys.is_empty() {
            return Ok(());
        }
        let hashes: Vec<&[u8]> = keys.iter().map(|k| k.0.as_slice()).collect();
        conn.execute(
            "DELETE FROM aegis_session WHERE org_id = $1 AND token_hash = ANY($2::bytea[])",
            &[&org, &hashes],
        )
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn append_chained_event(
        conn: &impl GenericClient,
        org: Uuid,
        prev: &Hash,
        event: &AuditEvent,
    ) -> Result<Hash, StoreFault> {
        let next = event_link(prev, event);
        conn.execute(
            "INSERT INTO aegis_event
                 (org_id, at_ms, actor, kind, target, detail, prev_hash, hash)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &org,
                &to_i64(event.at_ms)?,
                &event.actor,
                &event.kind.as_str(),
                &event.target,
                &event.detail,
                &prev.as_slice(),
                &next.as_slice(),
            ],
        )
        .await
        .map_err(backend)?;
        Ok(next)
    }

    async fn lock_chain(conn: &impl GenericClient, org: Uuid) -> Result<(Hash, Hash), StoreFault> {
        let genesis = event_genesis(org);
        conn.execute(
            "INSERT INTO aegis_event_chain (org_id, tail_hash, head_hash)
             VALUES ($1, $2, $2)
             ON CONFLICT (org_id) DO NOTHING",
            &[&org, &genesis.as_slice()],
        )
        .await
        .map_err(backend)?;
        let row = conn
            .query_one(
                "SELECT tail_hash, head_hash FROM aegis_event_chain
                 WHERE org_id = $1 FOR UPDATE",
                &[&org],
            )
            .await
            .map_err(backend)?;
        let tail = hash_of(row.try_get(0).map_err(backend)?)?;
        let head = hash_of(row.try_get(1).map_err(backend)?)?;
        Ok((tail, head))
    }

    pub(super) async fn record(&self, event: &AuditEvent) -> Result<(), StoreFault> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(backend)?;
        let (_tail, head) = Self::lock_chain(&tx, self.org).await?;
        let next = Self::append_chained_event(&tx, self.org, &head, event).await?;
        tx.execute(
            "UPDATE aegis_event_chain SET head_hash = $2 WHERE org_id = $1",
            &[&self.org, &next.as_slice()],
        )
        .await
        .map_err(backend)?;
        tx.commit().await.map_err(backend)
    }

    pub(super) async fn recent_events(&self, limit: usize) -> Result<Vec<AuditEvent>, StoreFault> {
        let client = self.client().await?;
        let limit = to_i64(limit as u64)?;
        let rows = client
            .query(
                "SELECT at_ms, actor, kind, target, detail FROM aegis_event
                 WHERE org_id = $1 ORDER BY event_id DESC LIMIT $2",
                &[&self.org, &limit],
            )
            .await
            .map_err(backend)?;
        rows.iter()
            .filter_map(|row| {
                let kind: String = row.get(2);
                let kind = kind_of(&kind)?;
                Some(Ok(AuditEvent {
                    at_ms: match to_u64(row.get(0)) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    },
                    actor: row.get(1),
                    kind,
                    target: row.get(3),
                    detail: row.get(4),
                }))
            })
            .collect()
    }

    /// Removes events older than `cutoff_ms`, then the oldest beyond `keep`,
    /// advancing the hash-chain tail anchor atomically with each deleted batch
    /// and recording an [`AuditKind::EventsTrimmed`] event in the chain (SEC-06).
    ///
    /// Works in bounded batches from the oldest rows in primary key order.
    /// Every committed batch leaves `(aegis_event_chain, aegis_event)` valid,
    /// so an interrupted trim never corrupts the chain and can be resumed.
    pub(super) async fn trim_events(
        &self,
        cutoff_ms: u64,
        keep: usize,
        now_ms: u64,
    ) -> Result<u64, StoreFault> {
        self.trim_events_bounded(cutoff_ms, keep, now_ms, TRIM_BATCH as usize, None)
            .await
    }

    /// Runs [`trim_events`](Self::trim_events) with an explicit per-transaction
    /// `batch_size` and optional `max_batches` cap (used to bound work or
    /// simulate an interruption after any batch boundary).
    ///
    /// # Errors
    ///
    /// A storage failure or corrupt stored hash.
    pub async fn trim_events_bounded(
        &self,
        cutoff_ms: u64,
        keep: usize,
        now_ms: u64,
        batch_size: usize,
        max_batches: Option<usize>,
    ) -> Result<u64, StoreFault> {
        let batch_limit = to_i64(batch_size.max(1) as u64)?;
        let mut client = self.client().await?;
        let cutoff = to_i64(cutoff_ms)?;
        let cut_row = if keep == 0 {
            client
                .query_opt(
                    "SELECT event_id FROM aegis_event
                     WHERE org_id = $1 ORDER BY event_id DESC LIMIT 1",
                    &[&self.org],
                )
                .await
                .map_err(backend)?
        } else {
            let offset = to_i64((keep - 1) as u64)?;
            client
                .query_opt(
                    "WITH age_cut AS (
                         SELECT max(event_id) AS id FROM aegis_event
                         WHERE org_id = $1 AND at_ms < $2
                     ),
                     min_kept AS (
                         SELECT event_id AS id FROM aegis_event
                         WHERE org_id = $1 AND at_ms >= $2 AND kind <> 'events_trimmed'
                         ORDER BY event_id DESC OFFSET $3 LIMIT 1
                     ),
                     cap_cut AS (
                         SELECT max(e.event_id) AS id
                         FROM aegis_event e, min_kept m
                         WHERE e.org_id = $1 AND e.event_id < m.id
                     ),
                     chosen AS (
                         SELECT GREATEST(
                             (SELECT id FROM age_cut),
                             (SELECT id FROM cap_cut)
                         ) AS id
                     )
                     SELECT e.event_id
                     FROM aegis_event e JOIN chosen c ON e.event_id = c.id
                     WHERE e.org_id = $1",
                    &[&self.org, &cutoff, &offset],
                )
                .await
                .map_err(backend)?
        };
        let Some(cut_row) = cut_row else {
            return Ok(0);
        };
        let cut_id: i64 = cut_row.try_get(0).map_err(backend)?;

        let mut removed = 0u64;
        let mut batches = 0usize;
        let mut interrupted = false;
        loop {
            if max_batches.is_some_and(|limit| batches >= limit) {
                interrupted = true;
                break;
            }
            let tx = client.transaction().await.map_err(backend)?;
            let (mut tail, mut head) = Self::lock_chain(&tx, self.org).await?;
            let row = tx
                .query_one(
                    "WITH victim AS (
                         SELECT event_id, hash
                         FROM aegis_event
                         WHERE org_id = $1 AND event_id <= $2
                         ORDER BY event_id ASC
                         LIMIT $3
                     ),
                     deleted AS (
                         DELETE FROM aegis_event e
                         USING victim v
                         WHERE e.event_id = v.event_id
                         RETURNING v.event_id, v.hash
                     )
                     SELECT count(*)::bigint AS removed,
                            (SELECT hash FROM deleted ORDER BY event_id DESC LIMIT 1) AS last_hash
                     FROM deleted",
                    &[&self.org, &cut_id, &batch_limit],
                )
                .await
                .map_err(backend)?;
            let batch_removed = to_u64(row.try_get(0).map_err(backend)?)?;
            if batch_removed == 0 {
                tx.commit().await.map_err(backend)?;
                break;
            }
            let last_hash_opt: Option<Vec<u8>> = row.try_get(1).map_err(backend)?;
            if let Some(raw) = last_hash_opt {
                let last_hash = hash_of(raw)?;
                if last_hash != [0u8; 32] {
                    if tail == head {
                        head = last_hash;
                    }
                    tail = last_hash;
                    tx.execute(
                        "UPDATE aegis_event_chain SET tail_hash = $2, head_hash = $3 WHERE org_id = $1",
                        &[&self.org, &tail.as_slice(), &head.as_slice()],
                    )
                    .await
                    .map_err(backend)?;
                }
            }
            tx.commit().await.map_err(backend)?;
            removed += batch_removed;
            batches += 1;
            if batch_removed < batch_limit as u64 {
                break;
            }
        }

        if removed == 0 || interrupted {
            return Ok(removed);
        }

        let tx = client.transaction().await.map_err(backend)?;
        let (_tail, head) = Self::lock_chain(&tx, self.org).await?;
        let trim_event = AuditEvent {
            at_ms: now_ms,
            actor: "system".to_owned(),
            kind: AuditKind::EventsTrimmed,
            target: String::new(),
            detail: format!("removed={removed}"),
        };
        let next = Self::append_chained_event(&tx, self.org, &head, &trim_event).await?;
        tx.execute(
            "UPDATE aegis_event_chain SET head_hash = $2 WHERE org_id = $1",
            &[&self.org, &next.as_slice()],
        )
        .await
        .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(removed)
    }

    /// Verifies the event hash chain and its retained-prefix anchor inside one
    /// consistent `REPEATABLE READ` database snapshot.
    ///
    /// # Errors
    ///
    /// A storage failure or corrupt stored row.
    pub async fn verify_events(&self) -> Result<ChainReport, StoreFault> {
        let mut client = self.client().await?;
        let tx = client
            .build_transaction()
            .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .await
            .map_err(backend)?;
        let genesis = event_genesis(self.org);
        let chain_row = tx
            .query_opt(
                "SELECT tail_hash, head_hash FROM aegis_event_chain WHERE org_id = $1",
                &[&self.org],
            )
            .await
            .map_err(backend)?;
        let (tail, head) = match chain_row {
            Some(row) => (
                hash_of(row.try_get(0).map_err(backend)?)?,
                hash_of(row.try_get(1).map_err(backend)?)?,
            ),
            None => (genesis, genesis),
        };
        let rows = tx
            .query(
                "SELECT at_ms, actor, kind, target, detail, prev_hash, hash
                 FROM aegis_event
                 WHERE org_id = $1 AND hash <> $2
                 ORDER BY event_id ASC",
                &[&self.org, &[0u8; 32].as_slice()],
            )
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        let mut verifier = EventChainVerifier::new(tail);
        for row in &rows {
            let kind_str: String = row.try_get(2).map_err(backend)?;
            let Some(kind) = kind_of(&kind_str) else {
                return Err(StoreFault::Corrupt(format!(
                    "unknown audit event kind: {kind_str}"
                )));
            };
            let event = AuditEvent {
                at_ms: to_u64(row.try_get(0).map_err(backend)?)?,
                actor: row.try_get(1).map_err(backend)?,
                kind,
                target: row.try_get(3).map_err(backend)?,
                detail: row.try_get(4).map_err(backend)?,
            };
            let prev = hash_of(row.try_get(5).map_err(backend)?)?;
            let hash = hash_of(row.try_get(6).map_err(backend)?)?;
            verifier.push(&event, &prev, &hash);
        }
        Ok(verifier.finish(&head))
    }
}
