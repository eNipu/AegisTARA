//! Tests of shared fragment trees: the immutable tree role, its visual ID
//! prefix, undo, and persistence in snapshots, op logs and workspace files.

mod common;

use std::sync::Arc;

use aegis_model::ControlStatus;
use aegis_model::{
    AssetId, AttackTree, DamageId, GateKind, MAX_FRAGMENT_DEPTH, MAX_FRAGMENT_REFERENCES,
    Methodology, ModelError, Node, NodeId, Op, Project, ProjectEvalCache, Provenance, StepRef,
    StepTarget, StoreError, ThreatId, TreeId, TreeRole,
};
use common::{
    builtin, condition_id, control_id, node_id, read_log, save, shared_step_id, tree_id, write_log,
};
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
fn references_fold_the_fragment_root_and_recover_after_unlink() {
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
    // The reference consumes the fragment root in both modes, while the
    // per-leaf accessors stay own-rating only and lint does not flag it.
    assert_eq!(
        p.evaluate_tree(tree_id(1)).expect("inherent").root_value(),
        &level
    );
    let residual = p.evaluate_residual_tree(tree_id(1)).expect("residual");
    assert_eq!(residual.root_value(), &level);
    assert_eq!(p.step_feasibility(idx, root), None);
    assert_eq!(p.effective_step_feasibility(idx, root), None);
    assert_eq!(p.residual_step_feasibility(idx, root), None);
    assert!(p.deciding_step(idx, &residual).is_none(), "tree-local");
    assert!(!p.lint().iter().any(|f| {
        matches!(
            f.code(),
            aegis_model::LintCode::AttackTreeStepUnrated { tree, .. } if tree == tree_id(1)
        )
    }));

    let mut cache = ProjectEvalCache::new(&p);
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(1))
            .expect("cached")
            .root_value(),
        &level
    );
    let unlink = p
        .apply_cached(
            Op::UnuseFragment {
                tree: tree_id(1),
                node: node_id(101),
                provenance: Provenance::human("alice", 9),
            },
            &mut cache,
        )
        .expect("unlink");
    assert!(matches!(
        cache.evaluate_tree(&p, tree_id(1)),
        Err(ModelError::MissingLeafValue(_))
    ));
    p.apply_cached(unlink, &mut cache).expect("undo unlink");
    assert_eq!(cache, ProjectEvalCache::new(&p));

    // An unrated fragment leaves the reference unrated: never a default.
    let unrate = p
        .apply_cached(
            Op::SetStepFeasibility {
                target: StepTarget::Step(tree_id(2), node_id(102)),
                level: None,
                provenance: Provenance::human("alice", 11),
            },
            &mut cache,
        )
        .expect("clear fragment rating");
    for result in [
        p.evaluate_tree(tree_id(1)),
        p.evaluate_residual_tree(tree_id(1)),
        cache.evaluate_tree(&p, tree_id(1)).cloned(),
        cache.evaluate_residual_tree(&p, tree_id(1)).cloned(),
    ] {
        assert!(
            matches!(result, Err(ModelError::MissingLeafValue(_))),
            "{result:?}"
        );
    }
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(3))
            .expect("unrelated")
            .root_value(),
        &level
    );
    p.apply_cached(unrate, &mut cache).expect("undo clear");
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(1))
            .expect("again")
            .root_value(),
        &level
    );
    assert_eq!(cache, ProjectEvalCache::new(&p));
}

/// Adds a gate tree `n` with `role`: root `1000 * n`, then the given
/// children (node IDs `1000 * n + i`) under the root in order.
fn gate_tree(
    n: u64,
    role: TreeRole,
    gate: GateKind,
    steps: &[(u64, Option<&str>)],
    m: &Methodology,
) -> Op {
    let root = node_id(1000 * n);
    let mut tree = AttackTree::new(root, Node::gate(gate, "Gate")).expect("tree");
    let mut rated = Vec::new();
    for &(i, level) in steps {
        tree.add_child(tree.root(), node_id(1000 * n + i), Node::step("Step"))
            .expect("child");
        if let Some(key) = level {
            rated.push((
                node_id(1000 * n + i),
                m.feasibility_level(key).expect("level"),
            ));
        }
    }
    Op::AddAttackTree {
        role,
        id: tree_id(n),
        tree,
        step_feasibility: rated,
        provenance: Provenance::human("alice", n),
    }
}

