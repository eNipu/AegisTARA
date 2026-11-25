//! Node-level attack tree operations (job 065, FND-02 and FND-03): add,
//! link, unlink, remove, rename and set the gate of one node, each undone by
//! its inverse, and trees of up to 4,096 nodes built from small ops.

use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, DamageId, GateKind, MAX_ATTACK_TREE_NODES, MAX_TREE_OP_NODES, Methodology,
    ModelError, Node, NodeId, Op, OpCodec, OpLogReader, OpLogWriter, Project, ProjectEvalCache,
    Provenance, StepTarget, ThreatId, TreeId,
};

struct Fx {
    project: Project,
    tree: TreeId,
    threat: ThreatId,
    root: NodeId,
}

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

/// A project with one threat that has a one-gate attack tree.
fn fixture() -> Fx {
    let m = methodology();
    let integrity = m.property("integrity").expect("property");
    let tampering = m.threat_category("tampering").expect("category");
    let safety = m.impact_category("safety").expect("category");
    let severe = m.level(safety, "severe").expect("level");
    let mut project = Project::new(Arc::clone(&m));
    let (asset, damage, threat, tree, root) = (
        AssetId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
        TreeId::new_v7(),
        NodeId::new_v7(),
    );
    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: [integrity].into_iter().collect(),
            provenance: Provenance::default(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Damage".into(),
            impacts: vec![(safety, severe)],
            provenance: Provenance::default(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Threat".into(),
            asset,
            category: tampering,
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: Provenance::default(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tree,
            tree: AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree"),
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
        Op::SetThreatAttackTree {
            threat,
            tree: Some(tree),
            provenance: Provenance::default(),
        },
    ] {
        project.apply(op).expect("applies");
    }
    Fx {
        project,
        tree,
        threat,
        root,
    }
}

fn who(at: u64) -> Provenance {
    Provenance::human("alice", at)
}

fn add(fx: &Fx, parent: NodeId, id: NodeId, node: Node, index: u32, position: u32) -> Op {
    Op::AddTreeNode {
        tree: fx.tree,
        parent,
        id,
        node,
        index,
        position,
        provenance: who(1),
    }
}

fn nodes_of(project: &Project, tree: TreeId) -> usize {
    let idx = project.attack_tree_idx(&tree).expect("tree");
    project.attack_tree(idx).expect("tree").len()
}

#[test]
fn a_node_is_added_and_removed_and_undo_restores_the_exact_tree() {
    let mut fx = fixture();
    let before = fx.project.clone();
    let a = NodeId::new_v7();
    let undo_a = fx
        .project
        .apply(add(&fx, fx.root, a, Node::step("Step A"), 1, 0))
        .expect("add");
    assert_eq!(nodes_of(&fx.project, fx.tree), 2);
    assert_eq!(
        undo_a,
        Op::RemoveTreeNode {
            tree: fx.tree,
            id: a,
            provenance: Provenance::default()
        }
    );
    // A node inserted in the middle shifts the ones after it.
    let b = NodeId::new_v7();
    let c = NodeId::new_v7();
    fx.project
        .apply(add(&fx, fx.root, b, Node::step("B"), 2, 1))
        .expect("add");
    let after_b = fx.project.clone();
    let undo_c = fx
        .project
        .apply(add(&fx, fx.root, c, Node::step("C"), 1, 0))
        .expect("insert before A");
    let idx = fx.project.attack_tree_idx(&fx.tree).expect("tree");
    let tree = fx.project.attack_tree(idx).expect("tree");
    let labels: Vec<_> = tree
        .children(tree.root())
        .iter()
        .map(|&i| tree.get(i).expect("n").label().to_owned())
        .collect();
    assert_eq!(labels, ["C", "Step A", "B"], "children keep their order");
    fx.project.apply(undo_c).expect("undo");
    assert_eq!(fx.project, after_b);
    fx.project
        .apply(Op::RemoveTreeNode {
            tree: fx.tree,
            id: b,
            provenance: who(2),
        })
        .expect("remove");
    fx.project.apply(undo_a).expect("undo add a");
    assert_eq!(fx.project, before, "everything undone");
}

#[test]
fn bad_placements_and_duplicates_are_refused() {
    let mut fx = fixture();
    let a = NodeId::new_v7();
    let (tree, root) = (fx.tree, fx.root);
    let op = move |id, node, index, position| Op::AddTreeNode {
        tree,
        parent: root,
        id,
        node,
        index,
        position,
        provenance: who(1),
    };
    // The root is index 0, so a child cannot take index 0 or skip past the end.
    assert_eq!(
        fx.project.validate(&op(a, Node::step("A"), 0, 0)),
        Err(ModelError::BadNodePlacement)
    );
    assert_eq!(
        fx.project.validate(&op(a, Node::step("A"), 2, 0)),
        Err(ModelError::BadNodePlacement)
    );
    assert_eq!(
        fx.project.validate(&op(a, Node::step("A"), 1, 1)),
        Err(ModelError::BadNodePlacement)
    );
    assert_eq!(
        fx.project.validate(&op(a, Node::step(" "), 1, 0)),
        Err(ModelError::EmptyName {
            kind: aegis_model::EntityKind::TreeNode
        })
    );
    fx.project.apply(op(a, Node::step("A"), 1, 0)).expect("add");
    assert_eq!(
        fx.project.validate(&op(a, Node::step("again"), 2, 1)),
        Err(ModelError::DuplicateId(a))
    );
    // A step cannot be a parent.
    let under_step = add(&fx, a, NodeId::new_v7(), Node::step("B"), 2, 0);
    assert!(matches!(
        fx.project.validate(&under_step),
        Err(ModelError::NotAGate(_))
    ));
    // A new gate has no children yet.
    let mut scratch = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "r")).expect("t");
    let some_idx = scratch
        .add_child(scratch.root(), NodeId::new_v7(), Node::step("x"))
        .expect("c");
    let busy = Node::Gate {
        kind: GateKind::And,
        label: "G".into(),
        children: vec![some_idx],
    };
    assert_eq!(
        fx.project.validate(&op(NodeId::new_v7(), busy, 2, 1)),
        Err(ModelError::ChildrenNotEmpty)
    );
}

