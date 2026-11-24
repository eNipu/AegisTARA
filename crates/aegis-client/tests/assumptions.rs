//! Integration tests for Assumptions page, headless Create claim draft,
//! exclusion formatting, and ConditionSnapshot toasts (job 136, slice 136e).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aegis_client::assumptions::assumptions_view;
use aegis_client::commands::{Command, Edit};
use aegis_client::configuration::ExcludedCounts;
use aegis_client::connection::DevLogin;
use aegis_client::exclusion::{ConditionSnapshot, excluded_entries, reasons_text};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages;
use aegis_client::session::{Session, SessionConfig};
use aegis_client::state::{ConnectionView, Mode, ProjectView, ViewModel};
use aegis_client::view::{DraftKind, Page, Selection, UiState, page_descriptions};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AssumptionId, AttackTree, ClaimId, ConditionId, DamageId, ElementRef, GateKind,
    Methodology, Node, NodeId, Op, Provenance, Reason, ThreatId, TreeId,
};
use aegis_server::auth_layer::{AuthLayer, DevAuthn};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::{AegisService, MemoryStore, Registry, server};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;

const NOW: u64 = 1_792_956_600_000;

fn prov() -> Provenance {
    Provenance::human("tester", NOW)
}

fn apply_edit(ws: &mut Workspace, edit: Edit) {
    let ops = edit.to_ops(ws.project(), "tester", NOW).expect("to_ops");
    ws.apply(ops, Origin::Edit).expect("apply edit");
}

struct Client(Session);

impl std::ops::Deref for Client {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.0
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.0.send(Command::Shutdown);
    }
}

struct TestServer {
    url: String,
    stop: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    async fn start() -> Self {
        let registry = Arc::new(Registry::new(
            Arc::new(MemoryStore::new()),
            Arc::new(Methodologies::builtin().expect("builtin")),
            Arc::new(ServerConfig {
                lease: Duration::from_secs(30),
                ..ServerConfig::default()
            }),
        ));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let (stop, stopped) = oneshot::channel::<()>();
        let service = AegisService::new(Arc::clone(&registry)).into_server();
        let layer = AuthLayer::new(Arc::new(DevAuthn), &url);
        let done = tokio::spawn(async move {
            server::builder()
                .layer(layer)
                .add_service(service)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = stopped.await;
                    registry.shutdown().await;
                })
                .await
                .expect("serve");
        });
        Self {
            url,
            stop: Some(stop),
            done: Some(done),
        }
    }

    fn session(&self, user: &str) -> Client {
        let mut config = SessionConfig::new(user);
        config.retry_delay = Duration::from_millis(50);
        Client(Session::start(&self.url, Arc::new(DevLogin::new(user, false)), config).0)
    }

    async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(done) = self.done.take() {
            tokio::time::timeout(Duration::from_secs(5), done)
                .await
                .expect("server stop")
                .expect("server task");
        }
    }
}

async fn until(
    session: &Session,
    what: &str,
    ready: impl Fn(&ViewModel) -> bool,
) -> Arc<ViewModel> {
    let mut rx = session.subscribe();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        {
            let view = rx.borrow_and_update().clone();
            if ready(&view) {
                return view;
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "timed out waiting for: {what}");
        let _ = tokio::time::timeout(left, rx.changed()).await;
    }
}

fn pv(vm: &ViewModel) -> &ProjectView {
    vm.project.as_ref().expect("project")
}

