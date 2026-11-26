//! Tests of shared fragment trees: the immutable tree role, its visual ID
//! prefix, undo, and persistence in snapshots, op logs and workspace files.

mod common;

use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, DamageId, MAX_FRAGMENT_DEPTH, MAX_FRAGMENT_REFERENCES, Methodology,
    ModelError, Node, NodeId, Op, Project, ProjectEvalCache, Provenance, StepRef, StepTarget,
    StoreError, ThreatId, TreeId, TreeRole,
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

#[test]
fn fragment_limits_are_exported_and_public_order_is_dependency_first() {
    assert_eq!((MAX_FRAGMENT_REFERENCES, MAX_FRAGMENT_DEPTH), (256, 8));
    let m = builtin();
    let p = mixed(&m);
    let order: Vec<_> = p
        .fragment_order()
        .expect("no references")
        .into_iter()
        .map(|i| p.attack_tree_id(i).expect("id"))
        .collect();
    assert_eq!(order, [tree_id(1), tree_id(2), tree_id(3)]);
    let fragment = p.attack_tree_idx(&tree_id(2)).expect("fragment");
    let root = p.attack_tree(fragment).expect("tree").root();
    assert!(p.fragment_users(fragment).is_empty());
    assert_eq!(p.step_fragment(fragment, root), None);
    assert_eq!(p.step_fragment_id(fragment, root), None);
}

#[test]
fn fragment_trees_cannot_realize_threats() {
    let m = builtin();
    let mut p = mixed(&m);
    let asset = AssetId::new_v7();
    let damage = DamageId::new_v7();
    let threat = ThreatId::new_v7();
    let safety = m.impact_category("safety").expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: Provenance::human("alice", 10),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Crash".into(),
            impacts: vec![(safety, severe)],
            provenance: Provenance::human("alice", 11),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Injection".into(),
            asset,
            category: m.threat_categories().next().expect("category"),
            catalog_refs: Vec::new(),
            damages: vec![damage],
            provenance: Provenance::human("alice", 12),
        },
    ] {
        p.apply(op).expect("setup");
    }
    let link = |tree| Op::SetThreatAttackTree {
        threat,
        tree: Some(tree_id(tree)),
        provenance: Provenance::human("alice", 13),
    };
    let before = p.clone();
    assert_eq!(p.apply(link(2)), Err(ModelError::FragmentRole(tree_id(2))));
    assert_eq!(p, before);
    p.apply(link(1)).expect("threat tree links");
}

/// A threat tree whose root step references a fragment, built through ops
/// with non-default provenance on every tree.
fn linked(m: &Arc<Methodology>) -> Project {
    project_with(
        m,
        &[
            add_tree(1, TreeRole::Threat),
            add_tree(2, TreeRole::Fragment),
            Op::UseFragment {
                tree: tree_id(1),
                node: node_id(101),
                fragment: tree_id(2),
                provenance: Provenance::human("alice", 7),
            },
        ],
    )
}

#[test]
fn op_log_round_trips_fragment_ops() {
    let m = builtin();
    let ops = [
        add_tree(1, TreeRole::Threat),
        add_tree(2, TreeRole::Fragment),
        Op::UseFragment {
            tree: tree_id(1),
            node: node_id(101),
            fragment: tree_id(2),
            provenance: Provenance::human("alice", 7),
        },
        Op::UnuseFragment {
            tree: tree_id(1),
            node: node_id(101),
            provenance: Provenance::human("alice", 8),
        },
    ];
    let bytes = write_log(&m, &ops);
    let text = std::str::from_utf8(&bytes).expect("utf8");
    assert!(text.contains("\"op\":\"use_fragment\""), "{text}");
    assert!(text.contains("\"op\":\"unuse_fragment\""), "{text}");
    let back: Vec<Op> = read_log(&m, &bytes).into_iter().map(|l| l.op).collect();
    assert_eq!(back, ops);
    assert_eq!(write_log(&m, &back), bytes);
    let mut replayed = Project::new(Arc::clone(&m));
    for (i, op) in back.into_iter().enumerate() {
        replayed.apply(op).expect("replay");
        if i == 2 {
            assert_eq!(replayed, linked(&m));
            assert_eq!(save(&replayed), save(&linked(&m)));
        }
    }
    assert_eq!(
        replayed,
        project_with(
            &m,
            &[
                add_tree(1, TreeRole::Threat),
                add_tree(2, TreeRole::Fragment)
            ]
        )
    );
}

