//! `Project::to_workspace_files`: layout, determinism, small diffs.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use aegis_model::{
    AttackTree, DataFlowId, ElementRef, Limit, MAX_SHARED_STEPS, Methodology, ModelError, Node, Op,
    Project, Provenance, StoreError, TrustBoundaryId, WorkspaceFile, WorkspaceProblem,
};
use common::{
    asset_id, builtin, component_id, interface_id, linked_shared_project, node_id, props,
    record_every_op, save, second, shared_step_id, tree_id,
};
use serde_json::{Value, json};

fn data_flow_id(n: u64) -> DataFlowId {
    DataFlowId::from_uuid(common::v7(n)).expect("v7")
}

fn boundary_id(n: u64) -> TrustBoundaryId {
    TrustBoundaryId::from_uuid(common::v7(n)).expect("v7")
}

fn files(p: &Project) -> Vec<WorkspaceFile> {
    p.to_workspace_files().expect("workspace files")
}

fn text(f: &WorkspaceFile) -> &str {
    std::str::from_utf8(&f.bytes).expect("utf8")
}

fn json(f: &WorkspaceFile) -> Value {
    serde_json::from_slice(&f.bytes).expect("json")
}

fn add_asset(m: &Methodology, n: u64, name: &str) -> Op {
    Op::AddAsset {
        id: asset_id(n),
        name: name.into(),
        properties: props(m, &["integrity"]),
        provenance: Provenance::default(),
    }
}

fn add_tree(n: u64) -> Op {
    Op::AddAttackTree {
        id: tree_id(n),
        tree: AttackTree::new(node_id(100 + n), Node::step("Step")).expect("tree"),
        step_feasibility: vec![],
        provenance: Provenance::default(),
    }
}

fn project_with(m: &Arc<Methodology>, ops: &[Op]) -> Project {
    let mut p = Project::new(Arc::clone(m));
    for op in ops {
        p.apply(op.clone()).expect("apply");
    }
    p
}

/// Checks that every non-empty snapshot key of `p` is in exactly one file,
/// and adds the keys to `seen`.
fn check_keys(p: &Project, seen: &mut BTreeSet<String>) {
    let snapshot: Value = serde_json::from_slice(&save(p)).expect("snapshot");
    let Value::Object(map) = snapshot else {
        panic!("snapshot is not an object");
    };
    let files = files(p);
    let empty = |v: &Value| matches!(v, Value::Array(a) if a.is_empty());
    for key in map
        .iter()
        .filter(|(k, v)| *k != "format" && *k != "version" && !empty(v))
        .map(|(k, _)| k)
    {
        let holders = files
            .iter()
            .filter(|f| matches!(json(f), Value::Object(o) if o.contains_key(key)))
            .count();
        assert_eq!(holders, 1, "key {key}");
        seen.insert(key.clone());
    }
}

/// Ops the recorded set leaves out (interfaces, data flows, trust
/// boundaries) plus trees, marks, descriptions, languages and elements.
fn extra_ops() -> (Arc<Methodology>, Vec<Op>) {
    let m = builtin();
    let (c1, c2) = (component_id(1), component_id(2));
    let kind = m.interface_kinds().next().expect("interface kind");
    let comp = |id, name: &str| Op::AddComponent {
        id,
        name: name.into(),
        parent: None,
        provenance: Provenance::default(),
    };
    let ops = [
        add_tree(6),
        Op::SetReviewMark {
            tree: tree_id(6),
            node: node_id(106),
            marked: true,
            provenance: Provenance::default(),
        },
        Op::SetDescription {
            target: ElementRef::Project,
            text: "Notes".into(),
            provenance: Provenance::default(),
        },
        Op::SetProjectLanguages {
            langs: Arc::from([Arc::<str>::from("ja")]),
            provenance: Provenance::default(),
        },
        comp(c1, "ECU"),
        comp(c2, "TCU"),
        Op::AddInterface {
            id: interface_id(3),
            name: "CAN".into(),
            component: c1,
            kind,
            provenance: Provenance::default(),
        },
        Op::AddDataFlow {
            id: data_flow_id(4),
            name: "Speed".into(),
            from: c1,
            to: c2,
            interface: Some(interface_id(3)),
            provenance: Provenance::default(),
        },
        Op::AddTrustBoundary {
            id: boundary_id(5),
            name: "Vehicle".into(),
            components: vec![c1, c2],
            provenance: Provenance::default(),
        },
        Op::AddElement {
            id: common::element_id(7),
            kind: aegis_model::ElementKind::MisuseCase,
            name: "Relay attack".into(),
            provenance: Provenance::default(),
        },
        Op::SetElementAttribute {
            id: common::element_id(7),
            key: "cal".into(),
            value: Some(aegis_model::AttrValue::Level("cal-2".into())),
            provenance: Provenance::default(),
        },
    ];
    (m, ops.into())
}