#[test]
fn only_a_bare_leaf_can_be_removed() {
    let mut fx = fixture();
    let (g, s, t) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    fx.project
        .apply(add(&fx, fx.root, g, Node::gate(GateKind::And, "G"), 1, 0))
        .expect("g");
    fx.project
        .apply(add(&fx, g, s, Node::step("S"), 2, 0))
        .expect("s");
    fx.project
        .apply(add(&fx, fx.root, t, Node::step("T"), 3, 1))
        .expect("t");
    let remove = |id| Op::RemoveTreeNode {
        tree: fx.tree,
        id,
        provenance: who(3),
    };

    assert_eq!(
        fx.project.validate(&remove(fx.root)),
        Err(ModelError::CannotRemoveRoot)
    );
    assert_eq!(
        fx.project.validate(&remove(g)),
        Err(ModelError::NodeHasChildren(g))
    );
    assert!(
        fx.project
            .validate(&Op::RemoveTreeNode {
                tree: fx.tree,
                id: NodeId::new_v7(),
                provenance: who(3)
            })
            .is_err()
    );

    // A shared node: unlink first.
    fx.project
        .apply(Op::LinkTreeNode {
            tree: fx.tree,
            parent: fx.root,
            child: s,
            position: 2,
            slot: 0,
            provenance: who(4),
        })
        .expect("share S under the root too");
    assert_eq!(
        fx.project.validate(&remove(s)),
        Err(ModelError::NodeIsShared(s))
    );
    fx.project
        .apply(Op::UnlinkTreeNode {
            tree: fx.tree,
            parent: fx.root,
            child: s,
            provenance: who(5),
        })
        .expect("unlink");

    // A rated step: clear the rating first.
    let m = methodology();
    let low = m.feasibility_level("low").expect("level");
    fx.project
        .apply(Op::SetStepFeasibility {
            target: StepTarget::Step(fx.tree, s),
            level: Some(low),
            provenance: who(6),
        })
        .expect("rate");
    assert_eq!(
        fx.project.validate(&remove(s)),
        Err(ModelError::NodeIsRated(s))
    );
    fx.project
        .apply(Op::SetStepFeasibility {
            target: StepTarget::Step(fx.tree, s),
            level: None,
            provenance: who(7),
        })
        .expect("clear");
    let before = fx.project.clone();
    let undo = fx.project.apply(remove(s)).expect("now it can go");
    assert_eq!(nodes_of(&fx.project, fx.tree), 3);
    fx.project
        .apply(undo)
        .expect("undo puts it back in the same place");
    assert_eq!(fx.project, before);
}

