//! Tests of shared fragment trees: the immutable tree role, its visual ID
//! prefix, undo, and persistence in snapshots, op logs and workspace files.

mod common;
#[path = "common/fragment_oracle.rs"]
mod fragment_oracle;

use std::sync::Arc;

use aegis_model::ControlStatus;
use aegis_model::{
    AssetId, AttackTree, DamageId, GateKind, Lifted, MAX_FRAGMENT_DEPTH, MAX_FRAGMENT_REFERENCES,
    Methodology, ModelError, Node, NodeId, Op, Project, ProjectEvalCache, Provenance, StepRef,
    StepTarget, StoreError, ThreatId, TreeId, TreeRole,
};
use common::{
    asset_id, builtin, condition_id, control_id, damage_id, feas, node_id, read_log, save,
    shared_step_id, threat_id, tree_id, write_log,
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

// ---------------------------------------------------------------------------
// Incremental invalidation of fragment users (142e).
// ---------------------------------------------------------------------------

/// Steps referencing `fragment`, found by walking every node of every tree
/// through the public per-step accessor, independent of any index.
fn scan_users(p: &Project, fragment: TreeId) -> Vec<(TreeId, NodeId)> {
    let mut out = Vec::new();
    for (idx, id, tree) in p.attack_trees() {
        for (node_idx, node, _) in tree.iter() {
            if p.step_fragment_id(idx, node_idx) == Some(fragment) {
                out.push((id, node));
            }
        }
    }
    out.sort();
    out
}

/// A diamond: fragment 34 is referenced by fragments 32 and 33, which the
/// threat tree 30 folds under one AND gate, so every edit in 34 reaches
/// tree 30 along two paths. Tree 31 is rated and unrelated, and threat 70
/// reads tree 30.
fn diamond(m: &Arc<Methodology>) -> Project {
    let prov = Provenance::human("alice", 60);
    let root = node_id(30000);
    let mut user = AttackTree::new(root, Node::gate(GateKind::Or, "Compromise")).expect("tree");
    let pair = user
        .add_child(
            user.root(),
            node_id(30001),
            Node::gate(GateKind::And, "Both"),
        )
        .expect("gate");
    for n in [30002, 30003] {
        user.add_child(pair, node_id(n), Node::step("Reference"))
            .expect("reference step");
    }
    user.add_child(user.root(), node_id(30004), Node::step("Own"))
        .expect("own step");
    let safety = m.impact_category("safety").expect("category");
    let severe = m.level(safety, "severe").expect("level");
    project_with(
        m,
        &[
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: tree_id(30),
                tree: user,
                step_feasibility: vec![(node_id(30004), feas(m, "very-low"))],
                provenance: prov.clone(),
            },
            gate_tree(31, TreeRole::Threat, GateKind::Or, &[(1, Some("low"))], m),
            gate_tree(
                32,
                TreeRole::Fragment,
                GateKind::Or,
                &[(1, Some("very-low")), (2, None)],
                m,
            ),
            gate_tree(
                33,
                TreeRole::Fragment,
                GateKind::Or,
                &[(1, Some("very-low")), (2, None)],
                m,
            ),
            gate_tree(
                34,
                TreeRole::Fragment,
                GateKind::Or,
                &[(1, Some("high")), (2, Some("low"))],
                m,
            ),
            use_fragment(32, 2, 34),
            use_fragment(33, 2, 34),
            Op::UseFragment {
                tree: tree_id(30),
                node: node_id(30002),
                fragment: tree_id(32),
                provenance: prov.clone(),
            },
            Op::UseFragment {
                tree: tree_id(30),
                node: node_id(30003),
                fragment: tree_id(33),
                provenance: prov.clone(),
            },
            Op::AddAsset {
                id: asset_id(70),
                name: "ECU".into(),
                properties: m.all_properties(),
                provenance: prov.clone(),
            },
            Op::AddDamageScenario {
                id: damage_id(70),
                name: "Crash".into(),
                impacts: vec![(safety, severe)],
                provenance: prov.clone(),
            },
            Op::AddThreatScenario {
                id: threat_id(70),
                name: "Spoof".into(),
                asset: asset_id(70),
                category: m.threat_categories().next().expect("threat category"),
                catalog_refs: vec![],
                damages: vec![damage_id(70)],
                provenance: prov.clone(),
            },
            Op::SetThreatAttackTree {
                threat: threat_id(70),
                tree: Some(tree_id(30)),
                provenance: prov,
            },
        ],
    )
}

/// Every tree of [`diamond`]: the cache agrees with an independent full
/// evaluation and with a freshly built cache, and the public reverse index
/// agrees with a scan of every step.
fn check_fragments(p: &Project, cache: &ProjectEvalCache) {
    for n in [30, 31, 32, 33, 34] {
        let id = tree_id(n);
        assert_eq!(
            cache.evaluate_tree(p, id).cloned(),
            p.evaluate_tree(id),
            "inherent {n}"
        );
        assert_eq!(
            cache.evaluate_residual_tree(p, id).cloned(),
            p.evaluate_residual_tree(id),
            "residual {n}"
        );
    }
    assert_eq!(
        cache.evaluate_risk(p, threat_id(70)).cloned(),
        p.evaluate_risk(threat_id(70)),
        "risk"
    );
    assert_eq!(
        cache.evaluate_residual_risk(p, threat_id(70)).cloned(),
        p.evaluate_residual_risk(threat_id(70)),
        "residual risk"
    );
    // Hand-independent numbers: a fold of the project inputs alone.
    let mut oracle = fragment_oracle::Oracle::new(p);
    for n in [30, 31, 32, 33, 34] {
        let id = tree_id(n);
        for residual in [false, true] {
            let cached = if residual {
                cache.evaluate_residual_tree(p, id)
            } else {
                cache.evaluate_tree(p, id)
            };
            match (oracle.root(id, residual), cached) {
                (Ok(Lifted::Value(v)), Ok(e)) => {
                    assert_eq!(e.root_value(), &v, "oracle {n} residual={residual}")
                }
                (Ok(Lifted::Excluded), Err(ModelError::TreeNotApplicable(_))) => {}
                (Err(_), Err(e)) => assert!(!matches!(e, ModelError::TreeNotApplicable(_))),
                (expected, actual) => {
                    panic!("{n} residual={residual}: oracle {expected:?}, cache {actual:?}")
                }
            }
        }
    }
    for n in [32, 33, 34] {
        let idx = p.attack_tree_idx(&tree_id(n)).expect("live fragment");
        assert_eq!(
            path_ids(&p.fragment_users(idx)),
            scan_users(p, tree_id(n)),
            "users of {n}"
        );
    }
    assert_eq!(cache, &ProjectEvalCache::new(p), "incremental equals full");
}

