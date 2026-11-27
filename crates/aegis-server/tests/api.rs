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
    let keys: Vec<&str> = info.methodologies.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, vec!["eu-cra", METHODOLOGY, "stride-services-cloud"]);

    for key in ["eu-cra", METHODOLOGY, "stride-services-cloud"] {
        let pack = alice
            .get_methodology(pb::GetMethodologyRequest { key: key.into() })
            .await
            .expect("pack")
            .into_inner();
        let m = aegis_model::Methodology::read_pack(pack.pack.as_ref()).expect("valid pack");
        assert_eq!(m.key(), key);
    }

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
async fn the_project_status_gives_the_sequence_and_the_lock_holder() {
    let server = TestServer::start(fast_config()).await;
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let mut alice = server.client("alice").await;
    let mut bob = server.client("bob").await;
    let id = create_project(&mut alice, "p").await;
    let status = |id: &str| pb::GetProjectStatusRequest {
        project_id: id.to_owned(),
    };

    let idle = bob
        .get_project_status(status(&id))
        .await
        .expect("status")
        .into_inner();
    assert_eq!((idle.last_seq, idle.lock_holder.as_str()), (0, ""));

    let fx = fixture(&m);
    let lease = lock(&mut alice, &id).await;
    submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");
    // Anyone may look; it takes no lease and changes nothing.
    let held = bob
        .get_project_status(status(&id))
        .await
        .expect("status")
        .into_inner();
    assert_eq!(
        (held.last_seq, held.lock_holder.as_str()),
        (fx.ops.len() as u64, "alice")
    );

    alice
        .release_lock(pb::ReleaseLockRequest {
            project_id: id.clone(),
            token: lease.token.clone(),
        })
        .await
        .expect("released");
    let freed = bob
        .get_project_status(status(&id))
        .await
        .expect("status")
        .into_inner();
    assert_eq!(
        (freed.last_seq, freed.lock_holder.as_str()),
        (fx.ops.len() as u64, "")
    );

    let err = bob
        .get_project_status(status(&uuid::Uuid::now_v7().to_string()))
        .await
        .expect_err("unknown project");
    assert_eq!(err.code(), tonic::Code::NotFound);
    let err = bob
        .get_project_status(status("not-an-id"))
        .await
        .expect_err("bad id");
    assert_eq!(error_code(&err), "bad_project_id");
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
async fn a_tree_of_many_nodes_is_built_by_atomic_batches_and_survives_a_restart() {
    use aegis_model::{AttackTree, GateKind, Node, NodeId, TreeId};
    let store = std::sync::Arc::new(aegis_server::MemoryStore::new());
    let m = methodology();
    let codec = OpCodec::new(m.clone());
    let first = TestServer::start_on(std::sync::Arc::clone(&store), fast_config()).await;
    let mut alice = first.client("alice").await;
    let id = create_project(&mut alice, "p").await;
    let lease = lock(&mut alice, &id).await;

    let (tree, root) = (TreeId::new_v7(), NodeId::new_v7());
    let mut seq = submit(
        &mut alice,
        &id,
        &lease,
        0,
        &[Op::AddAttackTree {
            role: Default::default(),
            id: tree,
            tree: AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree"),
            step_feasibility: vec![],
            provenance: Provenance::default(),
        }],
        &codec,
    )
    .await
    .expect("accepted");

    let add = |i: u32, id: NodeId| Op::AddTreeNode {
        tree,
        parent: root,
        id,
        node: Node::step(format!("step {i}")),
        index: i,
        position: i - 1,
        provenance: Provenance::default(),
    };
    // A batch whose last op is refused changes nothing.
    let mut bad: Vec<Op> = (1..=40).map(|i| add(i, NodeId::new_v7())).collect();
    bad.push(add(99, NodeId::new_v7()));
    let err = submit(&mut alice, &id, &lease, seq, &bad, &codec)
        .await
        .expect_err("one bad op refuses the batch");
    assert_eq!(error_code(&err), "rejected_op");
    let (after_refusal, _) = open(&mut alice, &id, &m).await;
    assert_eq!(after_refusal, seq, "nothing of the batch was kept");

    // Batches of 500 nodes build a tree of 1,501 nodes: far more than one op
    // can carry.
    let mut next = 1u32;
    for _ in 0..3 {
        let batch: Vec<Op> = (0..500)
            .map(|_| {
                let op = add(next, NodeId::new_v7());
                next += 1;
                op
            })
            .collect();
        seq = submit(&mut alice, &id, &lease, seq, &batch, &codec)
            .await
            .expect("accepted");
    }
    let (_, before) = open(&mut alice, &id, &m).await;
    first.stop().await;

    // After a restart the snapshot (and the ops after it) rebuild the tree.
    let second = TestServer::start_on(store, fast_config()).await;
    let mut alice = second.client("alice").await;
    let (seq_after, after) = open(&mut alice, &id, &m).await;
    assert_eq!(seq_after, seq);
    assert_eq!(after, before);
    let tree_idx = after.attack_tree_idx(&tree).expect("tree");
    assert_eq!(after.attack_tree(tree_idx).expect("tree").len(), 1_501);
    second.stop().await;
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

#[tokio::test]
async fn figures_lifecycle_chunked_streaming_sanitization_and_quotas() {
    let config = ServerConfig {
        chunk_bytes: 16,
        max_figures_per_project: 2,
        ..fast_config()
    };
    let server = TestServer::start(config).await;
    let mut alice = server.client("alice").await;
    let id = create_project(&mut alice, "p").await;

    let svg = br##"<svg viewBox="0 0 320 200"><rect x="0" y="0" width="320" height="200" fill="#0f172a"/><text x="16" y="32" fill="#f8fafc">Gateway</text></svg>"##;
    let chunks: Vec<bytes::Bytes> = svg.chunks(24).map(bytes::Bytes::copy_from_slice).collect();

    // Without lock -> refused.
    let err = alice
        .put_figure(pb::PutFigureRequest {
            project_id: id.clone(),
            lock_token: "bad".into(),
            caption: "Architecture".into(),
            target: String::new(),
            media_type: "image/svg+xml".into(),
            chunks: chunks.clone(),
        })
        .await
        .expect_err("no lease");
    assert_eq!(err.code(), Code::FailedPrecondition);

    let lease = lock(&mut alice, &id).await;

    // Unsafe SVG -> refused.
    let bad = alice
        .put_figure(pb::PutFigureRequest {
            project_id: id.clone(),
            lock_token: lease.token.clone(),
            caption: "Bad".into(),
            target: String::new(),
            media_type: "image/svg+xml".into(),
            chunks: vec![bytes::Bytes::from_static(
                b"<svg><script>alert(1)</script></svg>",
            )],
        })
        .await
        .expect_err("unsafe svg");
    assert_eq!(bad.code(), Code::InvalidArgument);

    // Valid SVG -> stored and sanitized.
    let info = alice
        .put_figure(pb::PutFigureRequest {
            project_id: id.clone(),
            lock_token: lease.token.clone(),
            caption: "  System context  ".into(),
            target: String::new(),
            media_type: "image/svg+xml".into(),
            chunks: chunks.clone(),
        })
        .await
        .expect("put figure")
        .into_inner();
    assert_eq!(info.caption, "System context");
    assert_eq!((info.width, info.height), (320, 200));

    // Stream back in 16-byte chunks.
    let mut stream = alice
        .get_figure(pb::GetFigureRequest {
            project_id: id.clone(),
            figure_id: info.figure_id.clone(),
        })
        .await
        .expect("get figure")
        .into_inner();
    let mut first_info = None;
    let mut data = Vec::new();
    let mut chunk_count = 0usize;
    while let Some(chunk) = stream.message().await.expect("chunk") {
        if chunk_count == 0 {
            first_info = chunk.info;
        } else {
            assert!(chunk.info.is_none());
        }
        data.extend_from_slice(&chunk.data);
        chunk_count += 1;
    }
    assert!(chunk_count > 1);
    assert_eq!(first_info.expect("info").figure_id, info.figure_id);
    assert_eq!(data.len() as u64, info.byte_len);

    // Update caption and target.
    let comp_id = aegis_model::ComponentId::new_v7();
    let updated = alice
        .update_figure(pb::UpdateFigureRequest {
            project_id: id.clone(),
            lock_token: lease.token.clone(),
            figure_id: info.figure_id.clone(),
            caption: "Gateway diagram".into(),
            target: format!("component:{comp_id}"),
        })
        .await
        .expect("update figure")
        .into_inner();
    assert_eq!(updated.caption, "Gateway diagram");
    assert_eq!(updated.target, format!("component:{comp_id}"));

    // Second figure succeeds; third hits max_figures_per_project = 2.
    alice
        .put_figure(pb::PutFigureRequest {
            project_id: id.clone(),
            lock_token: lease.token.clone(),
            caption: "Second".into(),
            target: String::new(),
            media_type: "image/svg+xml".into(),
            chunks: chunks.clone(),
        })
        .await
        .expect("second figure");
    let quota_err = alice
        .put_figure(pb::PutFigureRequest {
            project_id: id.clone(),
            lock_token: lease.token.clone(),
            caption: "Third".into(),
            target: String::new(),
            media_type: "image/svg+xml".into(),
            chunks,
        })
        .await
        .expect_err("quota exceeded");
    assert_eq!(quota_err.code(), Code::ResourceExhausted);

    // Delete first figure.
    alice
        .delete_figure(pb::DeleteFigureRequest {
            project_id: id.clone(),
            lock_token: lease.token.clone(),
            figure_id: info.figure_id.clone(),
        })
        .await
        .expect("delete figure");
    let listed = alice
        .list_figures(pb::ListFiguresRequest {
            project_id: id.clone(),
        })
        .await
        .expect("list")
        .into_inner();
    assert_eq!(listed.figures.len(), 1);
    server.stop().await;
}
