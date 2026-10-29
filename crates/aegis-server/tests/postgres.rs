//! PostgreSQL backend tests.
//!
//! They need a database. Set `AEGIS_TEST_DATABASE_URL` to a connection
//! string (for example `postgres://aegis:aegis@127.0.0.1/aegis_test`); each
//! test works in its own fresh schema (`t_<uuid>`), so tests run in parallel
//! and never see each other's rows. The schemas stay behind; use a
//! throwaway database. Without the variable every test prints a notice and
//! passes, so a plain `cargo test` works anywhere. CI sets the variable.

mod common;

use std::str::FromStr;
use std::sync::Arc;

use aegis_server::ProjectId;
use aegis_server::accounts::{
    AccountBackend, Accounts, AccountsConfig, AuditKind, PostgresAccounts, UserUpdate,
};
use aegis_server::store::{
    BaselineMeta, PostgresOptions, PostgresStore, ProjectMeta, ProjectStore, StoreFault, StoredOp,
    StoredSnapshot,
};
use tokio_postgres::NoTls;
use uuid::Uuid;

/// A store in its own schema, plus a raw connection for tampering.
struct Fresh {
    store: PostgresStore,
    raw: tokio_postgres::Client,
    config: tokio_postgres::Config,
}

async fn fresh() -> Option<Fresh> {
    let Ok(url) = std::env::var("AEGIS_TEST_DATABASE_URL") else {
        eprintln!("AEGIS_TEST_DATABASE_URL is not set: skipping");
        return None;
    };
    let schema = format!("t_{}", Uuid::new_v4().simple());
    let base = tokio_postgres::Config::from_str(&url).expect("url");
    let (admin, conn) = base.connect(NoTls).await.expect("connect");
    tokio::spawn(conn);
    admin
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .await
        .expect("schema");

    let mut config = base.clone();
    config.options(format!("-c search_path={schema}"));
    let store = PostgresStore::connect_config(config.clone(), &PostgresOptions::default())
        .await
        .expect("store");
    let (raw, conn) = config.connect(NoTls).await.expect("raw");
    tokio::spawn(conn);
    Some(Fresh { store, raw, config })
}

fn meta() -> ProjectMeta {
    ProjectMeta {
        id: ProjectId::new_v7(),
        name: "Brake".into(),
        methodology_key: "iso21434-r155".into(),
        methodology_version: "0.1.0".into(),
        created_ms: 1_700_000_000_000,
    }
}

fn op(seq: u64) -> StoredOp {
    StoredOp {
        seq,
        actor: "alice".into(),
        timestamp_ms: 1_700_000_000_000 + seq,
        op: format!("{{\"n\":{seq}}}").into_bytes(),
    }
}

#[tokio::test]
async fn create_list_load_append_snapshot() {
    let Some(f) = fresh().await else { return };
    let m = meta();
    f.store.create(&m, b"snap0").await.expect("create");
    assert!(matches!(
        f.store.create(&m, b"x").await,
        Err(StoreFault::Exists)
    ));
    assert_eq!(f.store.count().await.expect("count"), 1);
    assert_eq!(f.store.list(10).await.expect("list"), vec![m.clone()]);

    f.store
        .append(m.id, &[op(1), op(2), op(3)])
        .await
        .expect("append");
    let loaded = f.store.load(m.id).await.expect("load").expect("exists");
    assert_eq!(loaded.meta, m);
    assert_eq!(
        loaded.snapshot,
        StoredSnapshot {
            seq: 0,
            bytes: b"snap0".to_vec()
        }
    );
    assert_eq!(loaded.ops, vec![op(1), op(2), op(3)]);

    f.store
        .save_snapshot(
            m.id,
            &StoredSnapshot {
                seq: 2,
                bytes: b"snap2".to_vec(),
            },
        )
        .await
        .expect("snapshot");
    let loaded = f.store.load(m.id).await.expect("load").expect("exists");
    assert_eq!(loaded.snapshot.seq, 2);
    assert_eq!(loaded.ops, vec![op(3)]);

    assert!(
        f.store
            .load(ProjectId::new_v7())
            .await
            .expect("load")
            .is_none()
    );
}

