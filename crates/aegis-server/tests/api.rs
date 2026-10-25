//! End-to-end tests: a real gRPC server on a loopback port, real clients.

mod common;

use std::time::Duration;

use aegis_model::{AssetId, Op, OpCodec, Provenance};
use aegis_server::config::ServerConfig;
use aegis_server::error::{CURRENT_SEQ_HEADER, ERROR_CODE_HEADER};
use aegis_server::pb;
use aegis_server::store::ProjectStore;
use bytes::Bytes;
use common::{
    METHODOLOGY, TestServer, create_project, expected_project, fast_config, fixture, lock,
    methodology, open, submit,
};
use tonic::Code;

fn error_code(status: &tonic::Status) -> &str {
    status
        .metadata()
        .get(ERROR_CODE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

#[tokio::test]
async fn server_info_lists_the_engine_and_methodologies() {
    let server = TestServer::start(fast_config()).await;
    let mut alice = server.client("alice").await;

    let info = alice
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .expect("info")
        .into_inner();
    assert_eq!(info.engine_version, aegis_model::ENGINE_VERSION);
    assert_eq!(info.lease_ms, 60_000);
    assert_eq!(info.methodologies.len(), 1);
    assert_eq!(info.methodologies[0].key, METHODOLOGY);

    let pack = alice
        .get_methodology(pb::GetMethodologyRequest {
            key: METHODOLOGY.into(),
        })
        .await
        .expect("pack")
        .into_inner();
    let m = aegis_model::Methodology::read_pack(pack.pack.as_ref()).expect("valid pack");
    assert_eq!(m.key(), METHODOLOGY);

    let missing = alice
        .get_methodology(pb::GetMethodologyRequest { key: "nope".into() })
        .await
        .expect_err("unknown");
    assert_eq!(error_code(&missing), "unknown_methodology");
    server.stop().await;
}

#[tokio::test]
async fn calls_without_credentials_are_refused() {
    let server = TestServer::start(fast_config()).await;
    let channel = tonic::transport::Endpoint::from_shared(format!("http://{}", server.addr))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut anonymous = pb::aegis_client::AegisClient::new(channel);
    let err = anonymous
        .list_projects(pb::ListProjectsRequest {})
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::Unauthenticated);
    server.stop().await;
}

#[tokio::test]
async fn projects_are_created_listed_and_opened_empty() {
    let server = TestServer::start(fast_config()).await;
    let mut alice = server.client("alice").await;
    let m = methodology();

    let id = create_project(&mut alice, "  Brake system  ").await;
    let listed = alice
        .list_projects(pb::ListProjectsRequest {})
        .await
        .expect("list")
        .into_inner()
        .projects;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].project_id, id);
    assert_eq!(listed[0].name, "Brake system");
    assert_eq!(listed[0].created_ms, 1_700_000_000_000);

    let (seq, project) = open(&mut alice, &id, &m).await;
    assert_eq!(seq, 0);
    assert_eq!(project, aegis_model::Project::new(m));

    for (name, key, code) in [
        ("", METHODOLOGY, "invalid_request"),
        ("x\u{7}y", METHODOLOGY, "invalid_request"),
        ("p", "nope", "unknown_methodology"),
    ] {
        let err = alice
            .create_project(pb::CreateProjectRequest {
                name: name.into(),
                methodology_key: key.into(),
            })
            .await
            .expect_err("refused");
        assert_eq!(err.code(), Code::InvalidArgument, "{name:?}");
        assert_eq!(error_code(&err), code);
    }

    let unknown = alice
        .open_project(pb::OpenProjectRequest {
            project_id: "0198a3c2-0000-7000-8000-000000000000".into(),
        })
        .await
        .expect_err("unknown");
    assert_eq!(unknown.code(), Code::NotFound);
    let bad = alice
        .open_project(pb::OpenProjectRequest {
            project_id: "not-a-uuid".into(),
        })
        .await
        .expect_err("bad id");
    assert_eq!(error_code(&bad), "bad_project_id");
    server.stop().await;
}