#[test]
fn a_fragment_edit_folds_every_affected_tree_once_per_mode() {
    let m = builtin();
    let mut p = diamond(&m);
    let mut cache = ProjectEvalCache::new(&p);
    let (very_low, low, medium, high) = (
        feas(&m, "very-low"),
        feas(&m, "low"),
        feas(&m, "medium"),
        feas(&m, "high"),
    );
    assert_eq!(
        p.evaluate_tree(tree_id(30)).expect("user").root_value(),
        &high
    );
    let unrelated = cache
        .evaluate_tree(&p, tree_id(31))
        .expect("unrelated")
        .clone();
    let (nodes, threats) = (cache.recomputed_nodes(), cache.recomputed_threats());

    // Per mode: tree 34 folds its step and its root (2), trees 32 and 33
    // their reference step and their root (2 each), and tree 30 once with
    // both reference steps dirty (2 steps + the AND gate + the root = 4).
    // Folding tree 30 per path, or touching the unrelated tree, would raise
    // the count above 10 per mode.
    let rate = p
        .apply_cached(
            Op::SetStepFeasibility {
                target: StepTarget::Step(tree_id(34), node_id(34001)),
                level: Some(medium),
                provenance: Provenance::human("alice", 70),
            },
            &mut cache,
        )
        .expect("rate the shared fragment");
    assert_eq!(cache.recomputed_nodes() - nodes, 20, "10 folds per mode");
    assert_eq!(
        cache.recomputed_threats() - threats,
        2,
        "one threat, 2 modes"
    );
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(30))
            .expect("user")
            .root_value(),
        &medium
    );
    assert_eq!(
        cache.evaluate_tree(&p, tree_id(31)).expect("unrelated"),
        &unrelated
    );
    check_fragments(&p, &cache);

    let ctrl = control_id(70);
    for op in [
        Op::AddSecurityControl {
            id: ctrl,
            name: "SecOC".into(),
            provenance: Provenance::human("alice", 71),
        },
        Op::SetControlStatus {
            control: ctrl,
            status: ControlStatus::Planned,
            provenance: Provenance::human("alice", 72),
        },
    ] {
        p.apply_cached(op, &mut cache).expect("control");
    }
    let (nodes, threats) = (cache.recomputed_nodes(), cache.recomputed_threats());
    let mitigate = p
        .apply_cached(
            Op::SetStepMitigation {
                tree: tree_id(34),
                node: node_id(34001),
                control: ctrl,
                feasibility: Some(very_low),
                provenance: Provenance::human("alice", 73),
            },
            &mut cache,
        )
        .expect("plan a mitigation");
    // A planned control counts in the residual mode only, so the inherent
    // users are not dirtied at all.
    assert_eq!(cache.recomputed_nodes() - nodes, 10, "residual only");
    assert_eq!(cache.recomputed_threats() - threats, 1, "residual only");
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(30))
            .expect("inherent")
            .root_value(),
        &medium
    );
    assert_eq!(
        cache
            .evaluate_residual_tree(&p, tree_id(30))
            .expect("residual")
            .root_value(),
        &low
    );
    check_fragments(&p, &cache);

    let (nodes, threats) = (cache.recomputed_nodes(), cache.recomputed_threats());
    let implement = p
        .apply_cached(
            Op::SetControlStatus {
                control: ctrl,
                status: ControlStatus::Implemented,
                provenance: Provenance::human("alice", 74),
            },
            &mut cache,
        )
        .expect("implement the control");
    // Inherent gains the mitigation and propagates (10 folds). Residual
    // only re-reads the mitigated step (1): its value is unchanged there, so
    // no gate above it, and no user, is refolded.
    assert_eq!(cache.recomputed_nodes() - nodes, 11);
    assert_eq!(cache.recomputed_threats() - threats, 1);
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(30))
            .expect("inherent")
            .root_value(),
        &low
    );
    check_fragments(&p, &cache);

    for inv in [implement, mitigate, rate] {
        p.apply_cached(inv, &mut cache).expect("undo");
        check_fragments(&p, &cache);
    }
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(30))
            .expect("restored")
            .root_value(),
        &high
    );
}