#[tokio::test]
async fn append_is_gap_free_and_all_or_nothing() {
    let Some(f) = fresh().await else { return };
    let m = meta();
    f.store.create(&m, b"s").await.expect("create");
    assert!(matches!(
        f.store.append(m.id, &[op(1), op(3)]).await,
        Err(StoreFault::Sequence {
            expected: 2,
            found: 3
        })
    ));
    assert!(
        f.store
            .load(m.id)
            .await
            .expect("load")
            .expect("exists")
            .ops
            .is_empty()
    );
    f.store.append(m.id, &[op(1)]).await.expect("append");
    assert!(matches!(
        f.store.append(m.id, &[op(1)]).await,
        Err(StoreFault::Sequence {
            expected: 2,
            found: 1
        })
    ));
    assert!(matches!(
        f.store.append(ProjectId::new_v7(), &[op(1)]).await,
        Err(StoreFault::Missing)
    ));
    f.store
        .append(m.id, &[])
        .await
        .expect("empty append is a no-op");
}

#[tokio::test]
async fn snapshots_cannot_run_ahead_or_back() {
    let Some(f) = fresh().await else { return };
    let m = meta();
    f.store.create(&m, b"s").await.expect("create");
    f.store.append(m.id, &[op(1)]).await.expect("append");
    let snap = |seq| StoredSnapshot {
        seq,
        bytes: vec![1],
    };
    assert!(matches!(
        f.store.save_snapshot(m.id, &snap(2)).await,
        Err(StoreFault::Sequence {
            expected: 1,
            found: 2
        })
    ));
    f.store.save_snapshot(m.id, &snap(1)).await.expect("ok");
    assert!(f.store.save_snapshot(m.id, &snap(0)).await.is_err());
    assert!(matches!(
        f.store.save_snapshot(ProjectId::new_v7(), &snap(0)).await,
        Err(StoreFault::Missing)
    ));
}

#[tokio::test]
async fn racing_writers_cannot_interleave() {
    let Some(f) = fresh().await else { return };
    let m = meta();
    f.store.create(&m, b"s").await.expect("create");
    // Eight writers all try to append sequence numbers 1..=3.
    let tries: Vec<_> = (0..8)
        .map(|_| {
            let store = f.store.clone();
            let id = m.id;
            tokio::spawn(async move { store.append(id, &[op(1), op(2), op(3)]).await })
        })
        .collect();
    let mut won = 0;
    for t in tries {
        if t.await.expect("task").is_ok() {
            won += 1;
        }
    }
    assert_eq!(won, 1, "exactly one writer wins");
    let report = f.store.verify_chain(m.id).await.expect("verify");
    assert!(report.is_intact());
    assert_eq!(report.verified, 3);
}

#[tokio::test]
async fn a_large_batch_is_one_atomic_append() {
    let Some(f) = fresh().await else { return };
    let m = meta();
    f.store.create(&m, b"s").await.expect("create");
    let ops: Vec<_> = (1..=2000).map(op).collect();
    f.store.append(m.id, &ops).await.expect("append");
    let loaded = f.store.load(m.id).await.expect("load").expect("exists");
    assert_eq!(loaded.ops.len(), 2000);
    assert_eq!(loaded.ops[1999], ops[1999]);
    assert_eq!(
        f.store.verify_chain(m.id).await.expect("verify").verified,
        2000
    );
}

#[tokio::test]
async fn the_audit_chain_catches_tampering_in_the_database() {
    let Some(f) = fresh().await else { return };
    let m = meta();
    f.store.create(&m, b"s").await.expect("create");
    f.store.append(m.id, &[op(1), op(2)]).await.expect("append");
    f.store.append(m.id, &[op(3), op(4)]).await.expect("append");
    let ok = f.store.verify_chain(m.id).await.expect("verify");
    assert!(ok.is_intact());
    assert_eq!(ok.verified, 4);
    assert!(matches!(
        f.store.verify_chain(ProjectId::new_v7()).await,
        Err(StoreFault::Missing)
    ));

    let uuid = m.id.as_uuid();
    // 1. Someone rewrites an actor.
    f.raw
        .execute(
            "UPDATE aegis_project_op SET actor = 'mallory' WHERE project_id = $1 AND seq = 2",
            &[&uuid],
        )
        .await
        .expect("tamper");
    let report = f.store.verify_chain(m.id).await.expect("verify");
    assert_eq!(report.first_bad, Some(2));
    assert_eq!(report.verified, 1);
    f.raw
        .execute(
            "UPDATE aegis_project_op SET actor = 'alice' WHERE project_id = $1 AND seq = 2",
            &[&uuid],
        )
        .await
        .expect("restore");
    assert!(
        f.store
            .verify_chain(m.id)
            .await
            .expect("verify")
            .is_intact()
    );

    // 2. Someone deletes the last op: the project row still claims op 4.
    f.raw
        .execute(
            "DELETE FROM aegis_project_op WHERE project_id = $1 AND seq = 4",
            &[&uuid],
        )
        .await
        .expect("tamper");
    let report = f.store.verify_chain(m.id).await.expect("verify");
    assert_eq!(report.first_bad, Some(4));
    assert_eq!(report.verified, 3);
}