#[test]
fn snapshot_and_workspace_round_trip_the_reference_and_keep_provenance() {
    let m = builtin();
    let mut p = linked(&m);
    let bytes = save(&p);
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    let entries = json["attack_trees"][0]["step_fragments"]
        .as_array()
        .expect("step_fragments");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["node"], Value::from(node_id(101).to_string()));
    assert_eq!(entries[0]["fragment"], Value::from(tree_id(2).to_string()));

    // Every storage path restores the link, and the source tree keeps its
    // own provenance because reference commits never touch it.
    let from_snapshot = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("snapshot");
    assert_eq!(from_snapshot, p);
    assert_eq!(save(&from_snapshot), bytes);
    let files = p.to_workspace_files().expect("files");
    let from_workspace = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("workspace");
    assert_eq!(from_workspace, p);
    let restored_files = from_workspace
        .to_workspace_files()
        .expect("resave workspace");
    assert_eq!(
        restored_files
            .iter()
            .map(|f| (&f.path, &f.bytes))
            .collect::<Vec<_>>(),
        files
            .iter()
            .map(|f| (&f.path, &f.bytes))
            .collect::<Vec<_>>()
    );
    for restored in [&from_snapshot, &from_workspace] {
        let idx = restored.attack_tree_idx(&tree_id(1)).expect("tree");
        let root = restored.attack_tree(idx).expect("entry").root();
        assert_eq!(restored.step_fragment_id(idx, root), Some(tree_id(2)));
        assert_eq!(
            restored.attack_tree_provenance(idx),
            Some(&Provenance::human("alice", 1))
        );
    }

    // Unlink and undo: the exact project and the saved bytes come back.
    let before = p.clone();
    let inverse = p
        .apply(Op::UnuseFragment {
            tree: tree_id(1),
            node: node_id(101),
            provenance: Provenance::human("alice", 9),
        })
        .expect("unlink");
    let Op::UseFragment {
        fragment,
        provenance,
        ..
    } = &inverse
    else {
        panic!("inverse is not UseFragment: {inverse:?}");
    };
    assert_eq!(*fragment, tree_id(2));
    assert_eq!(*provenance, Provenance::human("alice", 9));
    let idx = p.attack_tree_idx(&tree_id(1)).expect("source");
    assert_eq!(
        p.attack_tree_provenance(idx),
        Some(&Provenance::human("alice", 1))
    );
    p.apply(inverse).expect("undo");
    assert_eq!(p, before);
    assert_eq!(save(&p), bytes);
}