fn use_fragment(n: u64, i: u64, target: u64) -> Op {
    Op::UseFragment {
        tree: tree_id(n),
        node: node_id(1000 * n + i),
        fragment: tree_id(target),
        provenance: Provenance::human("alice", 30),
    }
}

fn path_ids(path: &[StepRef]) -> Vec<(TreeId, NodeId)> {
    path.iter().map(|s| (s.tree, s.node)).collect()
}

#[test]
fn why_path_crosses_nested_references_in_the_requested_mode() {
    let m = builtin();
    let mut p = project_with(
        &m,
        &[
            gate_tree(
                1,
                TreeRole::Threat,
                GateKind::Or,
                &[(1, Some("low")), (2, None)],
                &m,
            ),
            gate_tree(
                2,
                TreeRole::Fragment,
                GateKind::And,
                &[(1, Some("high")), (2, None)],
                &m,
            ),
            gate_tree(
                3,
                TreeRole::Fragment,
                GateKind::Or,
                &[(1, Some("medium"))],
                &m,
            ),
            use_fragment(1, 2, 2),
            use_fragment(2, 2, 3),
        ],
    );
    let medium = m.feasibility_level("medium").expect("medium");
    let eval = p.evaluate_tree(tree_id(1)).expect("evaluates");
    assert_eq!(eval.root_value(), &medium);
    let tree = p
        .attack_tree(p.attack_tree_idx(&tree_id(1)).expect("idx"))
        .expect("tree");
    // The generic path stays inside the host tree and ends at the reference.
    let local: Vec<_> = eval
        .why_path(tree.root())
        .map(|i| tree.id_of(i).expect("id"))
        .collect();
    assert_eq!(local, [node_id(1000), node_id(1002)]);
    let crossing = [
        (tree_id(1), node_id(1000)),
        (tree_id(1), node_id(1002)),
        (tree_id(2), node_id(2000)),
        (tree_id(2), node_id(2002)),
        (tree_id(3), node_id(3000)),
        (tree_id(3), node_id(3001)),
    ];
    for residual in [false, true] {
        let path = p
            .fragment_why_path(tree_id(1), node_id(1000), residual)
            .expect("path");
        assert_eq!(path_ids(&path), crossing);
    }
    // Starting below the root walks from that node only.
    let from_ref = p
        .fragment_why_path(tree_id(1), node_id(1002), false)
        .expect("path");
    assert_eq!(path_ids(&from_ref), crossing[1..]);

    // A planned control on the host reference lowers only the residual
    // value, so only the residual path stops crossing.
    let ctrl = aegis_model::ControlId::new_v7();
    for op in [
        Op::AddSecurityControl {
            id: ctrl,
            name: "Guard".into(),
            provenance: Provenance::human("alice", 40),
        },
        Op::SetStepMitigation {
            tree: tree_id(1),
            node: node_id(1002),
            control: ctrl,
            feasibility: m.feasibility_level("very-low"),
            provenance: Provenance::human("alice", 41),
        },
    ] {
        p.apply(op).expect("mitigate");
    }
    let inherent = p
        .fragment_why_path(tree_id(1), node_id(1000), false)
        .expect("path");
    assert_eq!(path_ids(&inherent), crossing);
    let residual = p
        .fragment_why_path(tree_id(1), node_id(1000), true)
        .expect("path");
    assert_eq!(
        path_ids(&residual),
        [(tree_id(1), node_id(1000)), (tree_id(1), node_id(1001))]
    );

    // No reference on the path: identical to the generic path.
    let plain = p
        .fragment_why_path(tree_id(3), node_id(3000), false)
        .expect("path");
    assert_eq!(path_ids(&plain), crossing[4..]);
    assert!(matches!(
        p.fragment_why_path(tree_id(1), node_id(9), false),
        Err(ModelError::UnknownTreeNode { .. })
    ));
    assert!(matches!(
        p.fragment_why_path(tree_id(9), node_id(1000), false),
        Err(ModelError::UnknownAttackTree(_))
    ));
}

