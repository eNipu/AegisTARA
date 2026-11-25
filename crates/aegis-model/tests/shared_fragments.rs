//! Tests of shared fragment trees: the immutable tree role, its visual ID
//! prefix, undo, and persistence in snapshots, op logs and workspace files.

mod common;

use std::sync::Arc;

use aegis_model::{
    AttackTree, Methodology, Node, Op, Project, Provenance, StoreError, TreeId, TreeRole,
};
use common::{builtin, node_id, read_log, save, tree_id, write_log};
use serde_json::Value;

fn add_tree(n: u64, role: TreeRole) -> Op {
    Op::AddAttackTree {
        role,
        id: tree_id(n),
        tree: AttackTree::new(node_id(100 + n), Node::step("Step")).expect("tree"),
        step_feasibility: vec![],
        provenance: Provenance::human("alice", n),
    }
}

fn project_with(m: &Arc<Methodology>, ops: &[Op]) -> Project {
    let mut p = Project::new(Arc::clone(m));
    for op in ops {
        p.apply(op.clone()).expect("apply");
    }
    p
}

fn visual(p: &Project, id: TreeId) -> String {
    let idx = p.attack_tree_idx(&id).expect("live tree");
    p.attack_tree_visual_id(idx).expect("visual id")
}

fn role(p: &Project, id: TreeId) -> TreeRole {
    let idx = p.attack_tree_idx(&id).expect("live tree");
    p.attack_tree_role(idx).expect("role")
}

/// Threat trees and fragments share one sequence, so a number is never
/// reused across prefixes and a tree keeps its ID for life.
fn mixed(m: &Arc<Methodology>) -> Project {
    project_with(
        m,
        &[
            add_tree(1, TreeRole::Threat),
            add_tree(2, TreeRole::Fragment),
            add_tree(3, TreeRole::Threat),
        ],
    )
}

#[test]
fn role_sets_the_visual_prefix_and_never_reuses_a_number() {
    let m = builtin();
    let p = mixed(&m);
    assert_eq!(visual(&p, tree_id(1)), "AT00");
    assert_eq!(visual(&p, tree_id(2)), "TT01");
    assert_eq!(visual(&p, tree_id(3)), "AT02");
    assert_eq!(role(&p, tree_id(1)), TreeRole::Threat);
    assert_eq!(role(&p, tree_id(2)), TreeRole::Fragment);
    assert_eq!(TreeRole::default(), TreeRole::Threat);
}

#[test]
fn remove_and_undo_restore_the_fragment_role_and_id() {
    let m = builtin();
    let mut p = mixed(&m);
    let before = p.clone();
    let inverse = p
        .apply(Op::RemoveAttackTree { id: tree_id(2) })
        .expect("remove");
    let Op::AddAttackTree { role, .. } = &inverse else {
        panic!("inverse is not AddAttackTree: {inverse:?}");
    };
    assert_eq!(*role, TreeRole::Fragment);
    p.apply(inverse).expect("undo");
    assert_eq!(p, before);
    assert_eq!(visual(&p, tree_id(2)), "TT01");
}

#[test]
fn projects_differing_only_in_role_are_not_equal() {
    let m = builtin();
    let a = project_with(&m, &[add_tree(1, TreeRole::Threat)]);
    let b = project_with(&m, &[add_tree(1, TreeRole::Fragment)]);
    assert_ne!(a, b);
    assert_eq!(a.diff(&b).expect("diff").attack_trees().modified().len(), 1);
}

#[test]
fn snapshot_round_trips_roles_and_omits_the_default() {
    let m = builtin();
    let p = mixed(&m);
    let bytes = save(&p);
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    let trees = json["attack_trees"].as_array().expect("trees");
    assert!(trees[0].get("role").is_none(), "threat role is omitted");
    assert_eq!(trees[1]["role"], "fragment");
    assert!(trees[2].get("role").is_none());
    let back = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(back, p);
    assert_eq!(visual(&back, tree_id(2)), "TT01");
}

