//! Shared harness: a real gRPC server on a loopback port and clients.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use aegis_model::{
    AssetId, AttackTree, DamageId, GateKind, Methodology, Node, NodeId, Op, OpCodec, Project,
    Provenance, ThreatId, TreeId,
};
use aegis_server::auth::{DEV_ADMIN_HEADER, DEV_USER_HEADER, dev_interceptor};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::pb::aegis_client::AegisClient;
use aegis_server::{AegisService, MemoryStore, Registry, pb, server};
use bytes::Bytes;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::Interceptor;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Status};

pub const METHODOLOGY: &str = "iso21434-r155";

/// Adds the development identity headers to every call.
#[derive(Clone)]
pub struct DevUser {
    user: String,
    admin: bool,
}

impl Interceptor for DevUser {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let md = request.metadata_mut();
        md.insert(DEV_USER_HEADER, self.user.parse().expect("ascii"));
        if self.admin {
            md.insert(DEV_ADMIN_HEADER, "1".parse().expect("ascii"));
        }
        Ok(request)
    }
}

pub type Client = AegisClient<InterceptedService<Channel, DevUser>>;

pub struct TestServer {
    pub addr: std::net::SocketAddr,
    pub registry: Arc<Registry<MemoryStore>>,
    pub store: Arc<MemoryStore>,
    stop: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    pub async fn start(config: ServerConfig) -> Self {
        Self::start_on(Arc::new(MemoryStore::new()), config).await
    }

    /// Starts a server over an existing store (to test restarts).
    pub async fn start_on(store: Arc<MemoryStore>, config: ServerConfig) -> Self {
        let registry = Arc::new(Registry::new(
            Arc::clone(&store),
            Arc::new(Methodologies::builtin().expect("builtin")),
            Arc::new(config),
        ));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (stop, stopped) = oneshot::channel::<()>();
        let service = AegisService::new(Arc::clone(&registry)).into_server();
        let reg = Arc::clone(&registry);
        let done = tokio::spawn(async move {
            // Unload the projects first: that ends every `Watch` stream, so
            // the graceful drain of the connections can finish.
            server::builder()
                .layer(tonic::service::InterceptorLayer::new(dev_interceptor))
                .add_service(service)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = stopped.await;
                    reg.shutdown().await;
                })
                .await
                .expect("serve");
        });
        TestServer {
            addr,
            registry,
            store,
            stop: Some(stop),
            done: Some(done),
        }
    }

    pub async fn client(&self, user: &str) -> Client {
        self.client_as(user, false).await
    }

    pub async fn admin(&self, user: &str) -> Client {
        self.client_as(user, true).await
    }

    async fn client_as(&self, user: &str, admin: bool) -> Client {
        let channel = Endpoint::from_shared(format!("http://{}", self.addr))
            .expect("uri")
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await
            .expect("connect");
        AegisClient::with_interceptor(
            channel,
            DevUser {
                user: user.to_owned(),
                admin,
            },
        )
    }

    /// Stops the server and waits until every project is snapshotted.
    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(done) = self.done.take() {
            done.await.expect("server task");
        }
    }
}

pub fn fast_config() -> ServerConfig {
    ServerConfig {
        lease: Duration::from_secs(60),
        wall_clock: || 1_700_000_000_000,
        ..ServerConfig::default()
    }
}

pub async fn create_project(client: &mut Client, name: &str) -> String {
    client
        .create_project(pb::CreateProjectRequest {
            name: name.into(),
            methodology_key: METHODOLOGY.into(),
        })
        .await
        .expect("create")
        .into_inner()
        .project
        .expect("summary")
        .project_id
}

pub async fn lock(client: &mut Client, project: &str) -> pb::Lease {
    client
        .acquire_lock(pb::LockRequest {
            project_id: project.into(),
        })
        .await
        .expect("lock")
        .into_inner()
}

pub async fn submit(
    client: &mut Client,
    project: &str,
    lease: &pb::Lease,
    base_seq: u64,
    ops: &[Op],
    codec: &OpCodec,
) -> Result<u64, Status> {
    let ops: Vec<Bytes> = ops
        .iter()
        .map(|op| Bytes::from(codec.encode(op).expect("encodes")))
        .collect();
    client
        .submit_ops(pb::SubmitOpsRequest {
            project_id: project.into(),
            lock_token: lease.token.clone(),
            base_seq,
            ops,
        })
        .await
        .map(|r| r.into_inner().last_seq)
}

/// Opens a project and returns `(last_seq, loaded project)`.
pub async fn open(client: &mut Client, project: &str, m: &Arc<Methodology>) -> (u64, Project) {
    let mut stream = client
        .open_project(pb::OpenProjectRequest {
            project_id: project.into(),
        })
        .await
        .expect("open")
        .into_inner();
    let mut bytes = Vec::new();
    let mut header = None;
    while let Some(chunk) = stream.message().await.expect("chunk") {
        if let Some(h) = chunk.header {
            assert!(header.is_none(), "header only on the first chunk");
            header = Some(h);
        }
        bytes.extend_from_slice(&chunk.data);
    }
    let header = header.expect("header");
    assert_eq!(header.snapshot_bytes as usize, bytes.len());
    (
        header.last_seq,
        Project::load(bytes.as_slice(), Arc::clone(m)).expect("snapshot loads"),
    )
}

pub fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

/// Ops that build one asset, damage scenario, threat, and a rated tree.
pub struct Fixture {
    pub asset: AssetId,
    pub damage: DamageId,
    pub threat: ThreatId,
    pub tree: TreeId,
    pub ops: Vec<Op>,
}

pub fn fixture(m: &Methodology) -> Fixture {
    let integrity = m.property("integrity").expect("property");
    let tampering = m.threat_category("tampering").expect("category");
    let safety = m.impact_category("safety").expect("category");
    let severe = m.level(safety, "severe").expect("level");
    let low = m.feasibility_level("low").expect("feasibility");
    let (asset, damage, threat, tree) = (
        AssetId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
        TreeId::new_v7(),
    );
    let root = NodeId::new_v7();
    let step = NodeId::new_v7();
    let mut t = AttackTree::new(root, Node::gate(GateKind::Or, "Compromise ECU")).expect("tree");
    t.add_child(t.root(), step, Node::step("Flash modified firmware"))
        .expect("child");
    let ops = vec![
        Op::AddAsset {
            id: asset,
            name: "Brake ECU".into(),
            properties: [integrity].into_iter().collect(),
            provenance: Provenance::default(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Vehicle cannot brake".into(),
            impacts: vec![(safety, severe)],
            provenance: Provenance::default(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Modify brake firmware".into(),
            asset,
            category: tampering,
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: Provenance::default(),
        },
        Op::AddAttackTree {
            id: tree,
            tree: t,
            step_feasibility: vec![(step, low)],
            provenance: Provenance::default(),
        },
        Op::SetThreatAttackTree {
            threat,
            tree: Some(tree),
            provenance: Provenance::default(),
        },
    ];
    Fixture {
        asset,
        damage,
        threat,
        tree,
        ops,
    }
}

/// Applies `ops` to a fresh project: what a correct server must end up with.
pub fn expected_project(m: &Arc<Methodology>, ops: &[Op]) -> Project {
    let mut p = Project::new(Arc::clone(m));
    for op in ops {
        p.apply(op.clone()).expect("applies");
    }
    p
}
