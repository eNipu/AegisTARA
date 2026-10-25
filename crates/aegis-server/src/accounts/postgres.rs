//! PostgreSQL account storage (`migrations/002_accounts_baselines.sql`).
//!
//! Shares the connection pool of [`PostgresStore`](crate::PostgresStore),
//! which also runs the migrations. Every write is one statement, so each is
//! atomic on its own; [`Accounts`](super::Accounts) orders them so a crash
//! between two leaves at worst an orphan session row, which is refused
//! (its user check fails) and purged on expiry.

use deadpool_postgres::Pool;
use tokio_postgres::Row;
use uuid::Uuid;

use super::{AuditEvent, AuditKind, LoadedAccounts, SessionKey, SessionRecord, UserRecord};
use crate::store::StoreFault;

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
    use AuditKind as K;
    [
        K::LoginOk,
        K::LoginFailed,
        K::Logout,
        K::PasswordChanged,
        K::UserCreated,
        K::UserUpdated,
        K::UserDeleted,
        K::Export,
        K::ProjectDeleted,
        K::BaselineCreated,
        K::PasswordLoginChanged,
    ]
    .into_iter()
    .find(|k| k.as_str() == text)
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
        client
            .execute(
                "INSERT INTO aegis_setting (org_id, key, value) VALUES ($1, 'password_login', $2)
                 ON CONFLICT (org_id, key) DO UPDATE SET value = EXCLUDED.value",
                &[&self.org, &if enabled { "on" } else { "off" }],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    pub(super) async fn put_user(&self, user: &UserRecord) -> Result<(), StoreFault> {
        let client = self.client().await?;
        client
            .execute(
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
                    &self.org,
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
        client
            .execute(
                "INSERT INTO aegis_session
                     (token_hash, org_id, username, created_ms, expires_ms, client_name)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &session.key.0.as_slice(),
                    &self.org,
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

    pub(super) async fn delete_sessions(&self, keys: &[SessionKey]) -> Result<(), StoreFault> {
        if keys.is_empty() {
            return Ok(());
        }
        let client = self.client().await?;
        let hashes: Vec<&[u8]> = keys.iter().map(|k| k.0.as_slice()).collect();
        client
            .execute(
                "DELETE FROM aegis_session WHERE org_id = $1 AND token_hash = ANY($2::bytea[])",
                &[&self.org, &hashes],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    pub(super) async fn record(&self, event: &AuditEvent) -> Result<(), StoreFault> {
        let client = self.client().await?;
        client
            .execute(
                "INSERT INTO aegis_event (org_id, at_ms, actor, kind, target, detail)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &self.org,
                    &to_i64(event.at_ms)?,
                    &event.actor,
                    &event.kind.as_str(),
                    &event.target,
                    &event.detail,
                ],
            )
            .await
            .map_err(backend)?;
        Ok(())
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
}