#[test]
fn every_fragment_op_and_its_inverse_keep_users_and_the_index_exact() {
    let m = builtin();
    let mut p = diamond(&m);
    let (very_low, low, high) = (feas(&m, "very-low"), feas(&m, "low"), feas(&m, "high"));
    let shared = shared_step_id(70);
    let ctrl = control_id(70);
    let cond = condition_id(70);
    let prov = Provenance::human("alice", 80);
    // One shared step used from both fragments, a second reference to
    // fragment 32 in the using tree, and a planned control.
    for op in [
        Op::AddSharedStep {
            id: shared,
            name: "Shared".into(),
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: Some(low),
            provenance: prov.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl,
            name: "SecOC".into(),
            provenance: prov.clone(),
        },
        Op::SetControlStatus {
            control: ctrl,
            status: ControlStatus::Planned,
            provenance: prov.clone(),
        },
    ] {
        p.apply(op).expect("fixture");
    }
    share_in_fragments(&mut p, shared);
    for op in [
        Op::AddTreeNode {
            tree: tree_id(30),
            parent: node_id(30000),
            id: node_id(30005),
            node: Node::step("Reference again"),
            index: 5,
            position: 2,
            provenance: prov.clone(),
        },
        Op::UseFragment {
            tree: tree_id(30),
            node: node_id(30005),
            fragment: tree_id(32),
            provenance: prov.clone(),
        },
    ] {
        p.apply(op).expect("second reference");
    }
    let before = p.clone();
    let mut cache = ProjectEvalCache::new(&p);
    check_fragments(&p, &cache);
    assert_eq!(
        p.evaluate_tree(tree_id(30)).expect("user").root_value(),
        &high
    );

    let ops = vec![
        // A rating edit at the fragment root, then a gate edit that changes
        // how the fragment folds.
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34001)),
            level: Some(feas(&m, "medium")),
            provenance: prov.clone(),
        },
        Op::SetTreeNodeGate {
            tree: tree_id(34),
            id: node_id(34000),
            gate: GateKind::And,
            provenance: prov.clone(),
        },
        // Residual-only, then inherent too.
        Op::SetStepMitigation {
            tree: tree_id(34),
            node: node_id(34001),
            control: ctrl,
            feasibility: Some(very_low),
            provenance: prov.clone(),
        },
        Op::SetControlStatus {
            control: ctrl,
            status: ControlStatus::Implemented,
            provenance: prov.clone(),
        },
        // One shared-step edit with a user inside each fragment: unrating it
        // leaves both fragments, and the tree above them, unrated.
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: None,
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: Some(low),
            provenance: prov.clone(),
        },
        // Unlink and relink the second reference.
        Op::UnuseFragment {
            tree: tree_id(30),
            node: node_id(30005),
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(30), node_id(30005)),
            level: Some(low),
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(30), node_id(30005)),
            level: None,
            provenance: prov.clone(),
        },
        Op::UseFragment {
            tree: tree_id(30),
            node: node_id(30005),
            fragment: tree_id(32),
            provenance: prov.clone(),
        },
        // An insertion in the using tree moves every reference step's index.
        Op::AddTreeNode {
            tree: tree_id(30),
            parent: node_id(30000),
            id: node_id(30006),
            node: Node::step("Inserted"),
            index: 1,
            position: 0,
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(30), node_id(30006)),
            level: Some(very_low),
            provenance: prov.clone(),
        },
        // One edit whose users sit in both fragments, which must still reach
        // the moved reference steps of tree 30.
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: Some(high),
            provenance: prov.clone(),
        },
        // A condition excluding fragment 33 excludes the AND above it; a
        // local condition on the second reference excludes only that step.
        Op::AddCondition {
            id: cond,
            name: "Debug port".into(),
            description: "".into(),
            present: true,
            provenance: prov.clone(),
        },
        Op::NeedCondition {
            element: aegis_model::ElementRef::Tree(tree_id(33)),
            condition: cond,
            provenance: prov.clone(),
        },
        Op::SetConditionPresent {
            id: cond,
            present: false,
            provenance: prov.clone(),
        },
        Op::NeedStepCondition {
            tree: tree_id(30),
            node: node_id(30005),
            condition: cond,
            provenance: prov.clone(),
        },
        Op::SetConditionPresent {
            id: cond,
            present: true,
            provenance: prov.clone(),
        },
    ];
    let mut inverses = Vec::new();
    let mut run = |ops: Vec<Op>, p: &mut Project, cache: &mut ProjectEvalCache| {
        for op in ops {
            let inverse = p
                .apply_cached(op.clone(), cache)
                .unwrap_or_else(|e| panic!("{op:?}: {e}"));
            check_fragments(p, cache);
            inverses.push(inverse);
        }
    };
    run(ops, &mut p, &mut cache);
    // The implemented mitigation holds tree 34 at very-low, so the high
    // shared step decides both fragments, and tree 30 reads them through its
    // moved reference steps.
    assert_eq!(
        p.evaluate_tree(tree_id(30)).expect("shifted").root_value(),
        &high
    );
    // Deleting users: the second reference step, the inserted step before
    // the remaining references (their indices move down), then the whole
    // using tree; a fragment edit after each must reach only live users.
    let rate = |level: &str| Op::SetStepFeasibility {
        target: StepTarget::Step(tree_id(34), node_id(34002)),
        level: Some(feas(&m, level)),
        provenance: prov.clone(),
    };
    let deletions = vec![
        Op::UnneedStepCondition {
            tree: tree_id(30),
            node: node_id(30005),
            condition: cond,
            provenance: prov.clone(),
        },
        Op::UnuseFragment {
            tree: tree_id(30),
            node: node_id(30005),
            provenance: prov.clone(),
        },
        Op::RemoveTreeNode {
            tree: tree_id(30),
            id: node_id(30005),
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(30), node_id(30006)),
            level: None,
            provenance: prov.clone(),
        },
        Op::RemoveTreeNode {
            tree: tree_id(30),
            id: node_id(30006),
            provenance: prov.clone(),
        },
        rate("medium"),
        Op::SetThreatAttackTree {
            threat: threat_id(70),
            tree: None,
            provenance: prov.clone(),
        },
        // A tree holding references cannot be removed, so unlink first.
        Op::UnuseFragment {
            tree: tree_id(30),
            node: node_id(30002),
            provenance: prov.clone(),
        },
        Op::UnuseFragment {
            tree: tree_id(30),
            node: node_id(30003),
            provenance: prov.clone(),
        },
        Op::RemoveAttackTree { id: tree_id(30) },
        rate("high"),
    ];
    run(deletions, &mut p, &mut cache);
    assert_eq!(
        p.evaluate_tree(tree_id(30)),
        Err(ModelError::UnknownAttackTree(tree_id(30)))
    );
    for n in [32, 33] {
        assert!(scan_users(&p, tree_id(n)).is_empty(), "no users of {n}");
    }
    for inverse in inverses.into_iter().rev() {
        p.apply_cached(inverse.clone(), &mut cache)
            .unwrap_or_else(|e| panic!("inverse {inverse:?}: {e}"));
        check_fragments(&p, &cache);
    }
    assert_eq!(p, before, "every inverse restored the project");
    assert_eq!(
        p.evaluate_tree(tree_id(30)).expect("restored").root_value(),
        &high
    );
}

/// Gives fragments 32 and 33 of [`diamond`] each a step using `shared`.
fn share_in_fragments(p: &mut Project, shared: aegis_model::SharedStepId) {
    let prov = Provenance::human("alice", 81);
    for t in [32u64, 33] {
        p.apply(Op::AddTreeNode {
            tree: tree_id(t),
            parent: node_id(1000 * t),
            id: node_id(1000 * t + 3),
            node: Node::step("Shared"),
            index: 3,
            position: 2,
            provenance: prov.clone(),
        })
        .expect("shared step slot");
        p.apply(Op::UseSharedStep {
            tree: tree_id(t),
            node: node_id(1000 * t + 3),
            step: shared,
            provenance: prov.clone(),
        })
        .expect("use the shared step");
    }
}