#[test]
fn evaluation_fails_closed_on_references_and_recovers_after_unlink() {
    let m = builtin();
    let mut p = linked(&m);
    let level = m.feasibility_level("high").expect("level");
    p.apply(add_tree(3, TreeRole::Threat))
        .expect("ordinary tree");
    for n in [2, 3] {
        p.apply(Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(n), node_id(100 + n)),
            level: Some(level),
            provenance: Provenance::human("alice", 8),
        })
        .expect("rate target and unrelated tree");
    }
    assert_eq!(
        p.evaluate_tree(tree_id(3)).expect("ordinary").root_value(),
        &level
    );
    let idx = p.attack_tree_idx(&tree_id(1)).expect("tree");
    let root = p.attack_tree(idx).expect("entry").root();
    assert_eq!(p.step_fragment_id(idx, root), Some(tree_id(2)));
    // A reference is unrated: evaluation and lint fail closed, never a number.
    assert_eq!(p.step_feasibility(idx, root), None);
    assert_eq!(p.effective_step_feasibility(idx, root), None);
    assert!(matches!(
        p.evaluate_tree(tree_id(1)),
        Err(ModelError::MissingLeafValue(_))
    ));
    assert!(p.lint().iter().any(|f| {
        matches!(
            f.code(),
            aegis_model::LintCode::AttackTreeStepUnrated { tree, node }
            if tree == tree_id(1) && node == node_id(101)
        )
    }));

    // The cached path cannot retain a number across link or unlink either.
    let mut cache = ProjectEvalCache::new(&p);
    assert!(cache.evaluate_tree(&p, tree_id(1)).is_err());
    p.apply_cached(
        Op::UnuseFragment {
            tree: tree_id(1),
            node: node_id(101),
            provenance: Provenance::human("alice", 9),
        },
        &mut cache,
    )
    .expect("unlink");
    p.apply_cached(
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(1), node_id(101)),
            level: Some(level),
            provenance: Provenance::human("alice", 10),
        },
        &mut cache,
    )
    .expect("rate");
    let eval = cache.evaluate_tree(&p, tree_id(1)).expect("evaluates");
    assert_eq!(eval.root_value(), &level);

    p.apply_cached(
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(1), node_id(101)),
            level: None,
            provenance: Provenance::human("alice", 11),
        },
        &mut cache,
    )
    .expect("clear rating");
    p.apply_cached(
        Op::UseFragment {
            tree: tree_id(1),
            node: node_id(101),
            fragment: tree_id(2),
            provenance: Provenance::human("alice", 12),
        },
        &mut cache,
    )
    .expect("relink");
    assert!(cache.evaluate_tree(&p, tree_id(1)).is_err());
    assert!(cache.evaluate_residual_tree(&p, tree_id(1)).is_err());
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(3))
            .expect("unrelated cached tree")
            .root_value(),
        &level
    );
}

#[test]
fn loader_rejects_duplicate_malformed_and_unknown_fragment_links() {
    let m = builtin();
    let p = linked(&m);
    let good: Value = serde_json::from_slice(&save(&p)).expect("json");

    let mut duplicate = good.clone();
    let entry = duplicate["attack_trees"][0]["step_fragments"][0].clone();
    duplicate["attack_trees"][0]["step_fragments"]
        .as_array_mut()
        .expect("array")
        .push(entry);
    assert!(matches!(
        load_value(&m, &duplicate),
        Err(StoreError::DuplicateEntry)
    ));

    let mut unknown_field = good.clone();
    unknown_field["attack_trees"][0]["step_fragments"][0]["note"] = Value::from("x");
    assert!(load_value(&m, &unknown_field).is_err());

    let mut bad_uuid = good.clone();
    bad_uuid["attack_trees"][0]["step_fragments"][0]["fragment"] = Value::from("not-a-uuid");
    assert!(load_value(&m, &bad_uuid).is_err());

    // An unknown target is never dropped: the whole load fails.
    let mut unknown_target = good.clone();
    unknown_target["attack_trees"][0]["step_fragments"][0]["fragment"] =
        Value::from(TreeId::new_v7().to_string());
    assert!(matches!(
        load_value(&m, &unknown_target),
        Err(StoreError::Model(ModelError::UnknownAttackTree(_)))
    ));
}