#[tokio::test]
async fn migrations_are_idempotent_and_refuse_newer_databases() {
    let Some(f) = fresh().await else { return };
    // A second server starting against the same schema is fine.
    PostgresStore::connect_config(f.config.clone(), &PostgresOptions::default())
        .await
        .expect("second start");
    let rows = f
        .raw
        .query("SELECT version FROM aegis_schema_migration", &[])
        .await
        .expect("rows");
    assert_eq!(rows.len(), 3);

    // A database migrated by a newer build is refused.
    f.raw
        .execute(
            "INSERT INTO aegis_schema_migration (version, applied_ms) VALUES (999, 0)",
            &[],
        )
        .await
        .expect("insert");
    let err = PostgresStore::connect_config(f.config.clone(), &PostgresOptions::default())
        .await
        .err()
        .expect("refused");
    assert!(err.to_string().contains("newer"), "{err}");
}

#[tokio::test]
async fn projects_of_another_org_are_invisible() {
    let Some(f) = fresh().await else { return };
    let m = meta();
    f.store.create(&m, b"s").await.expect("create");
    let other = PostgresStore::connect_config(
        f.config.clone(),
        &PostgresOptions {
            org_id: Uuid::new_v4(),
            ..PostgresOptions::default()
        },
    )
    .await
    .expect("other org");
    assert!(other.load(m.id).await.expect("load").is_none());
    assert!(other.list(10).await.expect("list").is_empty());
    assert!(matches!(
        other.append(m.id, &[op(1)]).await,
        Err(StoreFault::Missing)
    ));
}

#[tokio::test]
async fn the_server_restarts_over_postgres_with_identical_projects() {
    let Some(f) = fresh().await else { return };
    common::restart_keeps_every_project_identical(Arc::new(f.store)).await;
}

#[tokio::test]
async fn audit_verification_works_through_the_api_on_postgres() {
    let Some(f) = fresh().await else { return };
    common::audit_is_verifiable_by_admins_only(Arc::new(f.store)).await;
}

#[tokio::test]
async fn rename_history_baselines_and_delete() {
    let Some(f) = fresh().await else { return };
    let m = meta();
    f.store.create(&m, b"s0").await.expect("create");
    f.store
        .append(m.id, &[op(1), op(2), op(3)])
        .await
        .expect("append");
    f.store.rename(m.id, "Renamed").await.expect("rename");
    assert_eq!(f.store.list(10).await.expect("list")[0].name, "Renamed");
    let recent = f.store.recent_ops(m.id, 2).await.expect("recent");
    assert_eq!(recent.iter().map(|o| o.seq).collect::<Vec<_>>(), [3, 2]);

    let editors = f
        .store
        .active_editors(1_700_000_000_002)
        .await
        .expect("editors");
    assert_eq!(editors, [("alice".to_owned(), 1_700_000_000_003)]);

    let b = BaselineMeta {
        id: Uuid::now_v7(),
        project: m.id,
        name: "B1".into(),
        seq: 3,
        created_by: "alice".into(),
        created_ms: 5,
        snapshot_bytes: 4,
    };
    f.store
        .create_baseline(&b, b"snap")
        .await
        .expect("baseline");
    assert!(matches!(
        f.store.create_baseline(&b, b"snap").await,
        Err(StoreFault::Exists)
    ));
    assert_eq!(
        f.store.list_baselines(m.id).await.expect("list"),
        std::slice::from_ref(&b)
    );
    let (got, bytes) = f
        .store
        .load_baseline(m.id, b.id)
        .await
        .expect("load")
        .expect("found");
    assert_eq!((got, bytes.as_slice()), (b.clone(), b"snap".as_slice()));
    assert!(
        f.store
            .load_baseline(m.id, Uuid::now_v7())
            .await
            .expect("load")
            .is_none()
    );

    f.store.delete(m.id).await.expect("delete");
    assert!(f.store.load(m.id).await.expect("load").is_none());
    assert!(matches!(
        f.store.delete(m.id).await,
        Err(StoreFault::Missing)
    ));
    assert!(matches!(
        f.store.list_baselines(m.id).await,
        Err(StoreFault::Missing)
    ));
    assert!(matches!(
        f.store.recent_ops(m.id, 5).await,
        Err(StoreFault::Missing)
    ));
}