/// The last step of the cross-tree derivation of tree 30's root.
fn why_end(p: &Project) -> (TreeId, NodeId) {
    let path = p
        .fragment_why_path(tree_id(30), node_id(30000), false)
        .expect("why path");
    *path_ids(&path).last().expect("non-empty path")
}

#[test]
fn why_only_metadata_and_shared_edits_fold_the_minimum() {
    let m = builtin();
    let mut p = diamond(&m);
    let mut cache = ProjectEvalCache::new(&p);
    let prov = Provenance::human("alice", 90);
    let rate = |node: u64, level: &str| Op::SetStepFeasibility {
        target: StepTarget::Step(tree_id(34), node_id(node)),
        level: Some(feas(&m, level)),
        provenance: prov.clone(),
    };
    let delta = |cache: &ProjectEvalCache, before: (u64, u64)| {
        (
            cache.recomputed_nodes() - before.0,
            cache.recomputed_threats() - before.1,
        )
    };
    let counters =
        |cache: &ProjectEvalCache| (cache.recomputed_nodes(), cache.recomputed_threats());
    assert_eq!(why_end(&p), (tree_id(34), node_id(34001)));

    // Why-only: tree 34 stays OR(high, high) = high, then OR(low, high) =
    // high, so each edit refolds the step and the root of tree 34 in each
    // mode (4) and no user; the explanation now ends at the other step.
    let mut inverses = Vec::new();
    for op in [rate(34002, "high"), rate(34001, "low")] {
        let before = counters(&cache);
        inverses.push(p.apply_cached(op, &mut cache).expect("why-only edit"));
        assert_eq!(delta(&cache, before), (4, 0), "root unchanged");
        check_fragments(&p, &cache);
    }
    assert_eq!(why_end(&p), (tree_id(34), node_id(34002)));
    let idx = p.attack_tree_idx(&tree_id(34)).expect("fragment");
    let tree = p.attack_tree(idx).expect("fragment");
    let cached = cache.evaluate_tree(&p, tree_id(34)).expect("cached");
    let local = cached.why_path(tree.root()).last().expect("path");
    assert_eq!(tree.id_of(local), Some(node_id(34002)));

    // Metadata only: nothing is refolded.
    let before = counters(&cache);
    for (tree, node) in [(34, 34001), (30, 30002), (32, 32000)] {
        inverses.push(
            p.apply_cached(
                Op::RenameTreeNode {
                    tree: tree_id(tree),
                    id: node_id(node),
                    label: "Renamed".into(),
                    provenance: prov.clone(),
                },
                &mut cache,
            )
            .expect("rename"),
        );
        check_fragments(&p, &cache);
    }
    assert_eq!(delta(&cache, before), (0, 0), "metadata folds nothing");
    for inverse in inverses.into_iter().rev() {
        p.apply_cached(inverse, &mut cache).expect("undo");
        check_fragments(&p, &cache);
    }
    assert_eq!(why_end(&p), (tree_id(34), node_id(34001)));

    // One shared step used inside both fragments 32 and 33, which tree 30
    // reads under one AND gate. With tree 34 at OR(very-low, low) = low and
    // the shared step at low, everything above is low.
    let shared = shared_step_id(71);
    for op in [
        Op::AddSharedStep {
            id: shared,
            name: "Shared".into(),
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: Some(feas(&m, "low")),
            provenance: prov.clone(),
        },
        rate(34001, "very-low"),
    ] {
        p.apply(op).expect("fixture");
    }
    share_in_fragments(&mut p, shared);
    let mut cache = ProjectEvalCache::new(&p);
    check_fragments(&p, &cache);
    assert_eq!(
        p.evaluate_tree(tree_id(30)).expect("user").root_value(),
        &feas(&m, "low")
    );
    // Per mode: the shared step and the root of trees 32 and 33 (2 each),
    // then tree 30 once with both reference steps dirty (2 + AND + root).
    // Propagating each shared user separately would fold tree 30 twice
    // (3 + 3 per mode) and give 20.
    let before = counters(&cache);
    let raise = p
        .apply_cached(
            Op::SetStepFeasibility {
                target: StepTarget::Shared(shared),
                level: Some(feas(&m, "high")),
                provenance: prov.clone(),
            },
            &mut cache,
        )
        .expect("raise the shared step");
    assert_eq!(delta(&cache, before), (16, 2), "one batch");
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(30))
            .expect("user")
            .root_value(),
        &feas(&m, "high")
    );
    check_fragments(&p, &cache);
    let before = counters(&cache);
    p.apply_cached(raise, &mut cache).expect("undo");
    assert_eq!(delta(&cache, before), (16, 2), "one batch back");
    check_fragments(&p, &cache);
}