async fn open_and_edit(server: &TestServer) -> Client {
    let alice = server.session("alice");
    until(&alice, "online", |vm| {
        matches!(vm.connection, ConnectionView::Online { .. })
    })
    .await;
    alice
        .send(Command::CreateProject {
            name: "Brake".into(),
            methodology_key: "iso21434-r155".into(),
        })
        .expect("sent");
    until(&alice, "project", |vm| vm.project.is_some()).await;
    alice.send(Command::BeginEdit).expect("sent");
    until(&alice, "editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    alice
}

#[tokio::test]
async fn headless_create_claim_from_assumption_and_draft_contexts() {
    let server = TestServer::start().await;
    let alice = open_and_edit(&server).await;

    let initial = alice.view();
    assert_eq!(pv(&initial).nav_counts[Page::Assumptions.index()], None);

    let as00 = AssumptionId::new_v7();
    let as01 = AssumptionId::new_v7();
    alice
        .send(Command::Edit(Edit::AddAssumption {
            id: as00,
            name: "Debug port fused".into(),
            holds: false,
        }))
        .expect("sent");
    alice
        .send(Command::Edit(Edit::AddAssumption {
            id: as01,
            name: "Bootloader locked".into(),
            holds: false,
        }))
        .expect("sent");
    let v = until(&alice, "two assumptions", |vm| {
        pv(vm).nav_counts[Page::Assumptions.index()] == Some(2)
    })
    .await;
    assert_eq!(pv(&v).page, Page::Assumptions);

    // Unknown AssumptionId is ignored with zero state change.
    alice.send(Command::ShowPage(Page::Overview)).expect("sent");
    until(&alice, "overview", |vm| pv(vm).page == Page::Overview).await;
    alice
        .send(Command::CreateClaimFromAssumption(AssumptionId::new_v7()))
        .expect("sent");
    alice.send(Command::ShowPage(Page::Goals)).expect("sent");
    let v = until(&alice, "goals after unknown", |vm| {
        pv(vm).page == Page::Goals
    })
    .await;
    assert_eq!(pv(&v).draft, None);
    assert_eq!(pv(&v).draft_assumption, None);
    assert!(pv(&v).data.claims.is_empty());
    assert!(v.notices.is_empty());

    // Valid CreateClaimFromAssumption opens DraftKind::Claim with draft_assumption set.
    alice
        .send(Command::CreateClaimFromAssumption(as00))
        .expect("sent");
    let v = until(&alice, "claim draft for as00", |vm| {
        let p = pv(vm);
        p.page == Page::Goals
            && p.draft == Some(DraftKind::Claim)
            && p.draft_assumption == Some(as00)
    })
    .await;
    assert!(!pv(&v).draft_incomplete);

    // CancelDraft clears draft_assumption and creates zero claims.
    alice.send(Command::CancelDraft).expect("sent");
    let v = until(&alice, "canceled", |vm| pv(vm).draft.is_none()).await;
    assert_eq!(pv(&v).draft_assumption, None);
    assert!(pv(&v).data.claims.is_empty());

    // Blank CommitDraft clears draft_assumption and creates zero claims.
    alice
        .send(Command::CreateClaimFromAssumption(as00))
        .expect("sent");
    until(&alice, "re-opened draft", |vm| {
        pv(vm).draft == Some(DraftKind::Claim) && pv(vm).draft_assumption == Some(as00)
    })
    .await;
    alice
        .send(Command::CommitDraft {
            name: "   ".into(),
            next: false,
        })
        .expect("sent");
    let v = until(&alice, "blank discarded", |vm| pv(vm).draft.is_none()).await;
    assert_eq!(pv(&v).draft_assumption, None);
    assert!(pv(&v).data.claims.is_empty());

    // CommitDraft creates CL00 linked to AS00 in one undo step.
    alice
        .send(Command::CreateClaimFromAssumption(as00))
        .expect("sent");
    until(&alice, "draft ready", |vm| {
        pv(vm).draft == Some(DraftKind::Claim) && pv(vm).draft_assumption == Some(as00)
    })
    .await;
    alice
        .send(Command::CommitDraft {
            name: "Debug header unpopulated".into(),
            next: false,
        })
        .expect("sent");
    let v = until(&alice, "claim created", |vm| {
        let p = pv(vm);
        p.draft.is_none() && p.data.claims.len() == 1
    })
    .await;
    assert_eq!(pv(&v).draft_assumption, None);
    assert_eq!(pv(&v).data.claims[0].visual_id, "CL00");
    assert_eq!(pv(&v).data.claims[0].name, "Debug header unpopulated");

    alice
        .send(Command::Select {
            selection: Selection::Assumption(as00),
            navigate: true,
        })
        .expect("sent");
    let v = until(&alice, "as00 linked", |vm| {
        pv(vm)
            .data
            .assumptions
            .as_ref()
            .and_then(|a| a.selected.as_ref())
            .is_some_and(|s| s.id == as00 && s.claims.len() == 1)
    })
    .await;
    let detail = pv(&v)
        .data
        .assumptions
        .as_ref()
        .and_then(|a| a.selected.as_ref())
        .expect("selected");
    assert_eq!(detail.claims[0].visual_id, "CL00");
    assert_eq!(detail.claims[0].name, "Debug header unpopulated");

    // One undo removes both claim and link cleanly.
    alice.send(Command::Undo).expect("sent");
    until(&alice, "undone link", |vm| {
        pv(vm)
            .data
            .assumptions
            .as_ref()
            .and_then(|a| a.selected.as_ref())
            .is_some_and(|s| s.id == as00 && s.claims.is_empty())
    })
    .await;
    alice.send(Command::ShowPage(Page::Goals)).expect("sent");
    let v = until(&alice, "goals empty after undo", |vm| {
        pv(vm).page == Page::Goals
    })
    .await;
    assert!(pv(&v).data.claims.is_empty());

    // BeginDraft(Claim) from Page::Assumptions with selected_assumption creates linked claim.
    alice
        .send(Command::Select {
            selection: Selection::Assumption(as01),
            navigate: true,
        })
        .expect("sent");
    until(&alice, "as01 selected", |vm| {
        pv(vm).page == Page::Assumptions
            && pv(vm)
                .data
                .assumptions
                .as_ref()
                .and_then(|a| a.selected.as_ref())
                .is_some_and(|s| s.id == as01)
    })
    .await;
    alice
        .send(Command::BeginDraft(DraftKind::Claim))
        .expect("sent");
    until(&alice, "draft from assumptions page", |vm| {
        pv(vm).draft == Some(DraftKind::Claim) && pv(vm).draft_assumption == Some(as01)
    })
    .await;
    alice
        .send(Command::CommitDraft {
            name: "Bootloader verified".into(),
            next: false,
        })
        .expect("sent");
    until(&alice, "linked claim from page", |vm| {
        pv(vm).draft.is_none() && pv(vm).data.claims.len() == 1
    })
    .await;

    // BeginDraft(Claim) from Page::Goals creates an unlinked claim (draft_assumption == None).
    alice
        .send(Command::BeginDraft(DraftKind::Claim))
        .expect("sent");
    let v = until(&alice, "draft from goals page", |vm| {
        pv(vm).draft == Some(DraftKind::Claim)
    })
    .await;
    assert_eq!(pv(&v).draft_assumption, None);
    alice
        .send(Command::CommitDraft {
            name: "Standalone claim".into(),
            next: false,
        })
        .expect("sent");
    until(&alice, "standalone claim committed", |vm| {
        pv(vm).draft.is_none() && pv(vm).data.claims.len() == 2
    })
    .await;

    alice
        .send(Command::ShowPage(Page::Assumptions))
        .expect("sent");
    let v = until(&alice, "assumptions counts", |vm| {
        pv(vm).page == Page::Assumptions && pv(vm).data.assumptions.is_some()
    })
    .await;
    let av = pv(&v).data.assumptions.as_ref().expect("view");
    assert_eq!(av.rows[0].claims, 0);
    assert_eq!(av.rows[1].claims, 1);
    assert_eq!(
        av.selected.as_ref().expect("as01").claims[0].name,
        "Bootloader verified"
    );

    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn session_assumptions_page_auto_select_find_and_nav_counts() {
    let server = TestServer::start().await;
    let alice = open_and_edit(&server).await;

    let as00 = AssumptionId::new_v7();
    let as01 = AssumptionId::new_v7();
    alice
        .send(Command::Edit(Edit::AddAssumption {
            id: as00,
            name: "Debug port fused".into(),
            holds: false,
        }))
        .expect("sent");
    alice
        .send(Command::Edit(Edit::AddAssumption {
            id: as01,
            name: "Encrypted flash".into(),
            holds: false,
        }))
        .expect("sent");
    alice
        .send(Command::Edit(Edit::SetDescription {
            target: ElementRef::Assumption(as01),
            text: "AES-256 XTS".into(),
        }))
        .expect("sent");
    until(&alice, "two assumptions", |vm| {
        pv(vm).nav_counts[Page::Assumptions.index()] == Some(2)
    })
    .await;

    // Clear selection by switching away & removing then testing ShowPage auto-select on a fresh open,
    // or verify ShowPage auto-selects when selected_assumption is stale/none.
    alice
        .send(Command::Edit(Edit::RemoveAssumption { assumption: as00 }))
        .expect("sent");
    alice.send(Command::ShowPage(Page::Overview)).expect("sent");
    until(&alice, "overview", |vm| pv(vm).page == Page::Overview).await;
    alice
        .send(Command::Edit(Edit::AddAssumption {
            id: as00,
            name: "Debug port fused".into(),
            holds: false,
        }))
        .expect("sent");
    alice.send(Command::ShowPage(Page::Overview)).expect("sent");
    until(&alice, "back on overview", |vm| {
        pv(vm).page == Page::Overview
    })
    .await;

    // Remove currently selected assumption while on Overview so selected_assumption becomes stale,
    // then open Page::Assumptions and verify auto-select picks the first live assumption.
    alice
        .send(Command::Edit(Edit::RemoveAssumption { assumption: as00 }))
        .expect("sent");
    alice.send(Command::ShowPage(Page::Overview)).expect("sent");
    until(&alice, "overview after remove", |vm| {
        pv(vm).page == Page::Overview
    })
    .await;
    alice
        .send(Command::ShowPage(Page::Assumptions))
        .expect("sent");
    let v = until(&alice, "auto-selected live assumption", |vm| {
        pv(vm).page == Page::Assumptions
            && pv(vm)
                .data
                .assumptions
                .as_ref()
                .and_then(|a| a.selected.as_ref())
                .is_some_and(|s| s.id == as01)
    })
    .await;
    assert_eq!(pv(&v).descriptions[0].kind, "assumption");
    assert_eq!(pv(&v).descriptions[0].text, "AES-256 XTS");

    // Re-add second assumption for find_rows filtering by visual ID and name.
    let as02 = AssumptionId::new_v7();
    alice
        .send(Command::Edit(Edit::AddAssumption {
            id: as02,
            name: "CAN bus isolated".into(),
            holds: false,
        }))
        .expect("sent");
    until(&alice, "two live assumptions", |vm| {
        pv(vm).nav_counts[Page::Assumptions.index()] == Some(2)
    })
    .await;

    let vis1 = pv(&alice.view())
        .data
        .assumptions
        .as_ref()
        .expect("assumptions")
        .rows[0]
        .visual_id
        .clone();

    alice
        .send(Command::SetFind {
            text: vis1.clone(),
            regex: false,
        })
        .expect("sent");
    let v = until(&alice, "find by visual id", |vm| {
        pv(vm)
            .find
            .as_ref()
            .is_some_and(|f| f.shown == 1 && f.total == 2)
    })
    .await;
    assert_eq!(
        pv(&v).data.assumptions.as_ref().expect("view").rows[0].id,
        as01
    );

    alice
        .send(Command::SetFind {
            text: "CAN bus".into(),
            regex: false,
        })
        .expect("sent");
    let v = until(&alice, "find by name", |vm| {
        pv(vm).data.assumptions.as_ref().is_some_and(|a| {
            a.rows.len() == 1
                && a.rows[0].id == as02
                && pv(vm).find.as_ref().is_some_and(|f| f.shown == 1)
        })
    })
    .await;
    assert_eq!(pv(&v).find.as_ref().expect("find").total, 2);

    alice
        .send(Command::SetFind {
            text: "".into(),
            regex: false,
        })
        .expect("sent");
    until(&alice, "cleared find", |vm| {
        pv(vm).find.is_none()
            && pv(vm)
                .data
                .assumptions
                .as_ref()
                .is_some_and(|a| a.rows.len() == 2)
    })
    .await;

    drop(alice);
    server.stop().await;
}

#[test]
fn assumptions_view_rows_strict_selection_affects_count_and_conditioned_off() {
    let m = Arc::new(Methodology::iso21434_r155().expect("methodology"));
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");
    let safety = m
        .impact_category("safety")
        .or_else(|| m.impact_category("safety-health"))
        .expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let spoofing = m.threat_category("spoofing").expect("spoofing");

    let mut ws = Workspace::empty(Arc::clone(&m));
    let cf00 = ConditionId::new_v7();
    let asset = AssetId::new_v7();
    let dmg = DamageId::new_v7();

    let t0 = ThreatId::new_v7();
    let tree0_id = TreeId::new_v7();
    let r0 = NodeId::new_v7();
    let s0 = NodeId::new_v7();
    let mut tree0 = AttackTree::new(r0, Node::gate(GateKind::Or, "Tree 0")).expect("r0");
    tree0
        .add_child(tree0.root(), s0, Node::step("Step 0"))
        .expect("s0");

    let t1 = ThreatId::new_v7();
    let tree1_id = TreeId::new_v7();
    let r1 = NodeId::new_v7();
    let s1 = NodeId::new_v7();
    let mut tree1 = AttackTree::new(r1, Node::gate(GateKind::Or, "Tree 1")).expect("r1");
    tree1
        .add_child(tree1.root(), s1, Node::step("Step 1"))
        .expect("s1");

    let t2 = ThreatId::new_v7();
    let tree2_id = TreeId::new_v7();
    let mut tree2 =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Tree 2")).expect("r2");
    tree2
        .add_child(tree2.root(), NodeId::new_v7(), Node::step("Step 2"))
        .expect("s2");

    let tree3_id = TreeId::new_v7();
    let mut tree3 =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Tree 3")).expect("r3");
    tree3
        .add_child(tree3.root(), NodeId::new_v7(), Node::step("Step 3"))
        .expect("s3");

    let cl0 = ClaimId::new_v7();
    let as00 = AssumptionId::new_v7();
    let as01 = AssumptionId::new_v7();

    ws.apply(
        vec![
            Op::AddCondition {
                id: cf00,
                name: "Debug board".into(),
                description: "".into(),
                present: false,
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset,
                name: "ECU".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg,
                name: "Brake failure".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree0_id,
                tree: tree0,
                step_feasibility: vec![],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree1_id,
                tree: tree1,
                step_feasibility: vec![],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree2_id,
                tree: tree2,
                step_feasibility: vec![],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree3_id,
                tree: tree3,
                step_feasibility: vec![],
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t0,
                name: "Threat 0".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t0,
                tree: Some(tree0_id),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t1,
                name: "Threat 1".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t1,
                tree: Some(tree1_id),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t2,
                name: "Threat 2".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t2,
                tree: Some(tree2_id),
                provenance: prov(),
            },
            Op::AddCybersecurityClaim {
                id: cl0,
                name: "Debug port absent".into(),
                provenance: prov(),
            },
            Op::AddAssumption {
                id: as00,
                name: "No debug header".into(),
                holds: true,
                provenance: prov(),
            },
            Op::AddAssumption {
                id: as01,
                name: "Factory sealed".into(),
                holds: false,
                provenance: prov(),
            },
            Op::LinkClaimAssumption {
                claim: cl0,
                assumption: as00,
                provenance: prov(),
            },
            Op::NeedCondition {
                element: ElementRef::Assumption(as00),
                condition: cf00,
                provenance: prov(),
            },
            Op::SetDescription {
                target: ElementRef::Assumption(as00),
                text: "Depopulated in production".into(),
                provenance: prov(),
            },
            Op::SetAssumptionExcludes {
                id: as00,
                trees: vec![tree0_id, tree3_id],
                steps: vec![
                    aegis_model::StepRef {
                        tree: tree0_id,
                        node: s0,
                    },
                    aegis_model::StepRef {
                        tree: tree1_id,
                        node: s1,
                    },
                ],
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup");

    let catalog = Catalog::load(Language::En);

    // Strict non-fallback: UiState::selected_assumption == None -> view.selected == None.
    let none_ui = UiState {
        page: Page::Assumptions,
        selected_assumption: None,
        ..UiState::default()
    };
    let none_view = assumptions_view(&ws, &none_ui, &catalog);
    assert_eq!(none_view.rows.len(), 2);
    assert_eq!(none_view.selected, None);
    assert!(page_descriptions(&ws, &none_ui).is_empty());

    // Stale selected_assumption -> view.selected == None.
    let stale_ui = UiState {
        page: Page::Assumptions,
        selected_assumption: Some(AssumptionId::new_v7()),
        ..UiState::default()
    };
    assert_eq!(assumptions_view(&ws, &stale_ui, &catalog).selected, None);
    assert!(page_descriptions(&ws, &stale_ui).is_empty());

    // Live selected_assumption resolves detail and page_descriptions.
    let ui = UiState {
        page: Page::Assumptions,
        selected_assumption: Some(as00),
        ..UiState::default()
    };
    let view = assumptions_view(&ws, &ui, &catalog);
    assert_eq!(view.rows[0].id, as00);
    assert_eq!(view.rows[0].visual_id, "AS00");
    assert!(view.rows[0].holds);
    assert_eq!(view.rows[0].claims, 1);
    assert_eq!(view.rows[0].excludes, 4);
    assert!(view.rows[0].excluded);
    assert_eq!(
        view.rows[0].excluded_reason.as_deref(),
        Some("Needs CF00 Debug board (off)")
    );
    assert!(!view.rows[1].excluded);
    assert_eq!(view.rows[1].excluded_reason, None);

    let detail = view.selected.expect("detail");
    assert_eq!(detail.id, as00);
    assert_eq!(detail.description, "Depopulated in production");
    assert!(detail.excluded);
    assert_eq!(
        detail.excluded_reason.as_deref(),
        Some("Needs CF00 Debug board (off)")
    );
    assert_eq!(detail.claims.len(), 1);
    assert_eq!(detail.claims[0].visual_id, "CL00");
    assert_eq!(detail.claims[0].selection, Selection::Claim(cl0));
    assert_eq!(detail.conditions.len(), 1);
    assert_eq!(detail.conditions[0].visual_id, "CF00");
    assert_eq!(detail.conditions[0].selection, Selection::Condition(cf00));
    assert_eq!(detail.excludes.len(), 4);
    assert_eq!(detail.excludes.iter().filter(|e| e.is_tree).count(), 2);
    assert_eq!(detail.excludes.iter().filter(|e| !e.is_tree).count(), 2);
    // 2 excluded_trees + 2 direct_steps + 2 affected_threats (t0, t1) = 6.
    assert_eq!(detail.affects_count, 6);
    assert!(detail.can_create_claim);

    let descs = page_descriptions(&ws, &ui);
    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].kind, "assumption");
    assert_eq!(descs[0].target, ElementRef::Assumption(as00));

    let total = view.excluded.elements + view.excluded.steps;
    assert_eq!(total, 1);
    assert_eq!(
        view.explainer,
        "Marking an assumption as holding excludes the trees and steps linked to it. 1 excluded."
    );

    // Turning CF00 on and AS00 off leaves zero exclusions -> assumptions-explainer-none.
    apply_edit(
        &mut ws,
        Edit::SetConditionPresent {
            condition: cf00,
            present: true,
        },
    );
    apply_edit(
        &mut ws,
        Edit::SetAssumptionHolds {
            assumption: as00,
            holds: false,
        },
    );
    let clear = assumptions_view(&ws, &ui, &catalog);
    assert_eq!(
        clear.excluded,
        ExcludedCounts {
            elements: 0,
            steps: 0
        }
    );
    assert_eq!(
        clear.explainer,
        "Marking an assumption as holding excludes the trees and steps linked to it. Nothing is excluded."
    );
    assert!(!clear.rows[0].excluded);
    assert_eq!(clear.rows[0].excluded_reason, None);
}

#[test]
fn exclusion_reasons_entries_and_assumption_switch_toasts() {
    let m = Arc::new(Methodology::iso21434_r155().expect("methodology"));
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");
    let safety = m
        .impact_category("safety")
        .or_else(|| m.impact_category("safety-health"))
        .expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let high = m.feasibility_level("high").expect("high");
    let low = m.feasibility_level("low").expect("low");

    let mut ws = Workspace::empty(Arc::clone(&m));
    let cf_own = ConditionId::new_v7();
    let cf_tree = ConditionId::new_v7();
    let asset = AssetId::new_v7();
    let dmg = DamageId::new_v7();

    let tree0_id = TreeId::new_v7();
    let s0_high = NodeId::new_v7();
    let s0_low = NodeId::new_v7();
    let mut tree0 =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Tamper")).expect("t0");
    tree0
        .add_child(tree0.root(), s0_high, Node::step("Easy tap"))
        .expect("s0_high");
    tree0
        .add_child(tree0.root(), s0_low, Node::step("Hard tap"))
        .expect("s0_low");
    let t0 = ThreatId::new_v7();

    let tree1_id = TreeId::new_v7();
    let s1 = NodeId::new_v7();
    let mut tree1 =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Remote")).expect("t1");
    tree1
        .add_child(tree1.root(), s1, Node::step("OTA debug"))
        .expect("s1");
    let t1 = ThreatId::new_v7();

    let tree2_id = TreeId::new_v7();
    let s2 = NodeId::new_v7();
    let mut tree2 =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Bench")).expect("t2");
    tree2
        .add_child(tree2.root(), s2, Node::step("Bench pin"))
        .expect("s2");
    let t2 = ThreatId::new_v7();

    let as00 = AssumptionId::new_v7();
    let as01 = AssumptionId::new_v7();

    ws.apply(
        vec![
            Op::AddCondition {
                id: cf_own,
                name: "Board rev B".into(),
                description: "".into(),
                present: true,
                provenance: prov(),
            },
            Op::AddCondition {
                id: cf_tree,
                name: "Test harness".into(),
                description: "".into(),
                present: false,
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset,
                name: "ECU".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg,
                name: "Loss of control".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree0_id,
                tree: tree0,
                step_feasibility: vec![(s0_high, high), (s0_low, low)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree1_id,
                tree: tree1,
                step_feasibility: vec![(s1, high)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree2_id,
                tree: tree2,
                step_feasibility: vec![(s2, high)],
                provenance: prov(),
            },
            Op::NeedCondition {
                element: ElementRef::Tree(tree2_id),
                condition: cf_tree,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t0,
                name: "Tamper".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t0,
                tree: Some(tree0_id),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t1,
                name: "Remote".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t1,
                tree: Some(tree1_id),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t2,
                name: "Bench".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t2,
                tree: Some(tree2_id),
                provenance: prov(),
            },
            Op::AddAssumption {
                id: as00,
                name: "Debug fused".into(),
                holds: false,
                provenance: prov(),
            },
            Op::AddAssumption {
                id: as01,
                name: "Spare assumption".into(),
                holds: false,
                provenance: prov(),
            },
            Op::SetAssumptionExcludes {
                id: as00,
                trees: vec![tree2_id],
                steps: vec![
                    aegis_model::StepRef {
                        tree: tree0_id,
                        node: s0_high,
                    },
                    aegis_model::StepRef {
                        tree: tree1_id,
                        node: s1,
                    },
                ],
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup");

    assert_eq!(
        reasons_text(ws.project(), &[Reason::AssumptionHolds(as00)]),
        "Assumption AS00 Debug fused holds"
    );
    assert_eq!(
        reasons_text(
            ws.project(),
            &[Reason::AssumptionHolds(AssumptionId::new_v7())]
        ),
        "An assumption holds"
    );

    // Toggle AS00 to holds = true -> excludes 2 leaf steps and moves 2 risks.
    let ops_on = vec![Op::SetAssumptionHolds {
        id: as00,
        holds: true,
        provenance: prov(),
    }];
    let snap_on = ConditionSnapshot::capture(&ws, &ops_on).expect("capture on");
    ws.apply(ops_on, Origin::Edit).expect("apply on");
    let toast_on = snap_on.toast(&ws);
    assert!(
        toast_on.starts_with(
            "Assumption 'AS00 Debug fused' now holds: 2 steps excluded, 0 included, 2 risks moved ("
        ),
        "{toast_on}"
    );
    assert!(toast_on.contains("not applicable"), "{toast_on}");

    // excluded_entries contains tree2 (combined ConditionOff + AssumptionHolds without duplicate key) and both steps.
    let entries = excluded_entries(ws.project());
    let tree2_vis = pages::selection_visual_id(ws.project(), Selection::Tree(tree2_id));
    let tree2_key = format!("{tree2_vis} · Bench");
    let tree2_matches: Vec<_> = entries.iter().filter(|(k, _)| k == &tree2_key).collect();
    assert_eq!(tree2_matches.len(), 1, "no duplicate tree key: {entries:?}");
    assert_eq!(
        tree2_matches[0].1,
        "Needs CF01 Test harness (off); Assumption AS00 Debug fused holds"
    );
    let keys: BTreeSet<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
    let tree0_vis = pages::selection_visual_id(ws.project(), Selection::Tree(tree0_id));
    let tree1_vis = pages::selection_visual_id(ws.project(), Selection::Tree(tree1_id));
    assert!(keys.contains(format!("{tree0_vis} · Easy tap").as_str()));
    assert!(keys.contains(format!("{tree1_vis} · OTA debug").as_str()));
    assert!(
        entries
            .iter()
            .filter(|(k, _)| k == &format!("{tree0_vis} · Easy tap"))
            .all(|(_, r)| r == "Assumption AS00 Debug fused holds")
    );

    // Toggle AS00 back to holds = false -> restores steps and risks.
    let ops_off = vec![Op::SetAssumptionHolds {
        id: as00,
        holds: false,
        provenance: prov(),
    }];
    let snap_off = ConditionSnapshot::capture(&ws, &ops_off).expect("capture off");
    ws.apply(ops_off, Origin::Edit).expect("apply off");
    let toast_off = snap_off.toast(&ws);
    assert!(
        toast_off.starts_with("Assumption 'AS00 Debug fused' no longer holds: 0 steps excluded, 2 included, 2 risks moved ("),
        "{toast_off}"
    );

    // Condition off on AS00 disables its exclusions: toggling holds gives 0 excluded and zero AssumptionHolds entries.
    ws.apply(
        vec![
            Op::NeedCondition {
                element: ElementRef::Assumption(as00),
                condition: cf_own,
                provenance: prov(),
            },
            Op::SetConditionPresent {
                id: cf_own,
                present: false,
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("disable cf_own");

    let ops_gated = vec![Op::SetAssumptionHolds {
        id: as00,
        holds: true,
        provenance: prov(),
    }];
    let snap_gated = ConditionSnapshot::capture(&ws, &ops_gated).expect("capture gated");
    ws.apply(ops_gated, Origin::Edit).expect("apply gated");
    assert_eq!(
        snap_gated.toast(&ws),
        "Assumption 'AS00 Debug fused' now holds: 0 steps excluded, 0 included, no risk moved"
    );
    let gated_entries = excluded_entries(ws.project());
    assert!(
        !gated_entries
            .iter()
            .any(|(_, r)| r.contains("Assumption AS00")),
        "zero Reason::AssumptionHolds entries when conditioned off: {gated_entries:?}"
    );
    let app = ws.project().applicability();
    assert!(
        !app.excluded_elements()
            .any(|(_, rs)| rs.iter().any(|r| matches!(r, Reason::AssumptionHolds(_))))
    );
    assert!(
        !app.excluded_nodes()
            .any(|(_, _, rs)| rs.iter().any(|r| matches!(r, Reason::AssumptionHolds(_))))
    );

    // Removed assumption toast falls back to "Assumption switched".
    let ops_removed = vec![Op::SetAssumptionHolds {
        id: as01,
        holds: true,
        provenance: prov(),
    }];
    let snap_removed = ConditionSnapshot::capture(&ws, &ops_removed).expect("capture removed");
    ws.apply(vec![Op::RemoveAssumption { id: as01 }], Origin::Edit)
        .expect("remove");
    assert_eq!(
        snap_removed.toast(&ws),
        "Assumption switched: 0 steps excluded, 0 included, no risk moved"
    );
}