#[tokio::test]
async fn ops_are_applied_stored_and_visible_to_every_client() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let mut bob = server.client("bob").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);

    let lease = lock(&mut alice, &id).await;
    assert_eq!(lease.holder, "alice");
    let last = submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");
    assert_eq!(last, fx.ops.len() as u64);

    // Bob, who never held the lock, reads the same project the server holds.
    let (seq, project) = open(&mut bob, &id, &m).await;
    assert_eq!(seq, last);
    assert_eq!(project, expected_project(&m, &fx.ops));

    // The store holds canonical ops with the actor and time.
    let stored = server.store.history(id.parse().expect("id"));
    assert_eq!(stored.len(), fx.ops.len());
    assert_eq!(stored[0].actor, "alice");
    assert_eq!(stored[0].timestamp_ms, 1_700_000_000_000);
    assert_eq!(stored[0].seq, 1);
    assert_eq!(codec.decode(&stored[0].op).expect("decodes"), fx.ops[0]);
    server.stop().await;
}

#[tokio::test]
async fn a_lease_reports_the_sequence_number_it_was_granted_at() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    assert_eq!(lease.last_seq, 0);
    submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");
    let renewed = alice
        .renew_lock(pb::RenewLockRequest {
            project_id: id.clone(),
            token: lease.token.clone(),
        })
        .await
        .expect("renew")
        .into_inner();
    assert_eq!(renewed.last_seq, fx.ops.len() as u64);
    // A client that last saw an older sequence knows it must resync.
    let again = lock(&mut alice, &id).await;
    assert_eq!(again.last_seq, fx.ops.len() as u64);
    server.stop().await;
}

#[tokio::test]
async fn only_the_lease_holder_may_submit() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let mut bob = server.client("bob").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);

    // No lease at all.
    let nobody = pb::Lease {
        token: "0".repeat(32),
        ..pb::Lease::default()
    };
    let err = submit(&mut alice, &id, &nobody, 0, &fx.ops[..1], &codec)
        .await
        .expect_err("not locked");
    assert_eq!(error_code(&err), "not_locked");

    let lease = lock(&mut alice, &id).await;
    // Bob cannot take it, and Alice's token is useless in his hands.
    let err = bob
        .acquire_lock(pb::LockRequest {
            project_id: id.clone(),
        })
        .await
        .expect_err("held");
    assert_eq!(err.code(), Code::FailedPrecondition);
    assert_eq!(error_code(&err), "lock_held");
    assert!(err.message().contains("alice"));
    let err = submit(&mut bob, &id, &lease, 0, &fx.ops[..1], &codec)
        .await
        .expect_err("not his token");
    assert_eq!(error_code(&err), "lock_invalid");

    // Release hands the project over.
    alice
        .release_lock(pb::ReleaseLockRequest {
            project_id: id.clone(),
            token: lease.token.clone(),
        })
        .await
        .expect("release");
    let bobs = lock(&mut bob, &id).await;
    assert_eq!(bobs.holder, "bob");
    let err = submit(&mut alice, &id, &lease, 0, &fx.ops[..1], &codec)
        .await
        .expect_err("released");
    assert_eq!(error_code(&err), "lock_invalid");
    submit(&mut bob, &id, &bobs, 0, &fx.ops[..1], &codec)
        .await
        .expect("bob edits");
    server.stop().await;
}

#[tokio::test]
async fn admins_break_locks_and_others_cannot() {
    let server = TestServer::start(fast_config()).await;
    let mut alice = server.client("alice").await;
    let mut bob = server.client("bob").await;
    let mut root = server.admin("root").await;
    let id = create_project(&mut alice, "p").await;
    lock(&mut alice, &id).await;

    let err = bob
        .break_lock(pb::LockRequest {
            project_id: id.clone(),
        })
        .await
        .expect_err("not admin");
    assert_eq!(err.code(), Code::PermissionDenied);

    root.break_lock(pb::LockRequest {
        project_id: id.clone(),
    })
    .await
    .expect("admin breaks");
    assert_eq!(lock(&mut bob, &id).await.holder, "bob");
    server.stop().await;
}