#[test]
fn link_unlink_rename_and_gate_changes_are_undone_exactly() {
    let mut fx = fixture();
    let (g1, g2, s) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    fx.project
        .apply(add(&fx, fx.root, g1, Node::gate(GateKind::Or, "G1"), 1, 0))
        .expect("g1");
    fx.project
        .apply(add(&fx, fx.root, g2, Node::gate(GateKind::Or, "G2"), 2, 1))
        .expect("g2");
    fx.project
        .apply(add(&fx, g1, s, Node::step("S"), 3, 0))
        .expect("s");
    let base = fx.project.clone();

    let link = Op::LinkTreeNode {
        tree: fx.tree,
        parent: g2,
        child: s,
        position: 0,
        slot: 0,
        provenance: who(2),
    };
    let undo_link = fx.project.apply(link.clone()).expect("share");
    assert_eq!(
        fx.project.validate(&link),
        Err(ModelError::DuplicateEdge {
            parent: g2,
            child: s
        })
    );
    // A cycle is refused, as for `AttackTree::link`.
    let cycle = Op::LinkTreeNode {
        tree: fx.tree,
        parent: s,
        child: fx.root,
        position: 0,
        slot: 0,
        provenance: who(2),
    };
    assert!(fx.project.validate(&cycle).is_err());
    // The owner's own edge is not an extra link.
    let owner_edge = Op::UnlinkTreeNode {
        tree: fx.tree,
        parent: g1,
        child: s,
        provenance: who(2),
    };
    assert_eq!(
        fx.project.validate(&owner_edge),
        Err(ModelError::NotLinked {
            parent: g1,
            child: s
        })
    );
    fx.project.apply(undo_link).expect("undo link");
    assert_eq!(fx.project, base);

    let undo_name = fx
        .project
        .apply(Op::RenameTreeNode {
            tree: fx.tree,
            id: g1,
            label: "Renamed".into(),
            provenance: who(3),
        })
        .expect("rename");
    assert_eq!(
        fx.project.validate(&Op::RenameTreeNode {
            tree: fx.tree,
            id: g1,
            label: "  ".into(),
            provenance: who(3)
        }),
        Err(ModelError::EmptyName {
            kind: aegis_model::EntityKind::TreeNode
        })
    );
    let undo_gate = fx
        .project
        .apply(Op::SetTreeNodeGate {
            tree: fx.tree,
            id: g1,
            gate: GateKind::SeqAnd,
            provenance: who(4),
        })
        .expect("gate");
    assert!(matches!(
        fx.project.validate(&Op::SetTreeNodeGate {
            tree: fx.tree,
            id: s,
            gate: GateKind::And,
            provenance: who(4)
        }),
        Err(ModelError::NotAGate(_))
    ));
    fx.project.apply(undo_gate).expect("undo gate");
    fx.project.apply(undo_name).expect("undo name");
    assert_eq!(fx.project, base);
}