/// A host with two references, one of them through a nested reference and
/// lowered by a planned control: tipping one reference leaves the other at
/// its resolved value, and the reported tips equal both a hand derivation
/// and the independent input fold, in each mode.
#[test]
fn tipping_a_reference_matches_hand_derived_and_independent_tips() {
    let m = builtin();
    let ctrl = control_id(40);
    let prov = Provenance::human("alice", 95);
    let p = project_with(
        &m,
        &[
            gate_tree(
                42,
                TreeRole::Fragment,
                GateKind::Or,
                &[(1, Some("medium"))],
                &m,
            ),
            gate_tree(
                41,
                TreeRole::Fragment,
                GateKind::And,
                &[(1, Some("high")), (2, None)],
                &m,
            ),
            gate_tree(
                40,
                TreeRole::Threat,
                GateKind::Or,
                &[(1, Some("low")), (2, None), (3, None)],
                &m,
            ),
            use_fragment(41, 2, 42),
            use_fragment(40, 2, 41),
            use_fragment(40, 3, 42),
            Op::AddSecurityControl {
                id: ctrl,
                name: "Guard".into(),
                provenance: prov.clone(),
            },
            Op::SetControlStatus {
                control: ctrl,
                status: ControlStatus::Planned,
                provenance: prov.clone(),
            },
            Op::SetStepMitigation {
                tree: tree_id(40),
                node: node_id(40003),
                control: ctrl,
                feasibility: Some(feas(&m, "very-low")),
                provenance: prov,
            },
        ],
    );
    let (low, medium, high) = (feas(&m, "low"), feas(&m, "medium"), feas(&m, "high"));
    // Inherent: OR(low, AND(high, medium), medium) = medium; residual lowers
    // the direct reference to very-low, so 40002 alone holds the root.
    let mut inherent = vec![
        (node_id(40002), medium, high, medium, high),
        (node_id(40003), medium, high, medium, high),
    ];
    let mut residual = vec![
        (node_id(40002), medium, low, medium, low),
        (node_id(40002), medium, high, medium, high),
    ];
    inherent.sort();
    residual.sort();
    let mut oracle = fragment_oracle::Oracle::new(&p);
    let tree = p
        .attack_tree(p.attack_tree_idx(&tree_id(40)).expect("host"))
        .expect("host");
    for (mode, expected) in [(false, inherent), (true, residual)] {
        assert_eq!(oracle.root(tree_id(40), mode), Ok(Lifted::Value(medium)));
        assert_eq!(oracle.tips(tree_id(40), mode).as_ref(), Ok(&expected));
        let tips = if mode {
            p.residual_tree_tipping_points(tree_id(40))
        } else {
            p.tree_tipping_points(tree_id(40))
        }
        .expect("tips");
        let mut actual: Vec<_> = tips
            .iter()
            .map(|t| {
                (
                    tree.id_of(t.step()).expect("node"),
                    t.from_step(),
                    t.to_step(),
                    t.from_root(),
                    t.to_root(),
                )
            })
            .collect();
        actual.sort();
        assert_eq!(actual, expected, "residual={mode}");
    }
}

