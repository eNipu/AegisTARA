//! End-to-end tests for the engine and protocol compatibility contract
//! (ADR 0075, 224-engine-compatibility).

use std::sync::Arc;
use std::time::{Duration, Instant};

use aegis_client::commands::Command;
use aegis_client::connection::{Connection, DevLogin};
use aegis_client::error::ServerCode;
use aegis_client::report::{Block, ReportContext, report_doc};
use aegis_client::session::{Session, SessionConfig};
use aegis_client::state::{ConnectionView, Mode, ViewModel};
use aegis_client::workspace::Workspace;
use aegis_model::{
    ClientContract, ENGINE_SEMANTICS_VERSION, ENGINE_VERSION, EngineContract, Methodology,
    PROTOCOL_VERSION, Project,
};
use aegis_server::auth_layer::{AuthLayer, DevAuthn};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::store::FolderStore;
use aegis_server::{AegisService, MemoryStore, ProjectStore, Registry, server};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;

struct Server {
    url: String,
    stop: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl Server {
    async fn stop(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(done) = self.done.take() {
            let _ = done.await;
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

async fn start_with_contract(contract: EngineContract) -> Server {
    let store = Arc::new(MemoryStore::new());
    start_with_store_and_contract(store, contract).await
}

async fn start_with_store_and_contract<S: ProjectStore + 'static>(
    store: Arc<S>,
    contract: EngineContract,
) -> Server {
    let methodologies = Arc::new(Methodologies::builtin().expect("built-in methodology"));
    let config = Arc::new(ServerConfig {
        engine: contract,
        ..ServerConfig::default()
    });
    let registry = Arc::new(Registry::new(store, methodologies, config));
    let service = AegisService::new(Arc::clone(&registry)).into_server();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let layer = AuthLayer::new(Arc::new(DevAuthn), &url);
    let (tx, rx) = oneshot::channel();
    let done = tokio::spawn(async move {
        let _ = server::builder()
            .layer(layer)
            .add_service(service)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = rx.await;
                registry.shutdown().await;
            })
            .await;
    });
    Server {
        url,
        stop: Some(tx),
        done: Some(done),
    }
}

async fn wait_for(session: &mut Session, check: impl Fn(&ViewModel) -> bool) -> Arc<ViewModel> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut rx = session.subscribe();
    loop {
        let v = rx.borrow_and_update().clone();
        if check(&v) {
            return v;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "timed out waiting for view state");
        let _ = tokio::time::timeout(left, rx.changed()).await;
    }
}

#[tokio::test]
async fn compatible_build_advertises_contract_and_records_baseline_and_report_identities() {
    let srv = start_with_contract(EngineContract::current()).await;
    let conn = Connection::connect(&srv.url, Arc::new(DevLogin::new("alice", true)))
        .await
        .expect("connect");

    let info = conn.info().await.expect("server info");
    assert_eq!(info.engine_version, ENGINE_VERSION);
    assert_eq!(info.engine_semantics_version, ENGINE_SEMANTICS_VERSION);
    assert_eq!(info.protocol_version, PROTOCOL_VERSION);
    assert_eq!(
        info.min_protocol_version,
        aegis_model::MIN_SUPPORTED_PROTOCOL_VERSION
    );

    let created = conn
        .create_project("Brake ECU", "iso21434-r155")
        .await
        .expect("create project");
    let baseline = conn
        .create_baseline(&created.project_id, "SOP Gate")
        .await
        .expect("create baseline");
    let expected_digest = Methodology::iso21434_r155()
        .expect("built-in")
        .pack_digest()
        .to_owned();
    assert_eq!(baseline.engine_version, ENGINE_VERSION);
    assert_eq!(baseline.engine_semantics_version, ENGINE_SEMANTICS_VERSION);
    assert_eq!(baseline.methodology_key, "iso21434-r155");
    assert!(!baseline.methodology_version.is_empty());
    assert_eq!(baseline.methodology_digest, expected_digest);

    // Report cover records engine semantics and pinned methodology pack digest.
    let m = Arc::new(Methodology::iso21434_r155().expect("built-in"));
    let ws = Workspace::new(Project::new(Arc::clone(&m)), 0);
    let ctx = ReportContext {
        project: "Brake ECU".into(),
        user: "alice".into(),
        now_ms: 1_700_000_000_000,
        seq: 0,
        unsaved: false,
    };
    let doc = report_doc(&ws, &ctx);
    let cover_rows = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table { rows, .. } => Some(rows),
            _ => None,
        })
        .expect("cover table");
    assert!(cover_rows.iter().any(|r| {
        r.first().map(String::as_str) == Some("Engine")
            && r.get(1).is_some_and(|v| {
                v.contains(ENGINE_VERSION)
                    && v.contains(&format!("semantics v{ENGINE_SEMANTICS_VERSION}"))
            })
    }));
    assert!(cover_rows.iter().any(|r| {
        r.first().map(String::as_str) == Some("Methodology pack")
            && r.get(1).map(String::as_str) == Some(expected_digest.as_str())
    }));
}

