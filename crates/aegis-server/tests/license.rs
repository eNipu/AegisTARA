//! The license gate against a real server: who may edit, and what still
//! works when the license is over.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use aegis_license::{DAY, Issuer, License, TrustedKeys};
use aegis_server::license::{ClockSecs, LoadError};
use aegis_server::pb::license_info::State;
use aegis_server::{LicenseGate, pb};
use common::{TestServer, create_project, fast_config};
use tonic::Code;

/// A clock the test moves.
#[derive(Clone)]
struct Clock(Arc<AtomicU64>);

impl Clock {
    fn at(secs: u64) -> Self {
        Clock(Arc::new(AtomicU64::new(secs)))
    }

    fn set(&self, secs: u64) {
        self.0.store(secs, Ordering::SeqCst);
    }

    fn secs(&self) -> ClockSecs {
        let inner = Arc::clone(&self.0);
        Arc::new(move || inner.load(Ordering::SeqCst))
    }
}

const START: u64 = 1_800_000_000;

fn record(seats: u32, grace_days: u32) -> License {
    License {
        version: 1,
        id: "test-1".into(),
        kid: "test".into(),
        customer: "Example Motors".into(),
        seats,
        issued_at: START,
        expires_at: START + 100 * DAY,
        grace_days,
        features: vec!["audit".into()],
    }
}

fn gate(seats: u32, grace_days: u32, clock: &Clock) -> Arc<LicenseGate> {
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let file = issuer.sign(&record(seats, grace_days)).expect("signs");
    let keys = TrustedKeys::single("test", public).expect("keys");
    Arc::new(LicenseGate::load(&file, &keys, clock.secs()).expect("loads"))
}

async fn error_code(status: &tonic::Status) -> String {
    status
        .metadata()
        .get(aegis_server::error::ERROR_CODE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned()
}

#[tokio::test]
async fn the_server_does_not_load_a_bad_license() {
    let clock = Clock::at(START + DAY);
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let keys = TrustedKeys::single("test", public).expect("keys");
    let file = issuer.sign(&record(5, 10)).expect("signs");

    assert!(LicenseGate::load(&file, &keys, clock.secs()).is_ok());
    // Garbage, and a key the build does not trust.
    assert!(matches!(
        LicenseGate::load("nonsense", &keys, clock.secs()),
        Err(LoadError::Invalid(_))
    ));
    assert!(matches!(
        LicenseGate::load(&file, &TrustedKeys::none(), clock.secs()),
        Err(LoadError::Invalid(_))
    ));
    // Not yet valid, in grace (loads), and over (does not load).
    clock.set(START - 5 * DAY);
    assert_eq!(
        LicenseGate::load(&file, &keys, clock.secs()).err(),
        Some(LoadError::NotYetValid)
    );
    clock.set(START + 105 * DAY);
    assert!(
        LicenseGate::load(&file, &keys, clock.secs()).is_ok(),
        "grace period"
    );
    clock.set(START + 111 * DAY);
    assert_eq!(
        LicenseGate::load(&file, &keys, clock.secs()).err(),
        Some(LoadError::Expired)
    );
}

#[tokio::test]
async fn server_info_reports_the_license() {
    let clock = Clock::at(START + DAY);
    let server = TestServer::start_licensed(fast_config(), gate(3, 14, &clock)).await;
    let mut alice = server.client("alice").await;

    let info = alice
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .expect("info")
        .into_inner()
        .license
        .expect("license");
    assert_eq!(info.state(), State::Active);
    assert_eq!(info.customer, "Example Motors");
    assert_eq!((info.seats, info.seats_used), (3, 0));
    assert_eq!(info.expires_at, START + 100 * DAY);
    assert_eq!(info.grace_ends_at, START + 114 * DAY);
    assert_eq!(info.features, vec!["audit".to_owned()]);

    let project = create_project(&mut alice, "Seats").await;
    alice
        .acquire_lock(pb::LockRequest {
            project_id: project,
        })
        .await
        .expect("lock");
    let info = alice
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .expect("info")
        .into_inner()
        .license
        .expect("license");
    assert_eq!(info.seats_used, 1);
    server.stop().await;
}

#[tokio::test]
async fn a_new_editor_is_refused_when_every_seat_is_taken() {
    let clock = Clock::at(START + DAY);
    let server = TestServer::start_licensed(fast_config(), gate(2, 0, &clock)).await;
    let mut alice = server.client("alice").await;
    let mut bob = server.client("bob").await;
    let mut carol = server.client("carol").await;
    let p1 = create_project(&mut alice, "One").await;
    let p2 = create_project(&mut alice, "Two").await;
    let p3 = create_project(&mut alice, "Three").await;

    alice
        .acquire_lock(pb::LockRequest {
            project_id: p1.clone(),
        })
        .await
        .expect("alice");
    bob.acquire_lock(pb::LockRequest { project_id: p2 })
        .await
        .expect("bob");
    let refused = carol
        .acquire_lock(pb::LockRequest {
            project_id: p3.clone(),
        })
        .await
        .expect_err("third user");
    assert_eq!(refused.code(), Code::ResourceExhausted);
    assert_eq!(error_code(&refused).await, "license_seats");

    // Carol can still read, and a known editor can take another project.
    let mut opened = carol
        .open_project(pb::OpenProjectRequest { project_id: p3 })
        .await
        .expect("read")
        .into_inner();
    while opened.message().await.expect("chunk").is_some() {}
    let p4 = create_project(&mut alice, "Four").await;
    alice
        .acquire_lock(pb::LockRequest { project_id: p4 })
        .await
        .expect("alice again");
    server.stop().await;
}

#[tokio::test]
async fn an_expired_license_keeps_reading_but_refuses_editing() {
    let clock = Clock::at(START + DAY);
    let server = TestServer::start_licensed(fast_config(), gate(5, 10, &clock)).await;
    let mut alice = server.client("alice").await;
    let project = create_project(&mut alice, "Lapsing").await;
    let lock = alice
        .acquire_lock(pb::LockRequest {
            project_id: project.clone(),
        })
        .await
        .expect("active");
    alice
        .release_lock(pb::ReleaseLockRequest {
            project_id: project.clone(),
            token: lock.into_inner().token,
        })
        .await
        .expect("release");

    // Grace: everything works, and the info says so.
    clock.set(START + 105 * DAY);
    alice
        .acquire_lock(pb::LockRequest {
            project_id: project.clone(),
        })
        .await
        .expect("grace period still edits");
    let info = alice
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .expect("info")
        .into_inner()
        .license
        .expect("license");
    assert_eq!(info.state(), State::Grace);

    // Over: editing stops, reading does not.
    clock.set(START + 111 * DAY);
    let refused = alice
        .acquire_lock(pb::LockRequest {
            project_id: project.clone(),
        })
        .await
        .expect_err("expired");
    assert_eq!(refused.code(), Code::FailedPrecondition);
    assert_eq!(error_code(&refused).await, "license_expired");
    let mut opened = alice
        .open_project(pb::OpenProjectRequest {
            project_id: project,
        })
        .await
        .expect("reading still works")
        .into_inner();
    while opened.message().await.expect("chunk").is_some() {}
    assert!(
        alice
            .list_projects(pb::ListProjectsRequest {})
            .await
            .is_ok()
    );
    let info = alice
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .expect("info")
        .into_inner()
        .license
        .expect("license");
    assert_eq!(info.state(), State::Expired);
    server.stop().await;
}