#[test]
fn the_evaluation_cache_agrees_with_a_fresh_one_after_every_structural_edit() {
    let mut fx = fixture();
    let m = methodology();
    let (low, high) = (
        m.feasibility_level("low").expect("l"),
        m.feasibility_level("high").expect("h"),
    );
    let mut cache = ProjectEvalCache::new(&fx.project);
    let mut ids = vec![fx.root];
    let mut step_ids = Vec::new();
    // A deterministic pseudo-random walk of node ops.
    let mut seed = 7u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for round in 0..120 {
        let tree_idx = fx.project.attack_tree_idx(&fx.tree).expect("tree");
        let len = fx.project.attack_tree(tree_idx).expect("t").len();
        let op = match next() % 4 {
            0 | 1 => {
                let id = NodeId::new_v7();
                let parents: Vec<NodeId> = ids.clone();
                let parent = parents[next() % parents.len()];
                let is_gate = next() % 3 == 0;
                let node = if is_gate {
                    Node::gate(GateKind::And, format!("g{round}"))
                } else {
                    Node::step(format!("s{round}"))
                };
                let tree = fx.project.attack_tree(tree_idx).expect("t");
                let p_idx = tree.idx_of(&parent).expect("p");
                if matches!(tree.get(p_idx), Some(Node::Step { .. })) {
                    continue;
                }
                let index = u32::try_from(len).expect("fits");
                let position = u32::try_from(tree.children(p_idx).len()).expect("fits");
                let op = add(&fx, parent, id, node, index, position);
                if fx.project.validate(&op).is_err() {
                    continue;
                }
                if !is_gate {
                    step_ids.push(id);
                }
                ids.push(id);
                op
            }
            2 if !step_ids.is_empty() => {
                let id = step_ids[next() % step_ids.len()];
                Op::SetStepFeasibility {
                    target: StepTarget::Step(fx.tree, id),
                    level: Some(if next() % 2 == 0 { low } else { high }),
                    provenance: who(5),
                }
            }
            _ => {
                let id = ids[next() % ids.len()];
                Op::RenameTreeNode {
                    tree: fx.tree,
                    id,
                    label: format!("n{round}").into(),
                    provenance: who(6),
                }
            }
        };
        fx.project.apply_cached(op, &mut cache).expect("applies");
        assert_eq!(cache, ProjectEvalCache::new(&fx.project), "round {round}");
        assert_eq!(
            fx.project.evaluate_risk(fx.threat).ok(),
            cache.evaluate_risk(&fx.project, fx.threat).ok().cloned(),
            "round {round}"
        );
    }
}

#[test]
fn trees_grow_to_the_node_cap_by_small_ops_and_not_one_node_more() {
    assert_eq!(MAX_ATTACK_TREE_NODES, 4_096);
    assert_eq!(MAX_TREE_OP_NODES, 32);
    let mut fx = fixture();
    for i in 1..MAX_ATTACK_TREE_NODES {
        let id = NodeId::new_v7();
        fx.project
            .apply(add(
                &fx,
                fx.root,
                id,
                Node::step(format!("step {i}")),
                u32::try_from(i).expect("fits"),
                u32::try_from(i - 1).expect("fits"),
            ))
            .expect("add");
    }
    assert_eq!(nodes_of(&fx.project, fx.tree), MAX_ATTACK_TREE_NODES);
    let one_more = add(
        &fx,
        fx.root,
        NodeId::new_v7(),
        Node::step("one more"),
        4_096,
        4_095,
    );
    assert!(matches!(
        fx.project.validate(&one_more),
        Err(ModelError::TooManyAttackTreeNodes { max: 4_096, .. })
    ));

    // Whole-tree ops stay small: a big tree cannot be added or removed in one op.
    assert!(matches!(
        fx.project.validate(&Op::RemoveAttackTree { id: fx.tree }),
        Err(ModelError::TreeTooLargeForOneOp { max: 32, .. })
    ));
    let mut big = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root")).expect("t");
    for i in 0..MAX_TREE_OP_NODES {
        big.add_child(big.root(), NodeId::new_v7(), Node::step(format!("s{i}")))
            .expect("c");
    }
    assert_eq!(big.len(), MAX_TREE_OP_NODES + 1);
    let add_big = Op::AddAttackTree {
        role: Default::default(),
        id: TreeId::new_v7(),
        tree: big,
        step_feasibility: vec![],
        provenance: Provenance::default(),
    };
    assert!(matches!(
        fx.project.validate(&add_big),
        Err(ModelError::TooManyAttackTreeNodes { max: 32, .. })
    ));
}