#[test]
fn set_tree_node_kind_retains_identities_edges_role_and_round_trips() {
    let m = builtin();
    let p_init = Provenance::human("alice", 10);
    let p_edit = Provenance::human("bob", 20);
    let p_edit2 = Provenance::ai("carol", 30);

    // Threat tree with a threat link, an empty root step at first converted to OR,
    // plus a fragment tree whose root is a bare Step, and a non-root multiply-parented node.
    let (threat_tree, frag_tree) = (tree_id(10), tree_id(11));
    let (root, g1, g2, s0, m_node, s2, s3) = (
        node_id(100),
        node_id(101),
        node_id(102),
        node_id(103),
        node_id(104),
        node_id(105),
        node_id(106),
    );
    let frag_root = node_id(200);
    let (asset, damage, threat) = (asset_id(10), damage_id(10), threat_id(10));
    let safety = m.impact_category("safety").expect("safety");
    let severe = m.level(safety, "severe").expect("severe");

    let mut p = project_with(
        &m,
        &[
            Op::AddAsset {
                id: asset,
                name: "Gateway".into(),
                properties: m.all_properties(),
                provenance: p_init.clone(),
            },
            Op::AddDamageScenario {
                id: damage,
                name: "Loss".into(),
                impacts: vec![(safety, severe)],
                provenance: p_init.clone(),
            },
            Op::AddThreatScenario {
                id: threat,
                name: "Tamper".into(),
                asset,
                category: m.threat_categories().next().expect("cat"),
                catalog_refs: vec![],
                damages: vec![damage],
                provenance: p_init.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: threat_tree,
                tree: AttackTree::new(root, Node::step("Threat root")).expect("root"),
                step_feasibility: vec![],
                provenance: p_init.clone(),
            },
            Op::SetThreatAttackTree {
                threat,
                tree: Some(threat_tree),
                provenance: p_init.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Fragment,
                id: frag_tree,
                tree: AttackTree::new(frag_root, Node::step("Frag root")).expect("frag"),
                step_feasibility: vec![],
                provenance: p_init.clone(),
            },
        ],
    );

    // 1. Root Step <-> empty OR / AND / SEQ_AND on both Threat and Fragment trees.
    for target_tree in [threat_tree, frag_tree] {
        let target_root = if target_tree == threat_tree {
            root
        } else {
            frag_root
        };
        let expected_role = if target_tree == threat_tree {
            TreeRole::Threat
        } else {
            TreeRole::Fragment
        };
        let expected_visual = if target_tree == threat_tree {
            "AT00"
        } else {
            "TT01"
        };
        for kind in [GateKind::Or, GateKind::And, GateKind::SeqAnd] {
            let before = p.clone();
            let op = Op::SetTreeNodeKind {
                tree: target_tree,
                id: target_root,
                gate: Some(kind),
                provenance: p_edit.clone(),
            };
            assert_eq!(p.validate(&op), Ok(()));
            let inv = p.apply(op.clone()).expect("step -> gate");
            assert_eq!(
                inv,
                Op::SetTreeNodeKind {
                    tree: target_tree,
                    id: target_root,
                    gate: None,
                    provenance: p_init.clone(),
                }
            );
            let tidx = p.attack_tree_idx(&target_tree).expect("idx");
            assert_eq!(p.attack_tree_role(tidx), Some(expected_role));
            assert_eq!(
                p.attack_tree_visual_id(tidx).as_deref(),
                Some(expected_visual)
            );
            let t = p.attack_tree(tidx).expect("tree");
            let r_idx = t.root();
            assert_eq!(t.idx_of(&target_root), Some(r_idx));
            assert_eq!(t.get(r_idx).and_then(Node::gate_kind), Some(kind));
            assert_eq!(p.attack_tree_provenance(tidx).expect("prov"), &p_edit);

            // Round-trip snapshot, op log, and workspace files with empty gate root.
            let snap = save(&p);
            let loaded = Project::load(snap.as_slice(), Arc::clone(&m)).expect("snap");
            assert_eq!(loaded, p);
            assert_eq!(save(&loaded), snap);
            let files = p.to_workspace_files().expect("ws");
            let ws_loaded = Project::from_workspace_files(
                files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
                Arc::clone(&m),
            )
            .expect("ws load");
            assert_eq!(ws_loaded, p);
            let log_bytes = write_log(&m, std::slice::from_ref(&op));
            let log_back: Vec<Op> = read_log(&m, &log_bytes).into_iter().map(|r| r.op).collect();
            assert_eq!(log_back.as_slice(), std::slice::from_ref(&op));

            // Undo back to bare Step root and redo.
            let redo = p.apply(inv).expect("undo");
            assert_eq!(redo, op);
            assert_eq!(p, before);
            assert_eq!(save(&p), save(&before));
        }
    }

    // 2. Non-root multiply-parented node Step <-> empty Gate retains ID, label, index,
    // owner, parents, children ordering, threat link, role, and visual ID.
    p.apply(Op::SetTreeNodeKind {
        tree: threat_tree,
        id: root,
        gate: Some(GateKind::Or),
        provenance: p_init.clone(),
    })
    .expect("root gate");
    for op in [
        Op::AddTreeNode {
            tree: threat_tree,
            parent: root,
            id: g1,
            node: Node::gate(GateKind::And, "G1"),
            index: 1,
            position: 0,
            provenance: p_init.clone(),
        },
        Op::AddTreeNode {
            tree: threat_tree,
            parent: root,
            id: g2,
            node: Node::gate(GateKind::Or, "G2"),
            index: 2,
            position: 1,
            provenance: p_init.clone(),
        },
        Op::AddTreeNode {
            tree: threat_tree,
            parent: g1,
            id: s0,
            node: Node::step("S0"),
            index: 3,
            position: 0,
            provenance: p_init.clone(),
        },
        Op::AddTreeNode {
            tree: threat_tree,
            parent: g1,
            id: m_node,
            node: Node::step("Shared bare node"),
            index: 4,
            position: 1,
            provenance: p_init.clone(),
        },
        Op::AddTreeNode {
            tree: threat_tree,
            parent: g1,
            id: s2,
            node: Node::step("S2"),
            index: 5,
            position: 2,
            provenance: p_init.clone(),
        },
        Op::AddTreeNode {
            tree: threat_tree,
            parent: g2,
            id: s3,
            node: Node::step("S3"),
            index: 6,
            position: 0,
            provenance: p_init.clone(),
        },
        Op::LinkTreeNode {
            tree: threat_tree,
            parent: g2,
            child: m_node,
            position: 0,
            slot: 0,
            provenance: p_init.clone(),
        },
    ] {
        p.apply(op).expect("build tree");
    }

    let before_multi = p.clone();
    let tidx = p.attack_tree_idx(&threat_tree).expect("idx");
    let t_before = p.attack_tree(tidx).expect("tree");
    let m_idx_before = t_before.idx_of(&m_node).expect("m_idx");
    let g1_idx = t_before.idx_of(&g1).expect("g1");
    let g2_idx = t_before.idx_of(&g2).expect("g2");
    let g1_children_before = t_before.children(g1_idx).to_vec();
    let g2_children_before = t_before.children(g2_idx).to_vec();
    let m_parents_before: Vec<_> = t_before.parents(m_idx_before).collect();

    let to_gate = Op::SetTreeNodeKind {
        tree: threat_tree,
        id: m_node,
        gate: Some(GateKind::SeqAnd),
        provenance: p_edit.clone(),
    };
    let inv1 = p.apply(to_gate.clone()).expect("multi step -> gate");
    assert_eq!(
        inv1,
        Op::SetTreeNodeKind {
            tree: threat_tree,
            id: m_node,
            gate: None,
            provenance: p_init.clone(),
        }
    );
    let to_or = Op::SetTreeNodeKind {
        tree: threat_tree,
        id: m_node,
        gate: Some(GateKind::Or),
        provenance: p_edit2.clone(),
    };
    let inv2 = p.apply(to_or.clone()).expect("empty gate -> empty gate");
    assert_eq!(
        inv2,
        Op::SetTreeNodeKind {
            tree: threat_tree,
            id: m_node,
            gate: Some(GateKind::SeqAnd),
            provenance: p_edit.clone(),
        }
    );

    let tidx = p.attack_tree_idx(&threat_tree).expect("idx");
    let t_after = p.attack_tree(tidx).expect("tree");
    assert_eq!(t_after.idx_of(&m_node), Some(m_idx_before));
    assert_eq!(t_after.id_of(m_idx_before), Some(m_node));
    assert_eq!(
        t_after.get(m_idx_before).map(Node::label),
        Some("Shared bare node")
    );
    assert_eq!(
        t_after.get(m_idx_before).and_then(Node::gate_kind),
        Some(GateKind::Or)
    );
    assert_eq!(t_after.owner(m_idx_before), Some(g1_idx));
    assert_eq!(
        t_after.parents(m_idx_before).collect::<Vec<_>>(),
        m_parents_before
    );
    assert_eq!(t_after.children(g1_idx), g1_children_before.as_slice());
    assert_eq!(t_after.children(g2_idx), g2_children_before.as_slice());
    assert_eq!(p.attack_tree_role(tidx), Some(TreeRole::Threat));
    assert_eq!(p.attack_tree_visual_id(tidx).as_deref(), Some("AT00"));
    assert_eq!(
        p.threat_scenario(p.threat_scenario_idx(&threat).expect("th"))
            .expect("th")
            .attack_tree(),
        Some(tidx)
    );
    assert_eq!(p.attack_tree_provenance(tidx).expect("prov"), &p_edit2);

    // Log, snapshot, and workspace round trips on the multi-parent empty gate.
    let log = write_log(&m, &[to_gate.clone(), to_or.clone()]);
    assert_eq!(
        read_log(&m, &log)
            .into_iter()
            .map(|r| r.op)
            .collect::<Vec<_>>(),
        [to_gate, to_or.clone()]
    );
    let snap = save(&p);
    assert_eq!(
        Project::load(snap.as_slice(), Arc::clone(&m)).expect("snap"),
        p
    );
    let files = p.to_workspace_files().expect("ws");
    assert_eq!(
        Project::from_workspace_files(
            files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
            Arc::clone(&m),
        )
        .expect("ws"),
        p
    );

    // Undo inv2 then inv1 -> exact original project, then redo inv2's redo.
    let redo2 = p.apply(inv2).expect("undo 2");
    assert_eq!(redo2, to_or);
    p.apply(inv1).expect("undo 1");
    assert_eq!(p, before_multi);
    assert_eq!(save(&p), save(&before_multi));
}