#[test]
fn excluded_fragments_and_local_conditions_exclude_the_reference() {
    let m = builtin();
    let cond = aegis_model::ConditionId::new_v7();
    let mut p = project_with(
        &m,
        &[
            gate_tree(
                1,
                TreeRole::Threat,
                GateKind::Or,
                &[(1, Some("low")), (2, None)],
                &m,
            ),
            gate_tree(
                2,
                TreeRole::Fragment,
                GateKind::Or,
                &[(1, Some("high"))],
                &m,
            ),
            gate_tree(
                4,
                TreeRole::Threat,
                GateKind::And,
                &[(1, Some("low")), (2, None)],
                &m,
            ),
            use_fragment(1, 2, 2),
            use_fragment(4, 2, 2),
            Op::AddCondition {
                id: cond,
                name: "Debug port".into(),
                description: "".into(),
                present: true,
                provenance: Provenance::human("alice", 50),
            },
            Op::NeedCondition {
                element: aegis_model::ElementRef::Tree(tree_id(2)),
                condition: cond,
                provenance: Provenance::human("alice", 51),
            },
        ],
    );
    let (low, high) = (
        m.feasibility_level("low").expect("low"),
        m.feasibility_level("high").expect("high"),
    );
    let mut cache = ProjectEvalCache::new(&p);
    assert_eq!(p.evaluate_tree(tree_id(1)).expect("on").root_value(), &high);
    let off = p
        .apply_cached(
            Op::SetConditionPresent {
                id: cond,
                present: false,
                provenance: Provenance::human("alice", 52),
            },
            &mut cache,
        )
        .expect("condition off");
    // The fragment is excluded, so OR falls back to the own step and AND is
    // excluded entirely, not unrated.
    let eval = p.evaluate_tree(tree_id(1)).expect("excluded reference");
    assert_eq!(eval.root_value(), &low);
    let tree = p
        .attack_tree(p.attack_tree_idx(&tree_id(1)).expect("idx"))
        .expect("tree");
    assert_eq!(eval.get(tree.idx_of(&node_id(1002)).expect("ref")), None);
    assert_eq!(
        p.evaluate_tree(tree_id(4)),
        Err(ModelError::TreeNotApplicable(tree_id(4)))
    );
    assert_eq!(cache, ProjectEvalCache::new(&p));
    p.apply_cached(off, &mut cache).expect("undo");
    assert_eq!(cache, ProjectEvalCache::new(&p));
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(1))
            .expect("on")
            .root_value(),
        &high
    );

    // A condition on the host step excludes the reference on its own.
    let local = aegis_model::ConditionId::new_v7();
    for op in [
        Op::AddCondition {
            id: local,
            name: "Local".into(),
            description: "".into(),
            present: false,
            provenance: Provenance::human("alice", 53),
        },
        Op::NeedStepCondition {
            tree: tree_id(1),
            node: node_id(1002),
            condition: local,
            provenance: Provenance::human("alice", 54),
        },
    ] {
        p.apply_cached(op, &mut cache).expect("local condition");
    }
    assert_eq!(
        p.evaluate_tree(tree_id(1)).expect("local").root_value(),
        &low
    );
    assert_eq!(cache, ProjectEvalCache::new(&p));
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

// ---------------------------------------------------------------------------
// Independent oracle: a recursive fold over a generated DAG description that
// never calls the production order, evaluation or cache.
// ---------------------------------------------------------------------------