#[tokio::test]
async fn a_lease_expires_unless_renewed() {
    let config = ServerConfig {
        lease: Duration::from_millis(400),
        ..fast_config()
    };
    let server = TestServer::start(config).await;
    let mut alice = server.client("alice").await;
    let mut bob = server.client("bob").await;
    let id = create_project(&mut alice, "p").await;

    let lease = lock(&mut alice, &id).await;
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        alice
            .renew_lock(pb::RenewLockRequest {
                project_id: id.clone(),
                token: lease.token.clone(),
            })
            .await
            .expect("renewed in time");
    }
    // Stop renewing: the lease runs out and Bob can take over.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let err = alice
        .renew_lock(pb::RenewLockRequest {
            project_id: id.clone(),
            token: lease.token.clone(),
        })
        .await
        .expect_err("expired");
    // Whether the actor's timer already swept the lease decides which of
    // the two a client sees; both mean "acquire again".
    assert!(["lock_invalid", "not_locked"].contains(&error_code(&err)));
    assert_eq!(lock(&mut bob, &id).await.holder, "bob");
    server.stop().await;
}

#[tokio::test]
async fn a_batch_is_atomic() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    submit(&mut alice, &id, &lease, 0, &fx.ops[..2], &codec)
        .await
        .expect("first two");
    let (_, before) = open(&mut alice, &id, &m).await;

    // Valid op, valid op, then one that refers to an unknown asset.
    let mut batch = vec![
        Op::AddAsset {
            id: AssetId::new_v7(),
            name: "Gateway".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
        Op::RenameAsset {
            id: fx.asset,
            name: "Brake control unit".into(),
            provenance: Provenance::default(),
        },
        Op::RenameAsset {
            id: AssetId::new_v7(),
            name: "ghost".into(),
            provenance: Provenance::default(),
        },
    ];
    let err = submit(&mut alice, &id, &lease, 2, &batch, &codec)
        .await
        .expect_err("third op is invalid");
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_eq!(error_code(&err), "rejected_op");
    assert!(err.message().starts_with("op 2 "), "{}", err.message());

    // Nothing changed in memory, in the store, or in the sequence.
    let (seq, after) = open(&mut alice, &id, &m).await;
    assert_eq!(seq, 2);
    assert_eq!(after, before);
    assert_eq!(server.store.history(id.parse().expect("id")).len(), 2);

    // The same batch without the bad op goes through.
    batch.pop();
    let last = submit(&mut alice, &id, &lease, 2, &batch, &codec)
        .await
        .expect("accepted");
    assert_eq!(last, 4);
    server.stop().await;
}

#[tokio::test]
async fn bad_requests_are_refused_with_reasons() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    submit(&mut alice, &id, &lease, 0, &fx.ops[..1], &codec)
        .await
        .expect("one op");

    // Built on an old state.
    let err = submit(&mut alice, &id, &lease, 0, &fx.ops[1..2], &codec)
        .await
        .expect_err("stale");
    assert_eq!(error_code(&err), "stale_base");
    assert_eq!(
        err.metadata().get(CURRENT_SEQ_HEADER).map(|v| v.as_bytes()),
        Some(&b"1"[..])
    );

    let raw = |ops: Vec<Bytes>| pb::SubmitOpsRequest {
        project_id: id.clone(),
        lock_token: lease.token.clone(),
        base_seq: 1,
        ops,
    };
    // Malformed op: position only, never the text.
    let err = alice
        .submit_ops(raw(vec![Bytes::from_static(
            b"{\"op\":\"nope\",\"secret\":\"x\"}",
        )]))
        .await
        .expect_err("malformed");
    assert_eq!(error_code(&err), "malformed_op");
    assert!(!err.message().contains("secret"), "{}", err.message());
    // Empty batch.
    let err = alice.submit_ops(raw(vec![])).await.expect_err("empty");
    assert_eq!(error_code(&err), "invalid_request");
    // Too many ops.
    let too_many = vec![Bytes::from_static(b"{}"); fast_config().max_batch_ops + 1];
    let err = alice.submit_ops(raw(too_many)).await.expect_err("too many");
    assert_eq!(error_code(&err), "invalid_request");
    // Oversized op (R3).
    let huge = Bytes::from(vec![b' '; aegis_model::MAX_OP_RECORD_BYTES + 1]);
    let err = alice.submit_ops(raw(vec![huge])).await.expect_err("huge");
    assert_eq!(error_code(&err), "malformed_op");
    // Nothing of that got in.
    let (seq, _) = open(&mut alice, &id, &m).await;
    assert_eq!(seq, 1);
    server.stop().await;
}