#[test]
fn a_full_tree_is_saved_loaded_and_logged() {
    let mut fx = fixture();
    let base = fx.project.clone();
    let m = methodology();
    let mut log = Vec::new();
    let mut writer = OpLogWriter::create(&mut log, Arc::clone(&m)).expect("writer");
    let mut applied = Vec::new();
    // A deep chain of gates and some shared steps, 1,000 nodes.
    let mut parent = fx.root;
    let mut gates = vec![fx.root];
    for i in 1..1_000usize {
        let id = NodeId::new_v7();
        let is_gate = i % 4 == 0;
        let node = if is_gate {
            Node::gate(GateKind::Or, format!("g{i}"))
        } else {
            Node::step(format!("s{i}"))
        };
        let tree_idx = fx.project.attack_tree_idx(&fx.tree).expect("t");
        let pos = u32::try_from(
            fx.project
                .attack_tree(tree_idx)
                .expect("t")
                .children(
                    fx.project
                        .attack_tree(tree_idx)
                        .expect("t")
                        .idx_of(&parent)
                        .expect("p"),
                )
                .len(),
        )
        .expect("fits");
        let op = add(&fx, parent, id, node, u32::try_from(i).expect("fits"), pos);
        fx.project.apply(op.clone()).expect("add");
        writer.append(&op).expect("logged");
        applied.push(op);
        if is_gate {
            parent = id;
            gates.push(id);
        }
    }
    // Share a few steps under other gates.
    for (n, g) in gates.iter().enumerate().skip(2).take(20) {
        let tree_idx = fx.project.attack_tree_idx(&fx.tree).expect("t");
        let tree = fx.project.attack_tree(tree_idx).expect("t");
        let step = tree
            .iter()
            .find(|(_, _, node)| node.label() == format!("s{}", n * 4 - 1))
            .map(|(_, id, _)| id);
        if let Some(step) = step {
            let pos = u32::try_from(tree.children(tree.idx_of(g).expect("g")).len()).expect("fits");
            let slots = u32::try_from(tree.parents(tree.idx_of(&step).expect("s")).count() - 1)
                .expect("fits");
            let link = Op::LinkTreeNode {
                tree: fx.tree,
                parent: *g,
                child: step,
                position: pos,
                slot: slots,
                provenance: who(9),
            };
            if fx.project.validate(&link).is_ok() {
                fx.project.apply(link.clone()).expect("link");
                writer.append(&link).expect("logged");
            }
        }
    }
    drop(writer);

    // Snapshot round trip, byte for byte.
    let mut bytes = Vec::new();
    fx.project.save(&mut bytes).expect("save");
    let loaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded, fx.project);
    let mut again = Vec::new();
    loaded.save(&mut again).expect("save");
    assert_eq!(again, bytes);

    // The op log replays to the same project, and every record is small.
    let longest = log
        .split(|b| *b == b'\n')
        .map(<[u8]>::len)
        .max()
        .unwrap_or(0);
    assert!(
        longest < aegis_model::MAX_OP_RECORD_BYTES / 4,
        "longest record {longest}"
    );
    let mut replay = base;
    for logged in OpLogReader::open(log.as_slice(), Arc::clone(&m)).expect("header") {
        replay.apply(logged.expect("record").op).expect("replays");
    }
    assert_eq!(replay, fx.project);
    let codec = OpCodec::new(Arc::clone(&m));
    let widest = Op::AddTreeNode {
        tree: fx.tree,
        parent: fx.root,
        id: NodeId::new_v7(),
        node: Node::step("é".repeat(256)),
        index: 4_095,
        position: 4_094,
        provenance: Provenance::human("a".repeat(250), u64::MAX),
    };
    assert!(codec.encode(&widest).expect("encodes").len() < aegis_model::MAX_OP_RECORD_BYTES / 4);
}