/// Trees 0, 1 and 5 are threat trees, 2..=4 fragments; nobody references 5.
const ORACLE_TREES: usize = 6;
const LEVELS: [&str; 4] = ["very-low", "low", "medium", "high"];

#[derive(Clone, Debug, PartialEq)]
enum Leaf {
    Own(Option<usize>),
    Shared(usize),
    Ref(usize),
}

#[derive(Clone, Debug)]
struct OStep {
    id: NodeId,
    leaf: Leaf,
    cond: Option<usize>,
    mit: Option<(usize, usize)>,
}

#[derive(Clone, Debug)]
enum OKid {
    Step(OStep),
    Gate(NodeId, GateKind, Vec<OStep>),
}

#[derive(Clone, Debug)]
struct OTree {
    gate: GateKind,
    kids: Vec<OKid>,
    cond: Option<usize>,
}

#[derive(Clone, Debug)]
struct Spec {
    trees: Vec<OTree>,
    shared: Vec<Option<usize>>,
    present: Vec<bool>,
    status: Vec<ControlStatus>,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) % n as u64) as usize
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.next(100) < percent
    }
}

const GATES: [GateKind; 3] = [GateKind::Or, GateKind::And, GateKind::SeqAnd];

fn oracle_tid(t: usize) -> TreeId {
    tree_id(500 + t as u64)
}

impl OTree {
    fn steps(&self) -> impl Iterator<Item = &OStep> {
        self.kids.iter().flat_map(|k| match k {
            OKid::Step(s) => std::slice::from_ref(s).iter(),
            OKid::Gate(_, _, s) => s.iter(),
        })
    }
    fn steps_mut(&mut self) -> impl Iterator<Item = &mut OStep> {
        self.kids.iter_mut().flat_map(|k| match k {
            OKid::Step(s) => std::slice::from_mut(s).iter_mut(),
            OKid::Gate(_, _, s) => s.iter_mut(),
        })
    }
}

fn gen_spec(rng: &mut Rng) -> Spec {
    let mut next_node = 0u64;
    let mut fresh = |t: usize| {
        next_node += 1;
        node_id(50_000 + 1000 * t as u64 + next_node)
    };
    let mut trees = Vec::new();
    for t in 0..ORACLE_TREES {
        let gen_step = |rng: &mut Rng, id: NodeId| {
            let leaf = match rng.next(10) {
                0 => Leaf::Own(None),
                1 | 2 => Leaf::Shared(rng.next(2)),
                3 | 4 if (1..ORACLE_TREES - 1).contains(&t) && t < 4 => {
                    Leaf::Ref(rng.next(4 - t.max(1)) + t.max(1) + 1)
                }
                _ => Leaf::Own(Some(rng.next(LEVELS.len()))),
            };
            let leaf = match leaf {
                Leaf::Ref(f) if f < 2 || f <= t => Leaf::Own(Some(1)),
                other => other,
            };
            OStep {
                id,
                leaf,
                cond: rng.chance(20).then(|| rng.next(2)),
                mit: rng
                    .chance(35)
                    .then(|| (rng.next(3), rng.next(LEVELS.len()))),
            }
        };
        let mut kids = Vec::new();
        for _ in 0..1 + rng.next(3) {
            if rng.chance(30) {
                let gid = fresh(t);
                let steps = (0..1 + rng.next(2))
                    .map(|_| {
                        let id = fresh(t);
                        gen_step(rng, id)
                    })
                    .collect();
                kids.push(OKid::Gate(gid, GATES[rng.next(3)], steps));
            } else {
                let id = fresh(t);
                kids.push(OKid::Step(gen_step(rng, id)));
            }
        }
        // Forced shape: tree 0 references 2 and 3, which both reference 4
        // (a diamond, and two references to one target); tree 1 also uses 2.
        let forced: &[usize] = match t {
            0 => &[2, 3],
            1 => &[2],
            2..=3 => &[4],
            _ => &[],
        };
        for &f in forced {
            let id = fresh(t);
            let mut s = gen_step(rng, id);
            s.leaf = Leaf::Ref(f);
            kids.push(OKid::Step(s));
        }
        if t == 5 {
            let id = fresh(t);
            kids.push(OKid::Step(OStep {
                id,
                leaf: Leaf::Own(None),
                cond: None,
                mit: None,
            }));
        }
        trees.push(OTree {
            gate: GATES[rng.next(3)],
            kids,
            cond: ((2..5).contains(&t) && rng.chance(25)).then(|| rng.next(2)),
        });
    }
    Spec {
        trees,
        shared: (0..2)
            .map(|_| (!rng.chance(15)).then(|| rng.next(LEVELS.len())))
            .collect(),
        present: (0..2).map(|_| !rng.chance(30)).collect(),
        status: (0..3).map(|_| ControlStatus::ALL[rng.next(3)]).collect(),
    }
}