fn load_value(m: &Arc<Methodology>, json: &Value) -> Result<Project, StoreError> {
    let bytes = serde_json::to_vec(json).expect("encode");
    Project::load(bytes.as_slice(), Arc::clone(m))
}

#[test]
fn snapshot_rejects_a_visual_id_whose_prefix_contradicts_the_role() {
    let m = builtin();
    let p = mixed(&m);
    let good: Value = serde_json::from_slice(&save(&p)).expect("json");

    // The fragment is at position 1 but has number 1, so its visual ID is
    // omitted; write it explicitly with the wrong prefix.
    let mut wrong_prefix = good.clone();
    wrong_prefix["attack_trees"][1]["visual_id"] = Value::from("AT01");
    assert!(matches!(
        load_value(&m, &wrong_prefix),
        Err(StoreError::InvalidId)
    ));

    let mut threat_as_fragment = good.clone();
    threat_as_fragment["attack_trees"][0]["visual_id"] = Value::from("TT00");
    assert!(matches!(
        load_value(&m, &threat_as_fragment),
        Err(StoreError::InvalidId)
    ));

    let mut matching = good.clone();
    matching["attack_trees"][1]["visual_id"] = Value::from("TT01");
    assert_eq!(load_value(&m, &matching).expect("load"), p);

    let mut unknown = good;
    unknown["attack_trees"][1]["role"] = Value::from("bypass");
    assert!(load_value(&m, &unknown).is_err(), "unknown role is refused");
}

#[test]
fn legacy_snapshot_without_role_loads_as_threat_trees() {
    let m = builtin();
    let p = project_with(&m, &[add_tree(1, TreeRole::Threat)]);
    let mut json: Value = serde_json::from_slice(&save(&p)).expect("json");
    json["attack_trees"][0]
        .as_object_mut()
        .expect("tree")
        .remove("role");
    let back = load_value(&m, &json).expect("load");
    assert_eq!(role(&back, tree_id(1)), TreeRole::Threat);
    assert_eq!(visual(&back, tree_id(1)), "AT00");
}

#[test]
fn op_log_round_trips_the_role_and_omits_the_default() {
    let m = builtin();
    let ops = [
        add_tree(1, TreeRole::Threat),
        add_tree(2, TreeRole::Fragment),
    ];
    let bytes = write_log(&m, &ops);
    let text = std::str::from_utf8(&bytes).expect("utf8");
    let lines: Vec<&str> = text.lines().collect();
    assert!(!lines[1].contains("\"role\""), "threat role is omitted");
    assert!(lines[2].contains("\"role\":\"fragment\""));
    let back: Vec<Op> = read_log(&m, &bytes).into_iter().map(|l| l.op).collect();
    assert_eq!(back, ops);
}

#[test]
fn workspace_files_name_fragments_by_their_prefix_and_round_trip() {
    let m = builtin();
    let p = mixed(&m);
    let files = p.to_workspace_files().expect("files");
    let trees: Vec<&str> = files
        .iter()
        .map(|f| f.path.as_str())
        .filter(|path| path.starts_with("systems/main/trees/"))
        .collect();
    assert_eq!(
        trees,
        [
            "systems/main/trees/at00.json",
            "systems/main/trees/at02.json",
            "systems/main/trees/tt01.json",
        ]
    );
    let back = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("load");
    assert_eq!(back, p);
}

#[test]
fn workspace_rejects_a_fragment_file_holding_a_threat_tree() {
    let m = builtin();
    let p = mixed(&m);
    let files = p.to_workspace_files().expect("files");
    let renamed: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|f| {
            let path = f.path.replace("trees/at00.json", "trees/tt00.json");
            (path, f.bytes.clone())
        })
        .collect();
    let result = Project::from_workspace_files(
        renamed.iter().map(|(p, b)| (p.as_str(), b.as_slice())),
        Arc::clone(&m),
    );
    assert!(
        matches!(
            &result,
            Err(StoreError::WorkspaceTreeName { path }) if path.ends_with("tt00.json")
        ),
        "file prefix must match the stored role: {result:?}"
    );
}