#[tokio::test]
async fn observers_follow_the_editor_live() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let mut bob = server.client("bob").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);

    let mut watch = bob
        .watch(pb::WatchRequest {
            project_id: id.clone(),
            after_seq: 0,
        })
        .await
        .expect("watch")
        .into_inner();
    use pb::watch_event::Event;
    // First event: who holds the lock now (nobody).
    let first = watch.message().await.expect("ok").expect("event");
    assert!(matches!(first.event, Some(Event::Lock(l)) if l.holder.is_empty()));
    // Then the risks as of now: an empty project has none.
    let event = watch.message().await.expect("ok").expect("event");
    assert!(
        matches!(event.event, Some(Event::Risks(r)) if r.as_of_seq == 0 && r.threats.is_empty())
    );

    let lease = lock(&mut alice, &id).await;
    let event = watch.message().await.expect("ok").expect("event");
    assert!(matches!(event.event, Some(Event::Lock(l)) if l.holder == "alice"));

    submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");
    for (i, op) in fx.ops.iter().enumerate() {
        let event = watch.message().await.expect("ok").expect("event");
        let Some(Event::Op(accepted)) = event.event else {
            panic!("expected an op event");
        };
        assert_eq!(accepted.seq, i as u64 + 1);
        assert_eq!(accepted.actor, "alice");
        assert_eq!(&codec.decode(&accepted.op).expect("decodes"), op);
    }
    // One risks event per batch, after its ops, equal to what GetRisks says.
    let event = watch.message().await.expect("ok").expect("event");
    let Some(Event::Risks(risks)) = event.event else {
        panic!("expected the risks after the batch");
    };
    assert_eq!(risks.as_of_seq, fx.ops.len() as u64);
    let asked = alice
        .get_risks(pb::GetRisksRequest {
            project_id: id.clone(),
        })
        .await
        .expect("risks")
        .into_inner();
    assert_eq!(risks.threats, asked.threats);
    assert_eq!(risks.threats.len(), 1);
    assert!(
        !risks.threats[0].inherent_risk.is_empty(),
        "the fixture is rated"
    );

    alice
        .release_lock(pb::ReleaseLockRequest {
            project_id: id.clone(),
            token: lease.token.clone(),
        })
        .await
        .expect("release");
    let event = watch.message().await.expect("ok").expect("event");
    assert!(matches!(event.event, Some(Event::Lock(l)) if l.holder.is_empty()));
    server.stop().await;
}

#[tokio::test]
async fn a_new_observer_gets_the_current_risks_after_the_backlog() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");

    let mut watch = alice
        .watch(pb::WatchRequest {
            project_id: id.clone(),
            after_seq: 3,
        })
        .await
        .expect("watch")
        .into_inner();
    use pb::watch_event::Event;
    let mut order = Vec::new();
    for _ in 0..4 {
        let event = watch.message().await.expect("ok").expect("event");
        order.push(match event.event {
            Some(Event::Lock(_)) => "lock".to_owned(),
            Some(Event::Op(op)) => format!("op{}", op.seq),
            Some(Event::Risks(r)) => format!("risks@{}", r.as_of_seq),
            None => "none".to_owned(),
        });
    }
    assert_eq!(order, ["lock", "op4", "op5", "risks@5"]);
    server.stop().await;
}

#[tokio::test]
async fn a_project_over_the_threat_limit_sends_ops_but_no_risks() {
    let config = ServerConfig {
        risks_event_max_threats: 0,
        ..fast_config()
    };
    let server = TestServer::start(config).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    // The project now has one threat scenario, which is over the limit of 0.
    let seq = submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");
    let mut watch = alice
        .watch(pb::WatchRequest {
            project_id: id.clone(),
            after_seq: seq,
        })
        .await
        .expect("watch")
        .into_inner();
    use pb::watch_event::Event;
    let first = watch.message().await.expect("ok").expect("event");
    assert!(matches!(first.event, Some(Event::Lock(_))));

    let rename = Op::RenameAsset {
        id: fx.asset,
        name: "Renamed".into(),
        provenance: Provenance::default(),
    };
    submit(&mut alice, &id, &lease, seq, &[rename], &codec)
        .await
        .expect("accepted");
    let next = watch.message().await.expect("ok").expect("event");
    assert!(matches!(next.event, Some(Event::Op(op)) if op.seq == seq + 1));
    // No risks event follows.
    let more = tokio::time::timeout(Duration::from_millis(300), watch.message()).await;
    assert!(more.is_err(), "no further event: {more:?}");
    // The pull call still works for such a project.
    assert!(
        alice
            .get_risks(pb::GetRisksRequest { project_id: id })
            .await
            .is_ok()
    );
    server.stop().await;
}