#[test]
fn loader_applies_role_node_and_rating_guards_to_links() {
    let m = builtin();
    let p = linked(&m);
    let good: Value = serde_json::from_slice(&save(&p)).expect("json");

    // The target must have the fragment role; tree 1 is a threat tree.
    let mut wrong_role = good.clone();
    wrong_role["attack_trees"][0]["step_fragments"][0]["fragment"] =
        Value::from(tree_id(1).to_string());
    assert!(matches!(
        load_value(&m, &wrong_role),
        Err(StoreError::Model(ModelError::FragmentRole(id))) if id == tree_id(1)
    ));

    // The source node must exist.
    let mut unknown_node = good.clone();
    unknown_node["attack_trees"][0]["step_fragments"][0]["node"] =
        Value::from(NodeId::new_v7().to_string());
    assert!(matches!(
        load_value(&m, &unknown_node),
        Err(StoreError::Model(ModelError::UnknownTreeNode { tree, .. })) if tree == tree_id(1)
    ));

    // A rated source step cannot gain a reference on load either.
    let mut rated = project_with(
        &m,
        &[
            add_tree(1, TreeRole::Threat),
            add_tree(2, TreeRole::Fragment),
        ],
    );
    let level = m.feasibility_level("high").expect("level");
    rated
        .apply(Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(1), node_id(101)),
            level: Some(level),
            provenance: Provenance::human("alice", 7),
        })
        .expect("rate");
    let mut rated_json: Value = serde_json::from_slice(&save(&rated)).expect("json");
    rated_json["attack_trees"][0]["step_fragments"] = serde_json::json!([{
        "node": node_id(101).to_string(),
        "fragment": tree_id(2).to_string(),
    }]);
    assert!(matches!(
        load_value(&m, &rated_json),
        Err(StoreError::Model(ModelError::StepIsRated { tree, node }))
        if tree == tree_id(1) && node == node_id(101)
    ));
}

#[test]
fn loader_rejects_cycles_and_over_deep_chains() {
    let m = builtin();
    let base = project_with(
        &m,
        &[
            add_tree(2, TreeRole::Fragment),
            add_tree(3, TreeRole::Fragment),
        ],
    );
    let mut cycle: Value = serde_json::from_slice(&save(&base)).expect("json");
    cycle["attack_trees"][0]["step_fragments"] = serde_json::json!([{
        "node": node_id(102).to_string(),
        "fragment": tree_id(3).to_string(),
    }]);
    cycle["attack_trees"][1]["step_fragments"] = serde_json::json!([{
        "node": node_id(103).to_string(),
        "fragment": tree_id(2).to_string(),
    }]);
    assert!(matches!(
        load_value(&m, &cycle),
        Err(StoreError::Model(ModelError::FragmentCycle(_)))
    ));

    let ops: Vec<Op> = (1..=10).map(|n| add_tree(n, TreeRole::Fragment)).collect();
    let base = project_with(&m, &ops);
    let mut deep: Value = serde_json::from_slice(&save(&base)).expect("json");
    for (i, n) in (1u64..10).enumerate() {
        deep["attack_trees"][i]["step_fragments"] = serde_json::json!([{
            "node": node_id(100 + n).to_string(),
            "fragment": tree_id(n + 1).to_string(),
        }]);
    }
    assert!(matches!(
        load_value(&m, &deep),
        Err(StoreError::Model(ModelError::FragmentDepth {
            max: MAX_FRAGMENT_DEPTH
        }))
    ));
}

#[test]
fn loader_refuses_more_fragment_links_than_the_cap_before_allocating() {
    let m = builtin();
    let p = linked(&m);
    let mut over: Value = serde_json::from_slice(&save(&p)).expect("json");
    let entries: Vec<Value> = (0..=MAX_FRAGMENT_REFERENCES)
        .map(|_| {
            serde_json::json!({
                "node": NodeId::new_v7().to_string(),
                "fragment": tree_id(2).to_string(),
            })
        })
        .collect();
    over["attack_trees"][0]["step_fragments"] = Value::Array(entries);
    assert!(matches!(
        load_value(&m, &over),
        Err(StoreError::LimitExceeded(_))
    ));
}