#[test]
fn layout_paths_are_exactly_the_written_ones() {
    let mut paths = BTreeSet::new();
    let m = second();
    let mut p = Project::new(Arc::clone(&m));
    for op in record_every_op(Arc::clone(&m)) {
        p.apply(op).expect("apply");
        paths.extend(files(&p).into_iter().map(|f| f.path));
    }
    let (m, ops) = extra_ops();
    paths.extend(files(&project_with(&m, &ops)).into_iter().map(|f| f.path));
    for path in &paths {
        assert!(WorkspaceFile::is_layout_path(path), "{path}");
    }
    assert!(paths.contains("aegis.json") && paths.contains("shared/damage.json"));
    assert!(paths.iter().any(|p| p.starts_with("systems/main/trees/")));
    let fixed =
        ["item", "assets", "threats", "assumptions"].map(|s| format!("systems/main/{s}.json"));
    assert!(paths.iter().any(|p| {
        let rest = p.strip_prefix("systems/main/").unwrap_or("/");
        !rest.contains('/') && rest.ends_with(".json") && !fixed.contains(p)
    }));
    let long = format!("systems/main/trees/{}.json", "a".repeat(300));
    for bad in [
        ".git/config.json",
        "manifest.json",
        "notes.json",
        "shared/other.json",
        "systems/other/assets.json",
        "systems/main/trees/a/b.json",
        "systems/main/trees/.json",
        "../aegis.json",
        "Aegis.json",
        "",
        &long,
    ] {
        assert!(!WorkspaceFile::is_layout_path(bad), "{bad}");
    }
}

#[test]
fn layout_covers_every_snapshot_key() {
    let mut seen = BTreeSet::new();
    // The recorded ops end by tearing the project down, so check after
    // every op and require that all keys were seen along the way.
    let m = second();
    let mut p = Project::new(Arc::clone(&m));
    for op in record_every_op(Arc::clone(&m)) {
        p.apply(op).expect("apply");
        check_keys(&p, &mut seen);
    }
    let (m, ops) = extra_ops();
    check_keys(&project_with(&m, &ops), &mut seen);
    for key in [
        "methodology",
        "assets",
        "damage_scenarios",
        "threat_scenarios",
        "attack_trees",
        "security_controls",
        "cybersecurity_goals",
        "cybersecurity_claims",
        "assumptions",
        "components",
        "interfaces",
        "data_flows",
        "trust_boundaries",
        "descriptions",
        "review_marks",
        "languages",
        "elements",
    ] {
        assert!(seen.contains(key), "never saw {key}");
    }
}

#[test]
fn same_project_same_bytes() {
    let m = builtin();
    let p = project_with(&m, &[add_asset(&m, 1, "ECU"), add_tree(2)]);
    assert_eq!(files(&p), files(&p));
}

#[test]
fn rename_changes_one_line_in_one_file() {
    let m = builtin();
    let mut p = project_with(
        &m,
        &[
            add_asset(&m, 1, "ECU"),
            add_asset(&m, 2, "TCU"),
            add_tree(3),
        ],
    );
    let before = files(&p);
    p.apply(Op::RenameAsset {
        id: asset_id(1),
        name: "Brake ECU".into(),
        provenance: Provenance::default(),
    })
    .expect("rename");
    let after = files(&p);
    assert_eq!(before.len(), after.len());
    let changed: Vec<_> = before.iter().zip(&after).filter(|(a, b)| a != b).collect();
    assert_eq!(changed.len(), 1);
    let (a, b) = changed[0];
    assert_eq!(a.path, b.path);
    let (ta, tb) = (text(a), text(b));
    assert_eq!(ta.lines().count(), tb.lines().count());
    let differing = ta.lines().zip(tb.lines()).filter(|(x, y)| x != y).count();
    assert_eq!(differing, 1);
}

#[test]
fn empty_project_writes_only_the_workspace_file() {
    let p = Project::new(builtin());
    let files = files(&p);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "aegis.json");
}