#[tokio::test]
async fn accounts_survive_a_restart_in_postgres() {
    let Some(f) = fresh().await else { return };
    let backend = || AccountBackend::Postgres(PostgresAccounts::new(f.store.pool(), f.store.org()));
    let accounts = Accounts::open(backend(), AccountsConfig::default())
        .await
        .expect("open");
    accounts
        .bootstrap_admin("root", "root long password")
        .await
        .expect("root");
    accounts
        .create_user("root", "bob", "Bob", "bobs long password", false)
        .await
        .expect("bob");
    let session = accounts
        .log_in("bob", "bobs long password", "laptop")
        .await
        .expect("login");
    let gone = accounts
        .log_in("root", "root long password", "")
        .await
        .expect("login");
    accounts.log_out(&gone.token).await.expect("logout");

    // A new process: same users, the open session still works, the closed one does not.
    let again = Accounts::open(backend(), AccountsConfig::default())
        .await
        .expect("reopen");
    assert_eq!(again.user_count(), 2);
    let p = again
        .authenticate(&session.token)
        .expect("session survives");
    assert_eq!(&*p.subject, "bob");
    assert!(again.authenticate(&gone.token).is_none());
    again
        .update_user(
            "root",
            "bob",
            UserUpdate {
                disabled: Some(true),
                ..UserUpdate::default()
            },
        )
        .await
        .expect("disable");
    let third = Accounts::open(backend(), AccountsConfig::default())
        .await
        .expect("reopen");
    assert!(
        third.authenticate(&session.token).is_none(),
        "disabling ended the session"
    );
    third.delete_user("root", "bob").await.expect("delete");
    assert_eq!(third.user_count(), 1);

    let kinds: Vec<AuditKind> = third
        .recent_events(20)
        .await
        .expect("events")
        .iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds.first(), Some(&AuditKind::UserDeleted));
    assert!(kinds.contains(&AuditKind::LoginOk) && kinds.contains(&AuditKind::Logout));
    // Nothing secret is stored in the clear.
    let rows = f
        .raw
        .query("SELECT encode(token_hash, 'hex') FROM aegis_session", &[])
        .await
        .expect("rows");
    for row in rows {
        let hash: String = row.get(0);
        assert!(!session.token.contains(&hash));
    }
    let hashes = f
        .raw
        .query("SELECT password_hash FROM aegis_user", &[])
        .await
        .expect("rows");
    for row in hashes {
        let h: String = row.get(0);
        assert!(h.starts_with("$argon2id$"));
    }
}

#[tokio::test]
async fn tls_encrypt_is_refused_by_a_server_without_tls() {
    let Some(f) = fresh().await else { return };
    let err = PostgresStore::connect_config(
        f.config.clone(),
        &PostgresOptions {
            tls: aegis_server::store::DbTls::Encrypt,
            ..PostgresOptions::default()
        },
    )
    .await
    .err()
    .expect("a plain server cannot do TLS");
    assert!(matches!(err, StoreFault::Backend(_)));
}

/// Needs a database with TLS on and a self-signed certificate, at
/// `AEGIS_TEST_TLS_DATABASE_URL`; skipped without it.
#[tokio::test]
async fn tls_encrypt_and_verify_refuse_an_untrusted_certificate_unless_ca_file_is_set() {
    let Ok(url) = std::env::var("AEGIS_TEST_TLS_DATABASE_URL") else {
        eprintln!("AEGIS_TEST_TLS_DATABASE_URL is not set: skipping");
        return;
    };
    let options = |tls, ca_file| PostgresOptions {
        tls,
        ca_file,
        ..PostgresOptions::default()
    };
    for tls in [
        aegis_server::store::DbTls::Encrypt,
        aegis_server::store::DbTls::Verify,
    ] {
        let refused = PostgresStore::connect(&url, &options(tls, None))
            .await
            .err()
            .expect("the certificate is not trusted without a CA file");
        assert!(matches!(refused, StoreFault::Backend(_)));
    }
    if let Ok(ca) = std::env::var("AEGIS_TEST_TLS_CA_FILE") {
        let store = PostgresStore::connect(
            &url,
            &options(
                aegis_server::store::DbTls::Encrypt,
                Some(std::path::PathBuf::from(ca)),
            ),
        )
        .await
        .expect("encrypted connection with configured CA file");
        assert!(store.count().await.is_ok());
    }
}