#[test]
fn forward_targets_and_two_users_of_one_fragment_load() {
    let m = builtin();
    // The source entry precedes its target: replay waits for every tree.
    let base = project_with(
        &m,
        &[
            add_tree(1, TreeRole::Threat),
            add_tree(3, TreeRole::Fragment),
        ],
    );
    let mut forward: Value = serde_json::from_slice(&save(&base)).expect("json");
    forward["attack_trees"][0]["step_fragments"] = serde_json::json!([{
        "node": node_id(101).to_string(),
        "fragment": tree_id(3).to_string(),
    }]);
    // The op replay uses default provenance, which is invisible because
    // reference commits never mutate tree provenance.
    let expected = project_with(
        &m,
        &[
            add_tree(1, TreeRole::Threat),
            add_tree(3, TreeRole::Fragment),
            Op::UseFragment {
                tree: tree_id(1),
                node: node_id(101),
                fragment: tree_id(3),
                provenance: Provenance::human("alice", 9),
            },
        ],
    );
    let back = load_value(&m, &forward).expect("forward target");
    assert_eq!(back, expected);
    assert_eq!(save(&back), save(&expected));

    // Two sources may reference one fragment.
    let base = project_with(
        &m,
        &[
            add_tree(1, TreeRole::Threat),
            add_tree(2, TreeRole::Threat),
            add_tree(3, TreeRole::Fragment),
        ],
    );
    let mut two: Value = serde_json::from_slice(&save(&base)).expect("json");
    two["attack_trees"][0]["step_fragments"] = serde_json::json!([{
        "node": node_id(101).to_string(),
        "fragment": tree_id(3).to_string(),
    }]);
    two["attack_trees"][1]["step_fragments"] = serde_json::json!([{
        "node": node_id(102).to_string(),
        "fragment": tree_id(3).to_string(),
    }]);
    let back = load_value(&m, &two).expect("two users");
    let fragment = back.attack_tree_idx(&tree_id(3)).expect("fragment");
    assert_eq!(
        back.fragment_users(fragment),
        [
            StepRef {
                tree: tree_id(1),
                node: node_id(101)
            },
            StepRef {
                tree: tree_id(2),
                node: node_id(102)
            },
        ]
    );
    let again = Project::load(save(&back).as_slice(), Arc::clone(&m)).expect("re-load");
    assert_eq!(again, back);
}

#[test]
fn trees_without_references_omit_the_field_and_keep_legacy_bytes() {
    let m = builtin();
    let p = mixed(&m);
    let bytes = save(&p);
    assert!(
        !std::str::from_utf8(&bytes)
            .expect("utf8")
            .contains("step_fragments"),
        "empty link lists are omitted"
    );
    let back = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(save(&back), bytes);
}

#[test]
fn unlink_then_remove_then_undo_restores_the_fragment() {
    let m = builtin();
    let mut p = linked(&m);
    let before = p.clone();
    let bytes = save(&p);
    // A referenced fragment cannot be removed.
    assert!(matches!(
        p.apply(Op::RemoveAttackTree { id: tree_id(2) }),
        Err(ModelError::FragmentInUse { .. })
    ));
    let unlink = p
        .apply(Op::UnuseFragment {
            tree: tree_id(1),
            node: node_id(101),
            provenance: Provenance::human("alice", 9),
        })
        .expect("unlink");
    let readd = p
        .apply(Op::RemoveAttackTree { id: tree_id(2) })
        .expect("remove unreferenced fragment");
    p.apply(readd).expect("undo remove");
    p.apply(unlink).expect("undo unlink");
    assert_eq!(p, before);
    assert_eq!(save(&p), bytes);
}

#[test]
fn use_inverse_and_invalid_provenance_preserve_the_entire_project() {
    let m = builtin();
    let mut p = project_with(
        &m,
        &[
            add_tree(1, TreeRole::Threat),
            add_tree(2, TreeRole::Fragment),
        ],
    );
    let before = p.clone();
    let bytes = save(&p);
    let mut use_op = Op::UseFragment {
        tree: tree_id(1),
        node: node_id(101),
        fragment: tree_id(2),
        provenance: Provenance::human(" ", 4),
    };
    assert_eq!(p.apply(use_op.clone()), Err(ModelError::EmptyActor));
    assert_eq!(p, before);
    use_op = use_op.with_provenance(Provenance::human("bob", 4));
    let inverse = p.apply(use_op).expect("use");
    assert_eq!(
        inverse,
        Op::UnuseFragment {
            tree: tree_id(1),
            node: node_id(101),
            provenance: Provenance::human("bob", 4),
        }
    );
    let linked_state = p.clone();
    let invalid_unuse = inverse.clone().with_provenance(Provenance::human(" ", 5));
    assert_eq!(p.apply(invalid_unuse), Err(ModelError::EmptyActor));
    assert_eq!(p, linked_state);
    p.apply(inverse).expect("undo use");
    assert_eq!(p, before);
    assert_eq!(save(&p), bytes);
}