#[tokio::test]
async fn a_late_observer_catches_up_from_the_recent_ops() {
    let config = ServerConfig {
        recent_ops: 3,
        ..fast_config()
    };
    let server = TestServer::start(config).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("5 ops");

    let watch = |after: u64| {
        let mut c = alice.clone();
        let id = id.clone();
        async move {
            c.watch(pb::WatchRequest {
                project_id: id,
                after_seq: after,
            })
            .await
        }
    };
    // The ring holds ops 3..=5, so resuming after 2 works...
    let mut stream = watch(2).await.expect("in range").into_inner();
    let _lock_state = stream.message().await.expect("ok").expect("lock");
    let mut seqs = Vec::new();
    for _ in 0..3 {
        let event = stream.message().await.expect("ok").expect("op");
        if let Some(pb::watch_event::Event::Op(op)) = event.event {
            seqs.push(op.seq);
        }
    }
    assert_eq!(seqs, vec![3, 4, 5]);
    // ...resuming at the head works...
    assert!(watch(5).await.is_ok());
    // ...and anything older, or in the future, must reopen the project.
    for after in [1, 6] {
        let err = watch(after).await.expect_err("out of range");
        assert_eq!(err.code(), Code::OutOfRange, "after {after}");
    }
    server.stop().await;
}

#[tokio::test]
async fn an_idle_observer_never_slows_the_editor() {
    // A tiny observer buffer and an observer that reads nothing. The editor
    // must not wait for it (R4: the observer is dropped or buffered, the
    // editor is never blocked). Lag handling itself is unit tested in
    // `service::tests`, because HTTP/2 flow control hides a slow reader on
    // a loopback socket.
    let config = ServerConfig {
        observer_buffer: 2,
        ..fast_config()
    };
    let server = TestServer::start(config).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let lease = lock(&mut alice, &id).await;
    let mut idle = alice
        .watch(pb::WatchRequest {
            project_id: id.clone(),
            after_seq: 0,
        })
        .await
        .expect("watch")
        .into_inner();

    let asset = AssetId::new_v7();
    let add = Op::AddAsset {
        id: asset,
        name: "A".into(),
        properties: methodology().all_properties(),
        provenance: Provenance::default(),
    };
    let mut seq = submit(&mut alice, &id, &lease, 0, &[add], &codec)
        .await
        .expect("add");
    let started = std::time::Instant::now();
    for i in 0..30 {
        let rename = Op::RenameAsset {
            id: asset,
            name: format!("A{i}").into(),
            provenance: Provenance::default(),
        };
        seq = submit(&mut alice, &id, &lease, seq, &[rename], &codec)
            .await
            .expect("rename");
    }
    assert_eq!(seq, 31);
    assert!(started.elapsed() < Duration::from_secs(10));
    // The observer still gets a consistent, ordered prefix of events.
    let mut last = 0;
    while let Ok(Ok(Some(event))) =
        tokio::time::timeout(Duration::from_millis(500), idle.message()).await
    {
        if let Some(pb::watch_event::Event::Op(op)) = event.event {
            assert_eq!(op.seq, last + 1);
            last = op.seq;
        }
    }
    assert!(last > 0);
    server.stop().await;
}

#[tokio::test]
async fn watch_streams_outlive_the_call_timeout() {
    // The transport's call timeout covers unary calls, not stream bodies.
    let server = TestServer::start(fast_config()).await;
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let mut watch = alice
        .watch(pb::WatchRequest {
            project_id: id,
            after_seq: 0,
        })
        .await
        .expect("watch")
        .into_inner();
    let _ = watch.message().await.expect("ok").expect("lock state");
    let _ = watch.message().await.expect("ok").expect("risks state");
    // Nothing happens for longer than a short timeout would allow.
    let waited = tokio::time::timeout(Duration::from_millis(300), watch.message()).await;
    assert!(waited.is_err(), "stream is still open and quiet");
    server.stop().await;
}

#[tokio::test]
async fn projects_survive_a_server_restart_identically() {
    common::restart_keeps_every_project_identical(std::sync::Arc::new(
        aegis_server::MemoryStore::new(),
    ))
    .await;
}