#[tokio::test]
async fn the_password_login_switch_survives_a_restart_in_postgres() {
    let Some(f) = fresh().await else { return };
    let backend = || AccountBackend::Postgres(PostgresAccounts::new(f.store.pool(), f.store.org()));
    let config = || AccountsConfig {
        company_login: true,
        ..AccountsConfig::default()
    };
    let accounts = Accounts::open(backend(), config()).await.expect("open");
    assert!(
        accounts.password_login_enabled(),
        "on until an admin says otherwise"
    );
    accounts
        .set_password_login("carol", false)
        .await
        .expect("off");

    let again = Accounts::open(backend(), config()).await.expect("reopen");
    assert!(
        !again.password_login_enabled(),
        "off stays off after a restart"
    );
    again.set_password_login("carol", true).await.expect("on");
    let third = Accounts::open(backend(), config()).await.expect("reopen");
    assert!(third.password_login_enabled());

    let events = third.recent_events(10).await.expect("events");
    let kinds: Vec<_> = events.iter().map(|e| (e.kind, e.detail.as_str())).collect();
    assert_eq!(
        kinds,
        [
            (AuditKind::PasswordLoginChanged, "on"),
            (AuditKind::PasswordLoginChanged, "off")
        ]
    );
}

/// The event log keeps a bounded number of rows (job 052): old events go in
/// batches, then anything beyond the row cap.
#[tokio::test]
async fn the_event_log_is_trimmed_by_age_and_by_row_cap_in_postgres() {
    use aegis_server::accounts::{EventLog, Retention};
    use std::time::Duration;

    let Some(f) = fresh().await else { return };
    let log = EventLog::standalone(AccountBackend::Postgres(PostgresAccounts::new(
        f.store.pool(),
        f.store.org(),
    )));
    let org = f.store.org();
    let now = log.now_ms();
    let day = 86_400_000u64;
    // 12 000 events 400 days old (more than two delete batches), then 50 young
    // ones, written the fast way.
    f.raw
        .execute(
            "INSERT INTO aegis_event (org_id, at_ms, actor, kind, target, detail)
             SELECT $1, $2::bigint + g, 'old', 'export', 'p', 'pdf' FROM generate_series(1, 12000) g",
            &[&org, &i64::try_from(now - 400 * day).expect("fits")],
        )
        .await
        .expect("old");
    for i in 0..50 {
        log.record("young", AuditKind::Export, &format!("p{i}"), "pdf")
            .await;
    }

    let removed = log
        .trim(Retention {
            max_age: Duration::from_secs(365 * 86_400),
            max_events: 1_000_000,
        })
        .await
        .expect("trim by age");
    assert_eq!(removed, 12_000);
    // 50 young events + 1 EventsTrimmed checkpoint event.
    assert_eq!(log.recent(1000).await.expect("recent").len(), 51);
    assert_eq!(log.verify_chain().await.expect("chain").first_bad, None);

    // Nothing old: a second pass removes nothing.
    let again = log
        .trim(Retention {
            max_age: Duration::from_secs(365 * 86_400),
            max_events: 1_000_000,
        })
        .await
        .expect("again");
    assert_eq!(again, 0);

    // The row cap keeps the newest ten non-trim events plus the trim event.
    let removed = log
        .trim(Retention {
            max_age: Duration::from_secs(365 * 86_400),
            max_events: 10,
        })
        .await
        .expect("trim by cap");
    assert_eq!(removed, 41);
    let left = log.recent(1000).await.expect("recent");
    assert_eq!(left.len(), 11);
    assert_eq!(left[0].kind, AuditKind::EventsTrimmed);
    assert_eq!(left[1].target, "p49");
    assert_eq!(left[10].target, "p40");
    assert_eq!(log.verify_chain().await.expect("chain").first_bad, None);

    // A cap above the row count removes nothing.
    let none = log
        .trim(Retention {
            max_age: Duration::from_secs(365 * 86_400),
            max_events: 1000,
        })
        .await
        .expect("no-op");
    assert_eq!(none, 0);
}