#[test]
fn fragment_op_wire_fields_and_lowercase_names_are_strict() {
    let m = builtin();
    for op in [
        Op::UseFragment {
            tree: tree_id(1),
            node: node_id(101),
            fragment: tree_id(2),
            provenance: Provenance::default(),
        },
        Op::UnuseFragment {
            tree: tree_id(1),
            node: node_id(101),
            provenance: Provenance::default(),
        },
    ] {
        let bytes = write_log(&m, &[op]);
        let lines: Vec<_> = std::str::from_utf8(&bytes).expect("utf8").lines().collect();
        let good: Value = serde_json::from_str(lines[1]).expect("record");
        for field in ["tree", "node", "fragment", "unknown", "op"] {
            if field == "fragment" && good.get(field).is_none() {
                continue;
            }
            let mut bad = good.clone();
            bad[field] = Value::from(if field == "op" {
                "UseFragment"
            } else {
                "invalid-uuid"
            });
            let log = format!("{}\n{}\n", lines[0], bad);
            assert!(
                aegis_model::OpLogReader::open(log.as_bytes(), Arc::clone(&m))
                    .expect("header")
                    .next()
                    .expect("record")
                    .is_err(),
                "accepted malformed {field}: {bad}"
            );
        }
    }
}

#[test]
fn maximum_parallel_references_round_trip_sorted_in_every_storage_path() {
    let m = builtin();
    let mut p = project_with(&m, &[add_tree(2, TreeRole::Fragment)]);
    let root = node_id(1000);
    p.apply(Op::AddAttackTree {
        id: tree_id(1),
        role: TreeRole::Threat,
        tree: AttackTree::new(root, Node::gate(aegis_model::GateKind::Or, "Root")).expect("tree"),
        step_feasibility: vec![],
        provenance: Provenance::human("alice", 20),
    })
    .expect("source");
    // Opposite arena order and ID order, to exercise canonical serialization.
    for n in (1..=MAX_FRAGMENT_REFERENCES).rev() {
        let node = node_id(1000 + n as u64);
        p.apply(Op::AddTreeNode {
            tree: tree_id(1),
            parent: root,
            id: node,
            node: Node::step("Reference"),
            index: (MAX_FRAGMENT_REFERENCES - n + 1) as u32,
            position: (MAX_FRAGMENT_REFERENCES - n) as u32,
            provenance: Provenance::human("alice", 20),
        })
        .expect("node");
        p.apply(Op::UseFragment {
            tree: tree_id(1),
            node,
            fragment: tree_id(2),
            provenance: Provenance::human("bob", 21),
        })
        .expect("link");
    }
    let bytes = save(&p);
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    let source = json["attack_trees"]
        .as_array()
        .expect("trees")
        .iter()
        .find(|tree| tree["id"] == tree_id(1).to_string())
        .expect("source");
    let entries = source["step_fragments"].as_array().expect("links");
    assert_eq!(entries.len(), MAX_FRAGMENT_REFERENCES);
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry["node"], node_id(1001 + i as u64).to_string());
    }
    let back = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("snapshot");
    assert_eq!(back, p);
    assert_eq!(save(&back), bytes);
    let files = p.to_workspace_files().expect("workspace");
    let back = Project::from_workspace_files(
        files
            .iter()
            .rev()
            .map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("workspace reverse file order");
    assert_eq!(back, p);
    assert_eq!(save(&back), bytes);
    let again = back.to_workspace_files().expect("resave workspace");
    assert_eq!(
        again
            .iter()
            .map(|f| (&f.path, &f.bytes))
            .collect::<Vec<_>>(),
        files
            .iter()
            .map(|f| (&f.path, &f.bytes))
            .collect::<Vec<_>>()
    );
}