#[test]
fn one_file_per_tree_named_by_visual_id() {
    let m = builtin();
    let p = project_with(&m, &[add_tree(1), add_tree(2)]);
    let paths: Vec<_> = files(&p).into_iter().map(|f| f.path).collect();
    assert_eq!(
        paths,
        [
            "aegis.json",
            "systems/main/trees/at00.json",
            "systems/main/trees/at01.json"
        ]
    );
    for f in files(&p).iter().filter(|f| f.path.contains("/trees/")) {
        let Value::Object(o) = json(f) else {
            panic!("not an object");
        };
        assert!(matches!(&o["attack_trees"], Value::Array(a) if a.len() == 1));
    }
}

#[test]
fn every_file_starts_with_the_envelope() {
    let m = builtin();
    let p = project_with(&m, &[add_asset(&m, 1, "ECU"), add_tree(2)]);
    let all = files(&p);
    assert!(all.len() >= 3);
    for f in &all {
        let t = text(f);
        let mut lines = t.lines();
        assert_eq!(lines.next(), Some("{"));
        assert_eq!(lines.next(), Some(r#"  "format": "aegis-workspace","#));
        assert_eq!(lines.next(), Some(r#"  "version": 1,"#));
        assert!(
            lines
                .next()
                .is_some_and(|l| l.starts_with(r#"  "kind": ""#))
        );
        assert!(t.ends_with("}\n") && !t.ends_with("\n\n"));
    }
}

// ---------------------------------------------------------------------------
// Reading back
// ---------------------------------------------------------------------------

fn read(files: &[WorkspaceFile], m: &Arc<Methodology>) -> Result<Project, aegis_model::StoreError> {
    Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(m),
    )
}

fn assert_round_trip(p: &Project, m: &Arc<Methodology>) {
    let back = read(&files(p), m).expect("read back");
    assert_eq!(&back, p);
    assert_eq!(save(&back), save(p));
}

fn replace(files: &mut [WorkspaceFile], path: &str, bytes: &[u8]) {
    let f = files.iter_mut().find(|f| f.path == path).expect("file");
    f.bytes = bytes.to_vec();
}

fn rewrite(files: &mut [WorkspaceFile], path: &str, edit: impl FnOnce(&mut Value)) {
    let f = files.iter_mut().find(|f| f.path == path).expect("file");
    let mut v = json(f);
    edit(&mut v);
    f.bytes = serde_json::to_vec(&v).expect("encode");
}

fn envelope_problem(err: Option<aegis_model::StoreError>, want_path: &str) -> WorkspaceProblem {
    match err {
        Some(StoreError::WorkspaceEnvelope { path, problem }) => {
            assert_eq!(path, want_path);
            problem
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn round_trip_fixture() {
    let m = builtin();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/project-v1.json"
    ))
    .expect("fixture");
    let p = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_round_trip(&p, &m);
    assert_eq!(save(&read(&files(&p), &m).expect("read")), save(&p));
}

#[test]
fn round_trip_every_kind() {
    let (m, ops) = extra_ops();
    assert_round_trip(&project_with(&m, &ops), &m);
    let m = second();
    let mut p = Project::new(Arc::clone(&m));
    for op in record_every_op(Arc::clone(&m)) {
        p.apply(op).expect("apply");
        assert_round_trip(&p, &m);
    }
}

#[test]
fn file_order_does_not_matter() {
    let (m, ops) = extra_ops();
    let p = project_with(&m, &ops);
    let mut all = files(&p);
    assert!(all.len() > 3);
    all.reverse();
    let back = read(&all, &m).expect("read");
    assert_eq!(back, p);
    assert_eq!(save(&back), save(&p));
}

#[test]
fn many_trees_keep_their_visual_ids() {
    let m = builtin();
    let ops: Vec<Op> = (1..=105).map(add_tree).collect();
    let p = project_with(&m, &ops);
    let mut all = files(&p);
    let back = read(&all, &m).expect("read");
    assert_eq!(save(&back), save(&p));
    all.reverse();
    let back = read(&all, &m).expect("read reversed");
    assert_eq!(save(&back), save(&p));
}

#[test]
fn missing_workspace_file() {
    let m = builtin();
    let p = project_with(&m, &[add_asset(&m, 1, "ECU")]);
    let all: Vec<_> = files(&p)
        .into_iter()
        .filter(|f| f.path != "aegis.json")
        .collect();
    let err = read(&all, &m).err();
    assert!(matches!(err, Some(StoreError::WorkspaceMissing { path }) if path == "aegis.json"));
}

#[test]
fn unknown_path() {
    let m = builtin();
    let mut all = files(&Project::new(Arc::clone(&m)));
    all.push(WorkspaceFile {
        path: "shared/other.json".into(),
        bytes: b"{}".to_vec(),
    });
    let err = read(&all, &m).err();
    assert!(matches!(err, Some(StoreError::WorkspacePath { path }) if path == "shared/other.json"));
    // A long path is cut at a character boundary.
    let long = "é".repeat(300);
    all.last_mut().expect("file").path = long;
    let Some(StoreError::WorkspacePath { path }) = read(&all, &m).err() else {
        panic!("expected WorkspacePath");
    };
    assert_eq!(path.len(), 256);
}

#[test]
fn wrong_format() {
    let m = builtin();
    let mut all = files(&Project::new(Arc::clone(&m)));
    rewrite(&mut all, "aegis.json", |v| {
        v["format"] = "aegis-project".into()
    });
    let err = read(&all, &m).err();
    assert_eq!(
        envelope_problem(err, "aegis.json"),
        WorkspaceProblem::Format
    );
}

#[test]
fn newer_version() {
    let m = builtin();
    let mut all = files(&Project::new(Arc::clone(&m)));
    rewrite(&mut all, "aegis.json", |v| v["version"] = 2.into());
    let err = read(&all, &m).err();
    assert_eq!(
        envelope_problem(err, "aegis.json"),
        WorkspaceProblem::Version
    );
}

#[test]
fn kind_does_not_match_path() {
    let m = builtin();
    let mut all = files(&project_with(&m, &[add_asset(&m, 1, "ECU")]));
    rewrite(&mut all, "systems/main/assets.json", |v| {
        v["kind"] = "threats".into()
    });
    let err = read(&all, &m).err();
    assert_eq!(
        envelope_problem(err, "systems/main/assets.json"),
        WorkspaceProblem::Kind
    );
}

#[test]
fn not_json() {
    let m = builtin();
    let mut all = files(&Project::new(Arc::clone(&m)));
    replace(&mut all, "aegis.json", b"{not json");
    let err = read(&all, &m).err();
    assert_eq!(envelope_problem(err, "aegis.json"), WorkspaceProblem::Json);
}

// Systems (119a).

fn add_system_op(n: u64, name: &str) -> Op {
    Op::AddElement {
        id: common::element_id(n),
        kind: aegis_model::ElementKind::System,
        name: name.into(),
        provenance: Provenance::default(),
    }
}

#[test]
fn systems_have_their_own_file() {
    let m = builtin();
    let p = project_with(&m, &[add_system_op(1, "Brake"), add_system_op(2, "ADAS")]);
    let all = files(&p);
    let f = all
        .iter()
        .find(|f| f.path == "systems/main/systems.json")
        .expect("systems file");
    assert_eq!(json(f)["kind"], "system");
    assert_round_trip(&p, &m);

    let none = project_with(&m, &[add_asset(&m, 1, "ECU")]);
    assert!(
        files(&none)
            .iter()
            .all(|f| f.path != "systems/main/systems.json")
    );
}

#[test]
fn old_fixture_has_no_systems() {
    let m = builtin();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/project-v1.json"
    ))
    .expect("fixture");
    let p = Project::load(bytes.as_slice(), m).expect("load");
    assert_eq!(p.element_count(aegis_model::ElementKind::System), 0);
}

#[test]
fn shared_steps_workspace_round_trips_linked_trees() {
    let m = builtin();
    let (p, shared, _, node) = linked_shared_project();
    let all = files(&p);
    let steps = all
        .iter()
        .find(|f| f.path == "shared/steps.json")
        .expect("file");
    let root = json(steps);
    assert_eq!(root["format"], "aegis-workspace");
    assert_eq!(root["version"], 1);
    assert_eq!(root["kind"], "shared-steps");
    assert_eq!(root["shared_steps"][0]["id"], shared.to_string());
    let tree = all
        .iter()
        .find(|f| f.path.starts_with("systems/main/trees/"))
        .expect("tree file");
    let tree = json(tree)["attack_trees"][0].clone();
    assert!(
        tree["step_feasibility"]
            .as_object()
            .is_some_and(|f| f.is_empty())
    );
    for key in ["step_factors", "step_via", "step_attack_type"] {
        assert!(
            tree.get(key).is_none(),
            "{key} must not duplicate shared ratings"
        );
    }
    assert_eq!(
        tree["step_shared"],
        json!([{"node": node.to_string(), "shared_step": shared.to_string()}])
    );
    let mut reversed = all;
    reversed.reverse();
    let back = read(&reversed, &m).expect("read reversed");
    assert_eq!(back, p);
    assert_eq!(save(&back), save(&p));
    assert_eq!(
        back.description_translation(ElementRef::SharedStep(shared), "ja"),
        Some("共有の診断アクセス")
    );
}

#[test]
fn shared_steps_workspace_shapes_are_strict() {
    let m = builtin();
    let (p, _, _, _) = linked_shared_project();
    let mut all = files(&p);
    rewrite(&mut all, "shared/steps.json", |v| v["extra"] = json!(1));
    assert!(matches!(
        read(&all, &m),
        Err(StoreError::WorkspaceUnknownField { path, field })
            if path == "shared/steps.json" && field == "extra"
    ));

    let mut all = files(&p);
    rewrite(&mut all, "shared/steps.json", |v| {
        v["shared_steps"][0]["extra"] = json!(1)
    });
    let Err(StoreError::InWorkspace { path, source }) = read(&all, &m) else {
        panic!("expected inner wire failure");
    };
    assert_eq!(path, "aegis.json");
    assert!(matches!(*source, StoreError::Decode { .. }));

    let mut all = files(&p);
    let tree_path = all
        .iter()
        .find(|f| text(f).contains("step_shared"))
        .expect("tree")
        .path
        .clone();
    rewrite(&mut all, &tree_path, |v| {
        v["attack_trees"][0]["step_shared"][0]["extra"] = json!(1)
    });
    let Err(StoreError::InWorkspace { path, source }) = read(&all, &m) else {
        panic!("expected inner wire failure");
    };
    assert_eq!(path, "aegis.json");
    assert!(matches!(*source, StoreError::Decode { .. }));

    let mut all = files(&p);
    rewrite(&mut all, &tree_path, |v| {
        let link = v["attack_trees"][0]["step_shared"][0].clone();
        v["attack_trees"][0]["step_shared"]
            .as_array_mut()
            .expect("links")
            .push(link);
    });
    let Err(StoreError::InWorkspace { path, source }) = read(&all, &m) else {
        panic!("expected duplicate link failure");
    };
    assert_eq!(path, "aegis.json");
    assert!(matches!(*source, StoreError::DuplicateEntry));
}

#[test]
fn v1_workspace_without_shared_steps_file_loads() {
    let m = builtin();
    let (p, _, _, _) = linked_shared_project();
    let mut all = files(&p);
    all.retain(|f| f.path != "shared/steps.json" && f.path != "descriptions.json");
    for f in all
        .iter_mut()
        .filter(|f| f.path.starts_with("systems/main/trees/"))
    {
        let mut value = json(f);
        value["attack_trees"][0]
            .as_object_mut()
            .expect("tree")
            .remove("step_shared");
        f.bytes = serde_json::to_vec(&value).expect("json");
    }
    let back = read(&all, &m).expect("v1 workspace loads");
    assert_eq!(back.shared_step_count(), 0);
    assert_eq!(back.attack_tree_count(), 1);
    assert!(files(&back).iter().all(|f| f.path != "shared/steps.json"));
}

#[test]
fn duplicate_shared_step_blame_routes_to_its_file() {
    let m = builtin();
    let (p, shared, _, _) = linked_shared_project();
    let mut all = files(&p);
    rewrite(&mut all, "shared/steps.json", |v| {
        let first = v["shared_steps"][0].clone();
        v["shared_steps"].as_array_mut().expect("steps").push(first);
    });
    let Err(StoreError::InWorkspace { path, source }) = read(&all, &m) else {
        panic!("expected duplicate shared step");
    };
    assert_eq!(path, "shared/steps.json");
    assert!(matches!(
        *source,
        StoreError::Model(ModelError::DuplicateSharedStep(id)) if id == shared
    ));
}

#[test]
fn shared_steps_workspace_cap_is_isolated() {
    let m = builtin();
    let mut all = files(&Project::new(Arc::clone(&m)));
    let steps: Vec<Value> = (0..=MAX_SHARED_STEPS)
        .map(|i| json!({"id": shared_step_id(3_000_000 + i as u64).to_string(), "name": "s"}))
        .collect();
    all.push(WorkspaceFile {
        path: "shared/steps.json".into(),
        bytes: serde_json::to_vec(&json!({
            "format": "aegis-workspace",
            "version": 1,
            "kind": "shared-steps",
            "shared_steps": steps,
        }))
        .expect("json"),
    });
    let Err(StoreError::InWorkspace { source, .. }) = read(&all, &m) else {
        panic!("expected shared-step cap");
    };
    assert!(matches!(
        *source,
        StoreError::LimitExceeded(Limit::SharedSteps {
            max: MAX_SHARED_STEPS
        })
    ));
}
