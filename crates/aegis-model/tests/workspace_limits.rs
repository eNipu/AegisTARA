//! `Project::from_workspace_files` on hostile input: caps, path rules,
//! refusals, and which file an error names.

mod common;

use std::sync::Arc;

use aegis_model::{
    AttackTree, Limit, MAX_PROJECT_FILE_BYTES, MAX_WORKSPACE_FILE_BYTES, MAX_WORKSPACE_FILES,
    Methodology, Node, Op, Project, Provenance, StoreError, WorkspaceFile,
};
use common::{asset_id, builtin, node_id, second, threat_id, tree_id};
use serde_json::Value;

fn add_tree(n: u64) -> Op {
    Op::AddAttackTree {
        role: Default::default(),
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

fn files(p: &Project) -> Vec<WorkspaceFile> {
    p.to_workspace_files().expect("files")
}

fn read(files: &[WorkspaceFile], m: &Arc<Methodology>) -> Result<Project, StoreError> {
    Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(m),
    )
}

fn rewrite(files: &mut [WorkspaceFile], path: &str, edit: impl FnOnce(&mut Value)) {
    let f = files.iter_mut().find(|f| f.path == path).expect("file");
    let mut v: Value = serde_json::from_slice(&f.bytes).expect("json");
    edit(&mut v);
    f.bytes = serde_json::to_vec(&v).expect("encode");
}

fn is_limit(err: &Option<StoreError>, want: Limit) -> bool {
    matches!(err, Some(StoreError::LimitExceeded(l)) if *l == want)
}

const HEAD: &[u8] = br#"{"format":"aegis-workspace","version":1,"kind":"workspace"}"#;

fn limit_files() -> Limit {
    Limit::WorkspaceFiles {
        max: MAX_WORKSPACE_FILES,
    }
}

fn limit_file_bytes() -> Limit {
    Limit::WorkspaceFileBytes {
        max: MAX_WORKSPACE_FILE_BYTES,
    }
}

// -- Caps ------------------------------------------------------------------

#[test]
fn file_count_cap() {
    let m = builtin();
    let paths: Vec<String> = (0..=MAX_WORKSPACE_FILES)
        .map(|n| format!("systems/main/trees/at{n:02}.json"))
        .collect();
    let run = |count: usize| {
        let items = std::iter::once(("aegis.json", HEAD)).chain(
            paths
                .iter()
                .take(count - 1)
                .map(|p| (p.as_str(), &b"{}"[..])),
        );
        Project::from_workspace_files(items, Arc::clone(&m)).err()
    };
    assert!(!is_limit(&run(MAX_WORKSPACE_FILES), limit_files()));
    assert!(is_limit(&run(MAX_WORKSPACE_FILES + 1), limit_files()));
}

#[test]
fn file_count_cap_stops_an_endless_iterator() {
    let m = builtin();
    let names: Vec<String> = (0..=MAX_WORKSPACE_FILES)
        .map(|n| format!("systems/main/trees/at{n}.json"))
        .collect();
    let endless = names
        .iter()
        .map(|p| (p.as_str(), &b"{}"[..]))
        .chain(std::iter::repeat(("shared/damage.json", &b"{}"[..])));
    let err = Project::from_workspace_files(endless, m).err();
    assert!(is_limit(&err, limit_files()));
}

#[test]
fn single_file_size_cap() {
    let m = builtin();
    let big = |len: u64| {
        let mut v = HEAD.to_vec();
        v.resize(usize::try_from(len).expect("len"), b' ');
        v
    };
    let under = big(MAX_WORKSPACE_FILE_BYTES);
    let over = big(MAX_WORKSPACE_FILE_BYTES + 1);
    let err =
        Project::from_workspace_files([("aegis.json", under.as_slice())], Arc::clone(&m)).err();
    assert!(
        !matches!(err, Some(StoreError::LimitExceeded(_))),
        "{err:?}"
    );
    let err = Project::from_workspace_files([("aegis.json", over.as_slice())], m).err();
    assert!(is_limit(&err, limit_file_bytes()));
}

#[test]
fn total_size_cap() {
    let m = builtin();
    let chunk = usize::try_from(MAX_WORKSPACE_FILE_BYTES).expect("len");
    let mut body = HEAD.to_vec();
    body.resize(chunk, b' ');
    let paths = [
        "aegis.json",
        "descriptions.json",
        "review-marks.json",
        "shared/damage.json",
    ];
    assert_eq!(
        MAX_PROJECT_FILE_BYTES,
        MAX_WORKSPACE_FILE_BYTES * paths.len() as u64
    );
    let all: Vec<(&str, &[u8])> = paths.iter().map(|p| (*p, body.as_slice())).collect();
    let err = Project::from_workspace_files(all.clone(), Arc::clone(&m)).err();
    assert!(
        !matches!(err, Some(StoreError::LimitExceeded(_))),
        "{err:?}"
    );
    let mut over = all;
    over.push(("shared/goals.json", &b"{}"[..]));
    let err = Project::from_workspace_files(over, m).err();
    assert!(is_limit(
        &err,
        Limit::ProjectFileBytes {
            max: MAX_PROJECT_FILE_BYTES
        }
    ));
}

// -- Path rules ------------------------------------------------------------

#[test]
fn path_rules() {
    let m = builtin();
    let long = format!("shared/{}.json", "a".repeat(260));
    for path in [
        long.as_str(),
        "",
        "/aegis.json",
        "systems\\main\\item.json",
        "shared/../aegis.json",
        "./aegis.json",
        "systems/./main/item.json",
        "Aegis.json",
        "systems/main/trees/AT00.json",
        "aegis.json\0",
        "shared/damagé.json",
    ] {
        let items = [("aegis.json", HEAD), (path, HEAD)];
        let err = Project::from_workspace_files(items, Arc::clone(&m)).err();
        assert!(
            matches!(&err, Some(StoreError::WorkspacePath { .. })),
            "{path:?}: {err:?}"
        );
    }
}

#[test]
fn repeated_path() {
    let m = builtin();
    let items = [("aegis.json", HEAD), ("aegis.json", HEAD)];
    let err = Project::from_workspace_files(items, m).err();
    assert!(
        matches!(err, Some(StoreError::WorkspaceDuplicatePath { path }) if path == "aegis.json")
    );
}

// -- Refusals --------------------------------------------------------------

#[test]
fn unknown_field() {
    let m = builtin();
    let mut all = files(&project_with(&m, &[add_tree(1)]));
    rewrite(&mut all, "aegis.json", |v| {
        v["assets"] = Value::Array(vec![])
    });
    let err = read(&all, &m).err();
    assert!(
        matches!(&err, Some(StoreError::WorkspaceUnknownField { path, field })
        if path == "aegis.json" && field == "assets")
    );
    let long = "f".repeat(1000);
    rewrite(&mut all, "systems/main/trees/at00.json", |v| {
        v[long.as_str()] = Value::Null;
    });
    rewrite(&mut all, "aegis.json", |v| {
        v.as_object_mut().expect("object").remove("assets");
    });
    let Some(StoreError::WorkspaceUnknownField { path, field }) = read(&all, &m).err() else {
        panic!("expected WorkspaceUnknownField");
    };
    assert_eq!(path, "systems/main/trees/at00.json");
    assert_eq!(field.len(), 64);
}

#[test]
fn tree_file_names() {
    let m = builtin();
    let base = files(&project_with(&m, &[add_tree(1), add_tree(2)]));
    let tree_name = |all: &[WorkspaceFile], want: &str| {
        let err = read(all, &m).err();
        assert!(
            matches!(&err, Some(StoreError::WorkspaceTreeName { path }) if path == want),
            "{err:?}"
        );
    };
    // Two trees in one file, and none.
    let mut all = base.clone();
    rewrite(&mut all, "systems/main/trees/at00.json", |v| {
        let tree = v["attack_trees"][0].clone();
        v["attack_trees"].as_array_mut().expect("list").push(tree);
    });
    tree_name(&all, "systems/main/trees/at00.json");
    let mut all = base.clone();
    rewrite(&mut all, "systems/main/trees/at00.json", |v| {
        v["attack_trees"] = Value::Array(vec![]);
    });
    tree_name(&all, "systems/main/trees/at00.json");
    // The name differs from the tree's visual ID.
    let mut all = base.clone();
    rewrite(&mut all, "systems/main/trees/at00.json", |v| {
        v["attack_trees"][0]["visual_id"] = "AT09".into();
    });
    tree_name(&all, "systems/main/trees/at00.json");
    // A name with no number, or a non-canonical one.
    for name in ["tree", "at5", "at"] {
        let mut all = base.clone();
        let path = format!("systems/main/trees/{name}.json");
        let f = all
            .iter_mut()
            .find(|f| f.path == "systems/main/trees/at00.json")
            .expect("file");
        f.path.clone_from(&path);
        tree_name(&all, &path);
    }
}

#[test]
fn other_methodology() {
    let m = builtin();
    let all = files(&Project::new(Arc::clone(&m)));
    let err = read(&all, &second()).err();
    assert!(matches!(err, Some(StoreError::WorkspaceMethodology { path }) if path == "aegis.json"));
}

// -- Which file ------------------------------------------------------------

#[test]
fn repeated_tree_id_names_the_second_file() {
    let m = builtin();
    let mut all = files(&project_with(&m, &[add_tree(1), add_tree(2), add_tree(3)]));
    let first_id = {
        let f = all
            .iter()
            .find(|f| f.path == "systems/main/trees/at01.json")
            .expect("file");
        let v: Value = serde_json::from_slice(&f.bytes).expect("json");
        v["attack_trees"][0]["id"].clone()
    };
    rewrite(&mut all, "systems/main/trees/at02.json", |v| {
        v["attack_trees"][0]["id"] = first_id;
    });
    let Some(StoreError::InWorkspace { path, source }) = read(&all, &m).err() else {
        panic!("expected InWorkspace");
    };
    assert_eq!(path, "systems/main/trees/at02.json");
    assert!(matches!(*source, StoreError::Model(_)));
}

#[test]
fn threat_with_no_asset_names_the_threats_file() {
    let m = builtin();
    let category = m.threat_categories().next().expect("category");
    let p = project_with(
        &m,
        &[
            Op::AddAsset {
                id: asset_id(1),
                name: "ECU".into(),
                properties: m.properties().collect(),
                provenance: Provenance::default(),
            },
            Op::AddThreatScenario {
                id: threat_id(2),
                name: "Spoof".into(),
                asset: asset_id(1),
                category,
                catalog_refs: vec![],
                damages: vec![],
                provenance: Provenance::default(),
            },
        ],
    );
    let all = files(&p);
    assert!(read(&all, &m).is_ok());
    let without: Vec<WorkspaceFile> = all
        .into_iter()
        .filter(|f| f.path != "systems/main/assets.json")
        .collect();
    let Some(StoreError::InWorkspace { path, .. }) = read(&without, &m).err() else {
        panic!("expected InWorkspace");
    };
    assert_eq!(path, "systems/main/threats.json");
}

#[test]
fn unplaceable_error_names_the_workspace_file() {
    let m = builtin();
    let mut all = files(&Project::new(Arc::clone(&m)));
    rewrite(&mut all, "aegis.json", |v| v["languages"] = 5.into());
    let Some(StoreError::InWorkspace { path, .. }) = read(&all, &m).err() else {
        panic!("expected InWorkspace");
    };
    assert_eq!(path, "aegis.json");
}

// -- Error text ------------------------------------------------------------

#[test]
fn error_text_stays_short() {
    let m = builtin();
    let long = "\u{1}".repeat(1 << 20);
    let long_tree = format!("systems/main/trees/{}.json", "a".repeat(1 << 20));
    let mut errors: Vec<StoreError> = Vec::new();
    // Paths: unknown, repeated, bad envelope, unknown field, tree name.
    errors.extend(
        Project::from_workspace_files(
            [("aegis.json", HEAD), (long.as_str(), HEAD)],
            Arc::clone(&m),
        )
        .err(),
    );
    errors.extend(
        Project::from_workspace_files(
            [(long_tree.as_str(), HEAD), (long_tree.as_str(), HEAD)],
            Arc::clone(&m),
        )
        .err(),
    );
    errors.extend(
        Project::from_workspace_files(
            [("aegis.json", HEAD), (long_tree.as_str(), HEAD)],
            Arc::clone(&m),
        )
        .err(),
    );
    let field_file =
        format!(r#"{{"format":"aegis-workspace","version":1,"kind":"workspace","{long}":1}}"#);
    errors.extend(
        Project::from_workspace_files([("aegis.json", field_file.as_bytes())], Arc::clone(&m))
            .err(),
    );
    errors.extend(
        Project::from_workspace_files([("aegis.json", &b"nope"[..])], Arc::clone(&m)).err(),
    );
    let tree = br#"{"format":"aegis-workspace","version":1,"kind":"tree","attack_trees":[]}"#;
    let name = format!("systems/main/trees/at00{}.json", "0".repeat(1 << 20));
    errors.extend(
        Project::from_workspace_files(
            [("aegis.json", HEAD), (name.as_str(), &tree[..])],
            Arc::clone(&m),
        )
        .err(),
    );
    errors.extend(Project::from_workspace_files(Vec::<(&str, &[u8])>::new(), Arc::clone(&m)).err());
    errors.extend(read(&files(&Project::new(Arc::clone(&m))), &second()).err());
    assert!(errors.len() >= 7, "{}", errors.len());
    for e in &errors {
        assert!(
            e.to_string().len() <= 512,
            "{} bytes: {e:?}",
            e.to_string().len()
        );
    }
}