#[tokio::test]
async fn mismatched_calculation_semantics_rejects_server_writes_and_keeps_session_read_only() {
    let srv = start_with_contract(EngineContract {
        engine_version: ENGINE_VERSION.to_owned(),
        engine_semantics: ENGINE_SEMANTICS_VERSION + 1,
        protocol_version: PROTOCOL_VERSION,
        min_protocol_version: PROTOCOL_VERSION,
    })
    .await;

    // Direct write RPCs from a client with older semantics are refused server-side.
    let conn = Connection::connect(&srv.url, Arc::new(DevLogin::new("alice", true)))
        .await
        .expect("connect");
    let err = conn
        .create_project("Blocked", "iso21434-r155")
        .await
        .expect_err("mismatched semantics must be refused");
    assert_eq!(err.code(), Some(&ServerCode::IncompatibleClient));
    assert!(err.to_string().contains("semantics"));

    // Session detects the mismatch on connect, stays read-only, and refuses editing.
    let (mut session, _task) = Session::start(
        &srv.url,
        Arc::new(DevLogin::new("alice", true)),
        SessionConfig::new("alice"),
    );
    let view = wait_for(&mut session, |v| {
        matches!(v.connection, ConnectionView::Online { .. })
    })
    .await;
    assert!(matches!(
        view.connection,
        ConnectionView::Online {
            engine_ok: false,
            ..
        }
    ));
    assert!(
        view.notices
            .iter()
            .any(|n| n.text.contains("semantics") && n.text.contains("Editing is off")),
        "expected actionable semantics notice, got {:?}",
        view.notices
    );

    // Even if the user attempts to start editing, the session stays read-only.
    let _ = session.send(Command::BeginEdit);
    tokio::time::sleep(Duration::from_millis(100)).await;
    if let Some(project) = &session.view().project {
        assert_eq!(project.mode, Mode::ReadOnly);
    }
    let _ = session.send(Command::Shutdown);
}

#[tokio::test]
async fn unsupported_protocol_version_rejects_server_writes_and_keeps_session_read_only() {
    // Server requires protocol v2..=v2; current client speaks v1.
    let srv = start_with_contract(EngineContract {
        engine_version: ENGINE_VERSION.to_owned(),
        engine_semantics: ENGINE_SEMANTICS_VERSION,
        protocol_version: PROTOCOL_VERSION + 1,
        min_protocol_version: PROTOCOL_VERSION + 1,
    })
    .await;

    let conn = Connection::connect(&srv.url, Arc::new(DevLogin::new("alice", true)))
        .await
        .expect("connect");
    let err = conn
        .create_project("Blocked", "iso21434-r155")
        .await
        .expect_err("older protocol must be refused");
    assert_eq!(err.code(), Some(&ServerCode::IncompatibleClient));
    assert!(err.to_string().contains("protocol"));

    let (mut session, _task) = Session::start(
        &srv.url,
        Arc::new(DevLogin::new("alice", true)),
        SessionConfig::new("alice"),
    );
    let view = wait_for(&mut session, |v| {
        matches!(v.connection, ConnectionView::Online { .. })
    })
    .await;
    assert!(matches!(
        view.connection,
        ConnectionView::Online {
            engine_ok: false,
            ..
        }
    ));
    assert!(
        view.notices
            .iter()
            .any(|n| n.text.contains("protocol") && n.text.contains("update AegisTARA")),
        "expected actionable protocol notice, got {:?}",
        view.notices
    );
    let _ = session.send(Command::Shutdown);

    // Also test a newer client (protocol v2) talking to a v1 server.
    let v1_srv = start_with_contract(EngineContract::current()).await;
    let newer_conn = Connection::connect(&v1_srv.url, Arc::new(DevLogin::new("alice", true)))
        .await
        .expect("connect")
        .with_client_contract(ClientContract {
            engine_version: "0.9.0".to_owned(),
            engine_semantics: ENGINE_SEMANTICS_VERSION,
            protocol_version: PROTOCOL_VERSION + 1,
        });
    let err = newer_conn
        .create_project("Too New", "iso21434-r155")
        .await
        .expect_err("newer protocol must be refused");
    assert_eq!(err.code(), Some(&ServerCode::IncompatibleClient));
    assert!(err.to_string().contains("newer"));
}