#[tokio::test]
async fn audit_is_verifiable_by_admins_only() {
    common::audit_is_verifiable_by_admins_only(std::sync::Arc::new(
        aegis_server::MemoryStore::new(),
    ))
    .await;
}

#[tokio::test]
async fn snapshots_are_written_as_ops_pile_up_and_on_shutdown() {
    let config = ServerConfig {
        snapshot_every: 3,
        ..fast_config()
    };
    let server = TestServer::start(config).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let pid = id.parse().expect("id");
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;

    submit(&mut alice, &id, &lease, 0, &fx.ops[..2], &codec)
        .await
        .expect("2 ops");
    let loaded = server.store.load(pid).await.expect("load").expect("exists");
    assert_eq!(loaded.snapshot.seq, 0, "below the threshold");
    submit(&mut alice, &id, &lease, 2, &fx.ops[2..3], &codec)
        .await
        .expect("3rd");
    // The actor snapshots after answering; poll briefly.
    let mut snapshot_seq = 0;
    for _ in 0..50 {
        snapshot_seq = server
            .store
            .load(pid)
            .await
            .expect("load")
            .expect("exists")
            .snapshot
            .seq;
        if snapshot_seq == 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(snapshot_seq, 3);

    submit(&mut alice, &id, &lease, 3, &fx.ops[3..], &codec)
        .await
        .expect("rest");
    let store = std::sync::Arc::clone(&server.store);
    server.stop().await;
    let loaded = store.load(pid).await.expect("load").expect("exists");
    assert_eq!(loaded.snapshot.seq, 5, "shutdown snapshots what is pending");
    assert!(loaded.ops.is_empty());
}

#[tokio::test]
async fn idle_projects_unload_and_reload_transparently() {
    let config = ServerConfig {
        idle_unload: Duration::from_millis(250),
        ..fast_config()
    };
    let server = TestServer::start(config).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");
    alice
        .release_lock(pb::ReleaseLockRequest {
            project_id: id.clone(),
            token: lease.token.clone(),
        })
        .await
        .expect("release");
    assert_eq!(server.registry.loaded_count(), 1);

    let mut waited = 0;
    while server.registry.loaded_count() > 0 && waited < 50 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        waited += 1;
    }
    assert_eq!(server.registry.loaded_count(), 0, "unloaded when idle");

    // The next request loads it again from the store.
    let (seq, project) = open(&mut alice, &id, &m).await;
    assert_eq!(seq, fx.ops.len() as u64);
    assert_eq!(project, expected_project(&m, &fx.ops));
    server.stop().await;
}

#[tokio::test]
async fn a_held_lease_keeps_a_project_loaded() {
    let config = ServerConfig {
        idle_unload: Duration::from_millis(200),
        lease: Duration::from_secs(60),
        ..fast_config()
    };
    let server = TestServer::start(config).await;
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    lock(&mut alice, &id).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(server.registry.loaded_count(), 1);
    server.stop().await;
}

#[tokio::test]
async fn risks_are_recomputed_by_the_server() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    let last = submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");

    let risks = alice
        .get_risks(pb::GetRisksRequest {
            project_id: id.clone(),
        })
        .await
        .expect("risks")
        .into_inner();
    assert_eq!(risks.as_of_seq, last);
    assert_eq!(risks.threats.len(), 1);
    let entry = &risks.threats[0];
    assert_eq!(entry.threat_id, fx.threat.to_string());

    // Same answer as evaluating the project directly with the engine.
    let project = expected_project(&m, &fx.ops);
    let direct = project.evaluate_risk(fx.threat).expect("evaluates");
    let key = m
        .risk_level_key(direct.max_risk().expect("rated"))
        .expect("key");
    assert_eq!(entry.inherent_risk, key);
    assert!(!entry.residual_risk.is_empty());
    server.stop().await;
}

#[tokio::test]
async fn concurrent_requests_for_a_cold_project_share_one_load() {
    let server = TestServer::start(fast_config()).await;
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let m = methodology();
    let opens: Vec<_> = (0..8)
        .map(|_| {
            let mut c = alice.clone();
            let id = id.clone();
            let m = m.clone();
            tokio::spawn(async move { open(&mut c, &id, &m).await.0 })
        })
        .collect();
    for open in opens {
        assert_eq!(open.await.expect("task"), 0);
    }
    assert_eq!(server.registry.loaded_count(), 1);
    server.stop().await;
}