fn oracle_root(t: usize) -> NodeId {
    node_id(40_000 + t as u64)
}

fn build(m: &Arc<Methodology>, spec: &Spec) -> Project {
    let mut p = Project::new(Arc::clone(m));
    let prov = || Provenance::human("oracle", 1);
    let level = |i: usize| m.feasibility_level(LEVELS[i]).expect("level");
    let mut ops = Vec::new();
    for (c, &present) in spec.present.iter().enumerate() {
        ops.push(Op::AddCondition {
            id: condition_id(c as u64 + 1),
            name: "Condition".into(),
            description: "".into(),
            present,
            provenance: prov(),
        });
    }
    for (k, &status) in spec.status.iter().enumerate() {
        ops.push(Op::AddSecurityControl {
            id: control_id(k as u64 + 1),
            name: "Control".into(),
            provenance: prov(),
        });
        ops.push(Op::SetControlStatus {
            control: control_id(k as u64 + 1),
            status,
            provenance: prov(),
        });
    }
    for (s, rating) in spec.shared.iter().enumerate() {
        ops.push(Op::AddSharedStep {
            id: shared_step_id(s as u64 + 1),
            name: "Shared".into(),
            provenance: prov(),
        });
        ops.push(Op::SetStepFeasibility {
            target: StepTarget::Shared(shared_step_id(s as u64 + 1)),
            level: rating.map(level),
            provenance: prov(),
        });
    }
    let mut links = Vec::new();
    for (t, tree) in spec.trees.iter().enumerate() {
        let mut at = AttackTree::new(oracle_root(t), Node::gate(tree.gate, "Root")).expect("tree");
        let mut rated = Vec::new();
        let mut add = |at: &mut AttackTree, parent, s: &OStep| {
            at.add_child(parent, s.id, Node::step("Step"))
                .expect("step");
            let tid = oracle_tid(t);
            match s.leaf {
                Leaf::Own(Some(l)) => rated.push((s.id, level(l))),
                Leaf::Own(None) => {}
                Leaf::Shared(sh) => links.push(Op::UseSharedStep {
                    tree: tid,
                    node: s.id,
                    step: shared_step_id(sh as u64 + 1),
                    provenance: prov(),
                }),
                Leaf::Ref(f) => links.push(Op::UseFragment {
                    tree: tid,
                    node: s.id,
                    fragment: oracle_tid(f),
                    provenance: prov(),
                }),
            }
            if let Some(c) = s.cond {
                links.push(Op::NeedStepCondition {
                    tree: tid,
                    node: s.id,
                    condition: condition_id(c as u64 + 1),
                    provenance: prov(),
                });
            }
            if let Some((k, l)) = s.mit {
                links.push(Op::SetStepMitigation {
                    tree: tid,
                    node: s.id,
                    control: control_id(k as u64 + 1),
                    feasibility: Some(level(l)),
                    provenance: prov(),
                });
            }
        };
        for kid in &tree.kids {
            let root = at.root();
            match kid {
                OKid::Step(s) => add(&mut at, root, s),
                OKid::Gate(g, kind, steps) => {
                    let gi = at
                        .add_child(root, *g, Node::gate(*kind, "Gate"))
                        .expect("gate");
                    for s in steps {
                        add(&mut at, gi, s);
                    }
                }
            }
        }
        ops.push(Op::AddAttackTree {
            role: if (2..5).contains(&t) {
                TreeRole::Fragment
            } else {
                TreeRole::Threat
            },
            id: oracle_tid(t),
            tree: at,
            step_feasibility: rated,
            provenance: prov(),
        });
        if let Some(c) = tree.cond {
            links.push(Op::NeedCondition {
                element: aegis_model::ElementRef::Tree(oracle_tid(t)),
                condition: condition_id(c as u64 + 1),
                provenance: prov(),
            });
        }
    }
    for op in ops.into_iter().chain(links) {
        p.apply(op.clone())
            .unwrap_or_else(|e| panic!("{op:?}: {e}"));
    }
    p
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum OVal {
    Excluded,
    Value(usize),
    Unrated,
}

#[derive(Clone, Debug, PartialEq)]
enum OErr {
    NotApplicable,
    Unrated,
}

type OResult = Result<Vec<(NodeId, Option<usize>)>, OErr>;

fn counts(status: ControlStatus, residual: bool) -> bool {
    match status {
        ControlStatus::Implemented => true,
        ControlStatus::Planned => residual,
        ControlStatus::Off => false,
    }
}

fn fold(values: &[OVal], gate: GateKind) -> OVal {
    if values.contains(&OVal::Unrated) {
        return OVal::Unrated;
    }
    let kept: Vec<usize> = values
        .iter()
        .filter_map(|v| match v {
            OVal::Value(l) => Some(*l),
            _ => None,
        })
        .collect();
    let any_excluded = values.contains(&OVal::Excluded);
    match gate {
        GateKind::Or if kept.is_empty() => OVal::Excluded,
        GateKind::Or => OVal::Value(*kept.iter().max().expect("value")),
        _ if any_excluded => OVal::Excluded,
        _ => OVal::Value(*kept.iter().min().expect("value")),
    }
}

/// Recursive fold of tree `t`: OR = max, AND/SEQ = min over non-excluded
/// children, conditions exclude, mode-counted mitigations cap steps.
fn oracle(spec: &Spec, t: usize, residual: bool) -> OResult {
    let tree = &spec.trees[t];
    if tree.cond.is_some_and(|c| !spec.present[c]) {
        return Err(OErr::NotApplicable);
    }
    // Applicability alone (step conditions, not evaluation) excludes a tree
    // whose root no applicable path reaches.
    let off = |s: &OStep| {
        if s.cond.is_some_and(|c| !spec.present[c]) {
            OVal::Excluded
        } else {
            OVal::Value(0)
        }
    };
    let presence: Vec<OVal> = tree
        .kids
        .iter()
        .map(|k| match k {
            OKid::Step(s) => off(s),
            OKid::Gate(_, kind, steps) => fold(&steps.iter().map(off).collect::<Vec<_>>(), *kind),
        })
        .collect();
    if fold(&presence, tree.gate) == OVal::Excluded {
        return Err(OErr::NotApplicable);
    }
    let step = |s: &OStep| -> OVal {
        if s.cond.is_some_and(|c| !spec.present[c]) {
            return OVal::Excluded;
        }
        let base = match &s.leaf {
            Leaf::Own(l) => *l,
            Leaf::Shared(sh) => spec.shared[*sh],
            Leaf::Ref(f) => match oracle(spec, *f, residual) {
                Ok(values) => values[0].1,
                Err(OErr::NotApplicable) => return OVal::Excluded,
                Err(OErr::Unrated) => None,
            },
        };
        let Some(mut v) = base else {
            return OVal::Unrated;
        };
        if let Some((k, l)) = s.mit
            && counts(spec.status[k], residual)
        {
            v = v.min(l);
        }
        OVal::Value(v)
    };
    let mut out = Vec::new();
    let mut root_kids = Vec::new();
    let mut unrated = false;
    for kid in &tree.kids {
        match kid {
            OKid::Step(s) => {
                let v = step(s);
                unrated |= v == OVal::Unrated;
                out.push((s.id, v));
                root_kids.push(v);
            }
            OKid::Gate(g, kind, steps) => {
                let vals: Vec<OVal> = steps.iter().map(step).collect();
                unrated |= vals.contains(&OVal::Unrated);
                let gv = fold(&vals, *kind);
                out.push((*g, gv));
                out.extend(steps.iter().map(|s| s.id).zip(vals));
                root_kids.push(gv);
            }
        }
    }
    if unrated {
        return Err(OErr::Unrated);
    }
    let OVal::Value(root) = fold(&root_kids, tree.gate) else {
        return Err(OErr::NotApplicable);
    };
    let mut values = vec![(oracle_root(t), Some(root))];
    values.extend(out.into_iter().map(|(id, v)| match v {
        OVal::Value(l) => (id, Some(l)),
        _ => (id, None),
    }));
    Ok(values)
}

fn assert_matches_oracle(
    m: &Methodology,
    p: &Project,
    spec: &Spec,
    got: &Result<aegis_model::Evaluation<aegis_model::FeasibilityLevel>, ModelError>,
    t: usize,
    residual: bool,
) {
    let want = oracle(spec, t, residual);
    let tree = p
        .attack_tree(p.attack_tree_idx(&oracle_tid(t)).expect("idx"))
        .expect("tree");
    match (&want, got) {
        (Ok(values), Ok(eval)) => {
            for (id, level) in values {
                let idx = tree.idx_of(id).expect("node");
                let expected = level.map(|l| m.feasibility_level(LEVELS[l]).expect("level"));
                assert_eq!(
                    eval.get(idx).copied(),
                    expected,
                    "tree {t} node {id} {residual}"
                );
            }
        }
        (Err(OErr::NotApplicable), Err(ModelError::TreeNotApplicable(_)))
        | (Err(OErr::Unrated), Err(ModelError::MissingLeafValue(_))) => {}
        _ => panic!("tree {t} residual {residual}: oracle {want:?}, engine {got:?}\n{spec:#?}"),
    }
}

fn check_all(m: &Methodology, p: &Project, cache: &ProjectEvalCache, spec: &Spec) {
    for t in 0..ORACLE_TREES {
        for residual in [false, true] {
            let id = oracle_tid(t);
            let (live, cached) = if residual {
                (
                    p.evaluate_residual_tree(id),
                    cache.evaluate_residual_tree(p, id).cloned(),
                )
            } else {
                (p.evaluate_tree(id), cache.evaluate_tree(p, id).cloned())
            };
            assert_eq!(live, cached, "cache tree {t} residual {residual}");
            assert_matches_oracle(m, p, spec, &live, t, residual);
        }
    }
    assert_eq!(cache, &ProjectEvalCache::new(p), "incremental equals full");
}

/// A random op valid in `spec`, with the spec it produces.
fn gen_op(m: &Methodology, rng: &mut Rng, spec: &Spec) -> (Op, Spec) {
    let prov = Provenance::human("oracle", 2);
    let mut next = spec.clone();
    let level = |i: usize| m.feasibility_level(LEVELS[i]).expect("level");
    loop {
        let t = rng.next(ORACLE_TREES);
        let count = spec.trees[t].steps().count();
        let pick = rng.next(count);
        let step = next.trees[t].steps_mut().nth(pick).expect("step");
        let (tid, node) = (oracle_tid(t), step.id);
        match rng.next(7) {
            0 => {
                let c = rng.next(2);
                next.present[c] = !next.present[c];
                let op = Op::SetConditionPresent {
                    id: condition_id(c as u64 + 1),
                    present: next.present[c],
                    provenance: prov,
                };
                return (op, next);
            }
            1 => {
                let k = rng.next(3);
                next.status[k] = ControlStatus::ALL[rng.next(3)];
                let op = Op::SetControlStatus {
                    control: control_id(k as u64 + 1),
                    status: next.status[k],
                    provenance: prov,
                };
                return (op, next);
            }
            2 => {
                let s = rng.next(2);
                next.shared[s] = (!rng.chance(25)).then(|| rng.next(LEVELS.len()));
                let op = Op::SetStepFeasibility {
                    target: StepTarget::Shared(shared_step_id(s as u64 + 1)),
                    level: next.shared[s].map(level),
                    provenance: prov,
                };
                return (op, next);
            }
            3 if matches!(step.leaf, Leaf::Own(_)) => {
                let l = (!rng.chance(25)).then(|| rng.next(LEVELS.len()));
                step.leaf = Leaf::Own(l);
                let op = Op::SetStepFeasibility {
                    target: StepTarget::Step(tid, node),
                    level: l.map(level),
                    provenance: prov,
                };
                return (op, next);
            }
            4 if matches!(step.leaf, Leaf::Ref(_)) => {
                step.leaf = Leaf::Own(None);
                return (
                    Op::UnuseFragment {
                        tree: tid,
                        node,
                        provenance: prov,
                    },
                    next,
                );
            }
            5 if step.leaf == Leaf::Own(None) && t != 4 => {
                let low = if (2..5).contains(&t) { t + 1 } else { 2 };
                let f = low + rng.next(5 - low);
                step.leaf = Leaf::Ref(f);
                let op = Op::UseFragment {
                    tree: tid,
                    node,
                    fragment: oracle_tid(f),
                    provenance: prov,
                };
                return (op, next);
            }
            6 => {
                let (k, l) = (rng.next(3), rng.next(LEVELS.len()));
                let cleared = rng.chance(30);
                let k = step.mit.map_or(k, |(old, _)| old);
                step.mit = (!cleared).then_some((k, l));
                let op = Op::SetStepMitigation {
                    tree: tid,
                    node,
                    control: control_id(k as u64 + 1),
                    feasibility: (!cleared).then(|| level(l)),
                    provenance: prov,
                };
                return (op, next);
            }
            _ => {}
        }
    }
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    #[test]
    fn fragment_evaluation_matches_an_independent_oracle(seed in proptest::prelude::any::<u64>()) {
        let m = builtin();
        let mut rng = Rng(seed);
        let mut spec = gen_spec(&mut rng);
        let mut p = build(&m, &spec);
        let mut cache = ProjectEvalCache::new(&p);
        check_all(&m, &p, &cache, &spec);
        for _ in 0..8 {
            let (op, next) = gen_op(&m, &mut rng, &spec);
            let inverse = p
                .apply_cached(op.clone(), &mut cache)
                .unwrap_or_else(|e| panic!("{op:?}: {e}"));
            check_all(&m, &p, &cache, &next);
            p.apply_cached(inverse, &mut cache).expect("inverse");
            check_all(&m, &p, &cache, &spec);
            p.apply_cached(op, &mut cache).expect("redo");
            check_all(&m, &p, &cache, &next);
            spec = next;
        }
        // Saved and reloaded projects give the live values.
        let snapshot = Project::load(save(&p).as_slice(), Arc::clone(&m)).expect("snapshot");
        let files = p.to_workspace_files().expect("files");
        let workspace = Project::from_workspace_files(
            files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
            Arc::clone(&m),
        )
        .expect("workspace");
        for reloaded in [&snapshot, &workspace] {
            for t in 0..ORACLE_TREES {
                let id = oracle_tid(t);
                assert_eq!(reloaded.evaluate_tree(id), p.evaluate_tree(id));
                assert_eq!(reloaded.evaluate_residual_tree(id), p.evaluate_residual_tree(id));
            }
            assert_eq!(&ProjectEvalCache::new(reloaded), &ProjectEvalCache::new(&p));
        }
    }
}