#[tokio::test]
async fn legacy_client_without_headers_is_accepted_on_v1_and_rejected_after_upgrade() {
    let legacy_contract = ClientContract {
        engine_version: ENGINE_VERSION.to_owned(),
        engine_semantics: 0,
        protocol_version: 0,
    };

    // On a v1 server, a legacy pre-224 client omitting headers is allowed.
    let v1_srv = start_with_contract(EngineContract::current()).await;
    let legacy_v1 = Connection::connect(&v1_srv.url, Arc::new(DevLogin::new("alice", true)))
        .await
        .expect("connect")
        .with_client_contract(legacy_contract.clone());
    let created = legacy_v1
        .create_project("Legacy OK", "iso21434-r155")
        .await
        .expect("legacy v1 client allowed on v1 server");
    assert_eq!(created.name, "Legacy OK");

    // Once the server upgrades semantics to v2, the same legacy client is rejected.
    let v2_srv = start_with_contract(EngineContract {
        engine_version: "0.2.0".to_owned(),
        engine_semantics: 2,
        protocol_version: 1,
        min_protocol_version: 1,
    })
    .await;
    let legacy_v2 = Connection::connect(&v2_srv.url, Arc::new(DevLogin::new("alice", true)))
        .await
        .expect("connect")
        .with_client_contract(legacy_contract);
    let err = legacy_v2
        .create_project("Legacy Blocked", "iso21434-r155")
        .await
        .expect_err("legacy client must be rejected after semantics bump");
    assert_eq!(err.code(), Some(&ServerCode::IncompatibleClient));
}

#[tokio::test]
async fn folder_store_round_trips_baseline_identities_across_reopen() {
    let dir = std::env::temp_dir().join(format!("aegis-224-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("mkdir");

    let store = Arc::new(FolderStore::open(&dir).expect("open folder store"));
    let srv = start_with_store_and_contract(Arc::clone(&store), EngineContract::current()).await;
    let conn = Connection::connect(&srv.url, Arc::new(DevLogin::new("alice", true)))
        .await
        .expect("connect");
    let created = conn
        .create_project("Gateway", "iso21434-r155")
        .await
        .expect("create");
    let b = conn
        .create_baseline(&created.project_id, "Release 1")
        .await
        .expect("baseline");
    drop(conn);
    srv.stop().await;
    drop(store);

    // Reopen FolderStore from disk and verify engine + methodology identities persisted.
    let reopened = Arc::new(FolderStore::open(&dir).expect("reopen folder store"));
    let srv2 = start_with_store_and_contract(reopened, EngineContract::current()).await;
    let conn2 = Connection::connect(&srv2.url, Arc::new(DevLogin::new("alice", true)))
        .await
        .expect("connect 2");
    let list = conn2.baselines(&created.project_id).await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].baseline_id, b.baseline_id);
    assert_eq!(list[0].engine_version, ENGINE_VERSION);
    assert_eq!(list[0].engine_semantics_version, ENGINE_SEMANTICS_VERSION);
    assert_eq!(list[0].methodology_key, "iso21434-r155");
    assert_eq!(
        list[0].methodology_digest,
        Methodology::iso21434_r155()
            .expect("built-in")
            .pack_digest()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