/// Stress (job 052): many writers and a trimmer at once. The size is
/// `AEGIS_STRESS_EVENTS` events in total (default 4000); CI keeps the
/// default, a local run can use 200000 or more. Nothing may fail, no event
/// may be lost while the cap is not reached, and the cap must hold at the end.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_many_writers_and_a_trimmer_keep_the_event_log_bounded() {
    use aegis_server::accounts::{EventLog, Retention};
    use std::time::{Duration, Instant};

    let Some(f) = fresh().await else { return };
    let total: usize = std::env::var("AEGIS_STRESS_EVENTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4000);
    let writers = 16;
    let per_writer = total / writers;
    let log = EventLog::standalone(AccountBackend::Postgres(PostgresAccounts::new(
        f.store.pool(),
        f.store.org(),
    )));

    // Phase 1: writers only. Every event must arrive.
    let started = Instant::now();
    let mut tasks = Vec::new();
    for w in 0..writers {
        let log = log.clone();
        tasks.push(tokio::spawn(async move {
            for i in 0..per_writer {
                log.record(
                    &format!("user{w}"),
                    AuditKind::Export,
                    &format!("p{i}"),
                    "pdf",
                )
                .await;
            }
        }));
    }
    for t in tasks {
        t.await.expect("writer");
    }
    let written = writers * per_writer;
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "event log stress: {written} events in {secs:.2} s ({:.0} per second)",
        written as f64 / secs
    );
    let count: i64 = f
        .raw
        .query_one("SELECT count(*) FROM aegis_event", &[])
        .await
        .expect("count")
        .get(0);
    assert_eq!(count as usize, written, "no event may be lost");

    // Phase 2: writers and a trimmer together; the cap holds at the end.
    let cap = (written / 4).max(1000);
    let retention = Retention {
        max_age: Duration::from_secs(3600),
        max_events: cap,
    };
    let trimmer = {
        let log = log.clone();
        tokio::spawn(async move {
            for _ in 0..20 {
                log.trim(retention).await.expect("trim under load");
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
    };
    let mut tasks = Vec::new();
    for w in 0..writers {
        let log = log.clone();
        tasks.push(tokio::spawn(async move {
            for i in 0..per_writer / 4 {
                log.record(
                    &format!("user{w}"),
                    AuditKind::Export,
                    &format!("q{i}"),
                    "pdf",
                )
                .await;
            }
        }));
    }
    for t in tasks {
        t.await.expect("writer");
    }
    trimmer.await.expect("trimmer");
    log.trim(retention).await.expect("final trim");
    let count: i64 = f
        .raw
        .query_one("SELECT count(*) FROM aegis_event", &[])
        .await
        .expect("count")
        .get(0);
    assert!(
        (count as usize) <= cap + 1,
        "the row cap holds (with at most one trailing trim checkpoint event)"
    );
    assert_eq!(log.verify_chain().await.expect("chain").first_bad, None);
    // The newest events survive.
    let newest = log.recent(1).await.expect("recent");
    assert_eq!(newest.len(), 1);
}

/// An ended session stays ended after a restart (job 055).
#[tokio::test]
async fn an_ended_session_stays_ended_after_a_restart_in_postgres() {
    let Some(f) = fresh().await else { return };
    let backend = || AccountBackend::Postgres(PostgresAccounts::new(f.store.pool(), f.store.org()));
    let accounts = Accounts::open(backend(), AccountsConfig::default())
        .await
        .expect("open");
    accounts
        .bootstrap_admin("root", "root long password")
        .await
        .expect("root");
    accounts
        .create_user("root", "bob", "", "bobs long password", false)
        .await
        .expect("bob");
    let keep = accounts
        .log_in("bob", "bobs long password", "keeps")
        .await
        .expect("keep");
    let end = accounts
        .log_in("bob", "bobs long password", "ends")
        .await
        .expect("end");
    let id = accounts
        .list_sessions()
        .iter()
        .find(|s| s.client_name == "ends")
        .map(|s| s.key.id())
        .expect("listed");
    accounts.revoke_session("root", &id).await.expect("revoke");
    assert!(accounts.authenticate(&end.token).is_none());

    let again = Accounts::open(backend(), AccountsConfig::default())
        .await
        .expect("reopen");
    assert!(again.authenticate(&end.token).is_none(), "still ended");
    assert!(again.authenticate(&keep.token).is_some(), "the other stays");
    assert_eq!(again.list_sessions().len(), 1);
    let kinds: Vec<AuditKind> = again
        .recent_events(10)
        .await
        .expect("events")
        .iter()
        .map(|e| e.kind)
        .collect();
    assert!(kinds.contains(&AuditKind::SessionRevoked));
}