#[test]
fn set_tree_node_kind_rejects_non_bare_nodes_and_bad_provenance_without_mutation() {
    let m = builtin();
    let prov = Provenance::human("alice", 10);
    let tree = tree_id(50);
    let frag = tree_id(51);
    let (root, step, gate_with_child) = (node_id(500), node_id(501), node_id(502));
    let shared = shared_step_id(50);
    let ctrl = control_id(50);
    let cond = condition_id(50);
    let comp = common::component_id(50);
    let assump = aegis_model::AssumptionId::from_uuid(common::v7(50)).expect("v7");

    let mut p = project_with(
        &m,
        &[
            Op::AddSharedStep {
                id: shared,
                name: "Shared".into(),
                provenance: prov.clone(),
            },
            Op::AddSecurityControl {
                id: ctrl,
                name: "Control".into(),
                provenance: prov.clone(),
            },
            Op::AddCondition {
                id: cond,
                name: "Cond".into(),
                description: "".into(),
                present: true,
                provenance: prov.clone(),
            },
            Op::AddComponent {
                id: comp,
                name: "ECU".into(),
                parent: None,
                provenance: prov.clone(),
            },
            Op::AddAssumption {
                id: assump,
                name: "Assump".into(),
                holds: true,
                provenance: prov.clone(),
            },
            add_tree(51, TreeRole::Fragment),
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: tree,
                tree: AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("root"),
                step_feasibility: vec![],
                provenance: prov.clone(),
            },
            Op::AddTreeNode {
                tree,
                parent: root,
                id: gate_with_child,
                node: Node::gate(GateKind::And, "Parent"),
                index: 1,
                position: 0,
                provenance: prov.clone(),
            },
            Op::AddTreeNode {
                tree,
                parent: gate_with_child,
                id: step,
                node: Node::step("Leaf"),
                index: 2,
                position: 0,
                provenance: prov.clone(),
            },
        ],
    );

    let assert_rejects = |proj: &mut Project, op: Op, expected: ModelError| {
        let before = proj.clone();
        let bytes = save(proj);
        assert_eq!(proj.validate(&op), Err(expected.clone()), "{op:?}");
        assert_eq!(proj.apply(op.clone()), Err(expected), "{op:?}");
        assert_eq!(proj, &before);
        assert_eq!(save(proj), bytes);
    };

    // Unknown tree / node / blank provenance on bare node.
    assert_rejects(
        &mut p,
        Op::SetTreeNodeKind {
            tree: tree_id(999),
            id: step,
            gate: Some(GateKind::Or),
            provenance: prov.clone(),
        },
        ModelError::UnknownAttackTree(tree_id(999)),
    );
    assert_rejects(
        &mut p,
        Op::SetTreeNodeKind {
            tree,
            id: node_id(999),
            gate: Some(GateKind::Or),
            provenance: prov.clone(),
        },
        ModelError::UnknownTreeNode {
            tree,
            node: node_id(999),
        },
    );
    assert_rejects(
        &mut p,
        Op::SetTreeNodeKind {
            tree,
            id: step,
            gate: Some(GateKind::Or),
            provenance: Provenance::human("   ", 11),
        },
        ModelError::EmptyActor,
    );

    // Child-bearing gates (root and non-root) cannot change kind, even to another GateKind.
    for gate in [None, Some(GateKind::And)] {
        assert_rejects(
            &mut p,
            Op::SetTreeNodeKind {
                tree,
                id: root,
                gate,
                provenance: prov.clone(),
            },
            ModelError::BareTreeNodeRequired { tree, node: root },
        );
        assert_rejects(
            &mut p,
            Op::SetTreeNodeKind {
                tree,
                id: gate_with_child,
                gate,
                provenance: prov.clone(),
            },
            ModelError::BareTreeNodeRequired {
                tree,
                node: gate_with_child,
            },
        );
    }

    // Each step attachment blocks SetTreeNodeKind with BareTreeNodeRequired, and clearing restores bareness.
    let f = m.feasibility_factors().next().expect("factor");
    let fl = m.factor_levels(f).next().expect("factor level");
    let at = m.attack_types().next().expect("attack type");
    let blockers: Vec<Op> = vec![
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree, step),
            level: Some(feas(&m, "high")),
            provenance: prov.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(tree, step),
            factor: f,
            level: Some(fl),
            provenance: prov.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree, step),
            via: Some(aegis_model::ItemRef::Component(comp)),
            provenance: prov.clone(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, step),
            attack_type: Some(at),
            provenance: prov.clone(),
        },
        Op::UseSharedStep {
            tree,
            node: step,
            step: shared,
            provenance: prov.clone(),
        },
        Op::UseFragment {
            tree,
            node: step,
            fragment: frag,
            provenance: prov.clone(),
        },
        Op::SetStepMitigation {
            tree,
            node: step,
            control: ctrl,
            feasibility: Some(feas(&m, "low")),
            provenance: prov.clone(),
        },
        Op::NeedStepCondition {
            tree,
            node: step,
            condition: cond,
            provenance: prov.clone(),
        },
        Op::SetReviewMark {
            tree,
            node: step,
            marked: true,
            provenance: prov.clone(),
        },
        Op::SetAssumptionExcludes {
            id: assump,
            trees: vec![],
            steps: vec![StepRef { tree, node: step }],
            provenance: prov.clone(),
        },
    ];

    for setup_op in blockers {
        let before_block = p.clone();
        let clear = p.apply(setup_op.clone()).expect("attach");
        assert_rejects(
            &mut p,
            Op::SetTreeNodeKind {
                tree,
                id: step,
                gate: Some(GateKind::Or),
                provenance: prov.clone(),
            },
            ModelError::BareTreeNodeRequired { tree, node: step },
        );
        // Clearing restores bareness and exact original state.
        p.apply(clear).expect("clear");
        assert_eq!(p, before_block);
        let to_gate = p
            .apply(Op::SetTreeNodeKind {
                tree,
                id: step,
                gate: Some(GateKind::Or),
                provenance: prov.clone(),
            })
            .expect("bare after clear");
        p.apply(to_gate).expect("restore step");
        assert_eq!(p, before_block);
    }
}

#[test]
fn set_tree_node_kind_wire_codec_is_strict() {
    let m = builtin();
    let codec = aegis_model::OpCodec::new(Arc::clone(&m));
    let t = tree_id(60);
    let n = node_id(600);
    let p_human = Provenance::human("alice", 42);

    let cases = [
        (
            Op::SetTreeNodeKind {
                tree: t,
                id: n,
                gate: None,
                provenance: Provenance::default(),
            },
            format!(r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":null}}"#),
        ),
        (
            Op::SetTreeNodeKind {
                tree: t,
                id: n,
                gate: Some(GateKind::Or),
                provenance: Provenance::default(),
            },
            format!(r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":"or"}}"#),
        ),
        (
            Op::SetTreeNodeKind {
                tree: t,
                id: n,
                gate: Some(GateKind::And),
                provenance: Provenance::default(),
            },
            format!(r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":"and"}}"#),
        ),
        (
            Op::SetTreeNodeKind {
                tree: t,
                id: n,
                gate: Some(GateKind::SeqAnd),
                provenance: p_human,
            },
            format!(
                r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":"seq_and","provenance":{{"actor":"alice","timestamp_ms":42,"source":"human"}}}}"#
            ),
        ),
    ];

    for (op, expected_json) in &cases {
        let bytes = codec.encode(op).expect("encode");
        assert_eq!(std::str::from_utf8(&bytes).expect("utf8"), expected_json);
        assert_eq!(&codec.decode(&bytes).expect("decode"), op);
        let log_bytes = write_log(&m, std::slice::from_ref(op));
        let back: Vec<Op> = read_log(&m, &log_bytes).into_iter().map(|r| r.op).collect();
        assert_eq!(&back, std::slice::from_ref(op));
    }

    for bad in [
        format!(r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}"}}"#),
        format!(r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":"xor"}}"#),
        format!(r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":1}}"#),
        format!(
            r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":null,"extra":true}}"#
        ),
        format!(r#"{{"op":"set_tree_node_kind","tree":"not-uuid","id":"{n}","gate":null}}"#),
        format!(r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"BAD","gate":null}}"#),
        format!(r#"{{"op":"SetTreeNodeKind","tree":"{t}","id":"{n}","gate":null}}"#),
        format!(
            r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":null,"provenance":null}}"#
        ),
        format!(
            r#"{{"op":"set_tree_node_kind","tree":"{t}","id":"{n}","gate":null,"provenance":{{"actor":"","timestamp_ms":0,"source":"human"}}}}"#
        ),
    ] {
        assert!(
            codec.decode(bad.as_bytes()).is_err(),
            "codec should reject {bad}"
        );
        let log = format!(
            "{}\n{{\"seq\":1,\"op\":{bad}}}\n",
            std::str::from_utf8(&write_log(&m, &[]))
                .expect("utf8")
                .trim_end()
        );
        assert!(
            aegis_model::OpLogReader::open(log.as_bytes(), Arc::clone(&m))
                .expect("header")
                .next()
                .expect("record")
                .is_err(),
            "op log should reject {bad}"
        );
    }
}

#[test]
fn set_tree_node_kind_updates_cache_and_fragment_users_transitively() {
    let m = builtin();
    let mut p = diamond(&m);
    let prov = Provenance::human("alice", 99);
    let before = p.clone();
    let mut cache = ProjectEvalCache::new(&p);
    check_fragments(&p, &cache);

    // Split fragment 34's second leaf (34002, rated low) into an AND gate with two children,
    // raising its value to high so fragments 32, 33, host 30, and threat 70 update transitively.
    let ops = vec![
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34002)),
            level: None,
            provenance: prov.clone(),
        },
        Op::SetTreeNodeKind {
            tree: tree_id(34),
            id: node_id(34002),
            gate: Some(GateKind::And),
            provenance: prov.clone(),
        },
        Op::AddTreeNode {
            tree: tree_id(34),
            parent: node_id(34002),
            id: node_id(34003),
            node: Node::step("Child A"),
            index: 3,
            position: 0,
            provenance: prov.clone(),
        },
        Op::AddTreeNode {
            tree: tree_id(34),
            parent: node_id(34002),
            id: node_id(34004),
            node: Node::step("Child B"),
            index: 4,
            position: 1,
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34003)),
            level: Some(feas(&m, "medium")),
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34004)),
            level: Some(feas(&m, "medium")),
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34001)),
            level: Some(feas(&m, "very-low")),
            provenance: prov.clone(),
        },
    ];

    let mut inverses = Vec::new();
    for op in ops {
        let inv = p.apply_cached(op, &mut cache).expect("apply cached");
        check_fragments(&p, &cache);
        inverses.push(inv);
    }
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(30))
            .expect("host")
            .root_value(),
        &feas(&m, "medium")
    );

    // Also test root kind change on fragment 34 after clearing its children, then undo all back to before.
    let mut cleanup_inverses = Vec::new();
    for op in [
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34001)),
            level: None,
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34003)),
            level: None,
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34004)),
            level: None,
            provenance: prov.clone(),
        },
        Op::RemoveTreeNode {
            tree: tree_id(34),
            id: node_id(34004),
            provenance: prov.clone(),
        },
        Op::RemoveTreeNode {
            tree: tree_id(34),
            id: node_id(34003),
            provenance: prov.clone(),
        },
        Op::RemoveTreeNode {
            tree: tree_id(34),
            id: node_id(34002),
            provenance: prov.clone(),
        },
        Op::RemoveTreeNode {
            tree: tree_id(34),
            id: node_id(34001),
            provenance: prov.clone(),
        },
        Op::SetTreeNodeKind {
            tree: tree_id(34),
            id: node_id(34000),
            gate: None,
            provenance: prov.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree_id(34), node_id(34000)),
            level: Some(feas(&m, "high")),
            provenance: prov,
        },
    ] {
        let inv = p.apply_cached(op, &mut cache).expect("collapse to step");
        check_fragments(&p, &cache);
        cleanup_inverses.push(inv);
    }
    assert_eq!(
        cache
            .evaluate_tree(&p, tree_id(30))
            .expect("host")
            .root_value(),
        &feas(&m, "high")
    );

    for inv in cleanup_inverses
        .into_iter()
        .rev()
        .chain(inverses.into_iter().rev())
    {
        p.apply_cached(inv, &mut cache).expect("undo");
        check_fragments(&p, &cache);
    }
    assert_eq!(p, before);
}
