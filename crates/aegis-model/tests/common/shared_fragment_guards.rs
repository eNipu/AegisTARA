//! Private setup only: no production API can install a fragment reference.
#![allow(clippy::pedantic)]

use super::*;
use crate::testkit::{builtin, node_id, tree_id};

fn provenance() -> Provenance {
    Provenance::human("alice", 1)
}

fn add(p: &mut Project, n: u64, role: TreeRole, steps: usize) {
    let tree =
        AttackTree::new(node_id(n * 1000), Node::gate(crate::GateKind::Or, "Root")).expect("root");
    p.apply(Op::AddAttackTree {
        id: tree_id(n),
        role,
        tree,
        step_feasibility: vec![],
        provenance: provenance(),
    })
    .expect("add");
    // One op may carry only a few nodes, so steps are added one at a time.
    for i in 1..=steps {
        p.apply(Op::AddTreeNode {
            tree: tree_id(n),
            parent: node_id(n * 1000),
            id: node_id(n * 1000 + i as u64),
            node: Node::step("Step"),
            index: i as u32,
            position: i as u32 - 1,
            provenance: provenance(),
        })
        .expect("step");
    }
}

fn project(count: u64) -> Project {
    let mut p = Project::new(builtin());
    for n in 1..=count {
        add(
            &mut p,
            n,
            if n == 1 {
                TreeRole::Threat
            } else {
                TreeRole::Fragment
            },
            2,
        );
    }
    p
}

fn check(p: &Project, source: u64, node: u64, target: u64) -> Result<(), ModelError> {
    p.check_use_fragment(
        tree_id(source),
        node_id(source * 1000 + node),
        tree_id(target),
        &provenance(),
    )
}

fn install(p: &mut Project, source: u64, node: u64, target: u64) {
    let target = p.attack_tree_idx(&tree_id(target)).expect("target");
    let (_, at) = p
        .tree_node(tree_id(source), node_id(source * 1000 + node))
        .expect("node");
    p.tree_mut(tree_id(source))
        .expect("source")
        .set_fragment(at.index(), Some(target));
}

fn clear(p: &mut Project, source: u64, node: u64) {
    let (_, at) = p
        .tree_node(tree_id(source), node_id(source * 1000 + node))
        .expect("node");
    p.tree_mut(tree_id(source))
        .expect("source")
        .set_fragment(at.index(), None);
}

fn order(p: &Project) -> Vec<TreeId> {
    p.fragment_order()
        .expect("order")
        .into_iter()
        .map(|i| p.attack_tree_id(i).expect("id"))
        .collect()
}

#[test]
fn diamonds_parallel_edges_and_read_only_candidates() {
    let mut p = project(4);
    for (s, n, t) in [(1, 1, 2), (1, 2, 3), (2, 1, 4), (2, 2, 4), (3, 1, 4)] {
        let before = p.clone();
        assert_eq!(check(&p, s, n, t), Ok(()));
        assert_eq!(p, before);
        install(&mut p, s, n, t);
    }
    assert_eq!(order(&p), [tree_id(4), tree_id(2), tree_id(3), tree_id(1)]);
    let f = p.attack_tree_idx(&tree_id(4)).expect("fragment");
    assert_eq!(
        p.fragment_users(f),
        [
            StepRef {
                tree: tree_id(2),
                node: node_id(2001)
            },
            StepRef {
                tree: tree_id(2),
                node: node_id(2002)
            },
            StepRef {
                tree: tree_id(3),
                node: node_id(3001)
            },
        ]
    );
    p.check_invariants().expect("invariants");
}

fn assert_cycle(p: &Project, source: u64, target: u64) {
    let before = p.clone();
    let Err(ModelError::FragmentCycle(cycle)) = check(p, source, 1, target) else {
        panic!("expected cycle");
    };
    assert_eq!(p, &before);
    assert_eq!(cycle.first(), cycle.last());
    assert_eq!(cycle.first(), cycle.iter().min());
    for edge in cycle.windows(2) {
        let real = p
            .trees
            .iter()
            .find(|(_, id, _)| *id == edge[0])
            .expect("source")
            .2;
        let exists = real.tree.iter().any(|(at, _, _)| {
            real.fragment_at(at.index())
                .and_then(|t| p.attack_tree_id(t))
                == Some(edge[1])
        });
        assert!(
            exists || edge == [tree_id(source), tree_id(target)],
            "invalid edge {edge:?}"
        );
    }
}

#[test]
fn self_cross_and_residual_cycles_report_real_edges() {
    let mut p = project(5);
    assert_cycle(&p, 2, 2);
    install(&mut p, 2, 1, 3);
    install(&mut p, 3, 1, 4);
    install(&mut p, 1, 1, 2); // upstream residual node is not itself in the cycle
    assert_cycle(&p, 4, 2);
    install(&mut p, 4, 1, 2);
    let Err(ModelError::FragmentCycle(cycle)) = p.fragment_order() else {
        panic!("cycle");
    };
    assert_eq!(cycle, [tree_id(2), tree_id(3), tree_id(4), tree_id(2)]);
}

#[test]
fn depth_includes_threat_entry_and_upstream_users() {
    let mut p = project(10);
    for n in 1..8 {
        install(&mut p, n, 1, n + 1);
    }
    assert_eq!(check(&p, 8, 1, 9), Ok(())); // eight edges from Threat
    install(&mut p, 8, 1, 9);
    let before = p.clone();
    assert_eq!(
        check(&p, 9, 1, 10),
        Err(ModelError::FragmentDepth { max: 8 })
    );
    assert_eq!(p, before);
    assert_eq!(p.fragment_order().expect("depth 8").len(), 10);
    install(&mut p, 9, 1, 10);
    assert_eq!(
        p.fragment_order(),
        Err(ModelError::FragmentDepth { max: 8 })
    );
}

#[test]
fn cap_counts_source_nodes_not_distinct_edges() {
    let mut p = Project::new(builtin());
    add(&mut p, 1, TreeRole::Threat, 257);
    add(&mut p, 2, TreeRole::Fragment, 1);
    for n in 1..=256 {
        assert_eq!(check(&p, 1, n, 2), Ok(()));
        install(&mut p, 1, n, 2);
    }
    p.check_invariants().expect("256");
    let before = p.clone();
    assert_eq!(
        check(&p, 1, 257, 2),
        Err(ModelError::TooManyFragmentReferences {
            tree: tree_id(1),
            max: 256
        })
    );
    assert_eq!(p, before);
    clear(&mut p, 1, 1);
    assert_eq!(check(&p, 1, 257, 2), Ok(()));
}

#[test]
fn roles_gates_ratings_shared_slots_and_provenance() {
    let mut p = project(3);
    assert_eq!(
        check(&p, 2, 1, 1),
        Err(ModelError::FragmentRole(tree_id(1)))
    );
    assert_eq!(
        check(&p, 1, 0, 2),
        Err(ModelError::NotAStep {
            tree: tree_id(1),
            node: node_id(1000)
        })
    );
    assert_eq!(
        p.check_unuse_fragment(tree_id(1), node_id(1001), &provenance()),
        Err(ModelError::NodeNotFragment {
            tree: tree_id(1),
            node: node_id(1001)
        })
    );
    assert_eq!(
        p.check_use_fragment(
            tree_id(1),
            node_id(1001),
            tree_id(2),
            &Provenance::human(" ", 1)
        ),
        Err(ModelError::EmptyActor)
    );
    let level = p.methodology.feasibility_levels().next().expect("level").0;
    p.tree_mut(tree_id(1)).expect("tree").step_feasibility[1] = Some(level);
    assert_eq!(
        check(&p, 1, 1, 2),
        Err(ModelError::StepIsRated {
            tree: tree_id(1),
            node: node_id(1001)
        })
    );
    p.tree_mut(tree_id(1)).expect("tree").step_feasibility[1] = None;
    let shared = SharedStepId::new_v7();
    p.apply(Op::AddSharedStep {
        id: shared,
        name: "Shared".into(),
        provenance: provenance(),
    })
    .expect("shared");
    p.apply(Op::UseSharedStep {
        tree: tree_id(1),
        node: node_id(1001),
        step: shared,
        provenance: provenance(),
    })
    .expect("use");
    assert_eq!(
        check(&p, 1, 1, 2),
        Err(ModelError::NodeUsesSharedStep {
            tree: tree_id(1),
            node: node_id(1001),
            shared_step: shared
        })
    );
    p.apply(Op::UnuseSharedStep {
        tree: tree_id(1),
        node: node_id(1001),
        provenance: provenance(),
    })
    .expect("unuse");
    install(&mut p, 1, 1, 2);
    let error = ModelError::NodeUsesFragment {
        tree: tree_id(1),
        node: node_id(1001),
        fragment: tree_id(2),
    };
    assert_eq!(check(&p, 1, 1, 3), Err(error.clone()));
    assert_eq!(
        p.check_set_step_feasibility(tree_id(1), node_id(1001), None),
        Err(error.clone())
    );
    assert_eq!(
        p.check_set_step_via(tree_id(1), node_id(1001), None),
        Err(error.clone())
    );
    assert_eq!(
        p.check_set_step_attack_type(tree_id(1), node_id(1001), None),
        Err(error.clone())
    );
    let factor = p.methodology.feasibility_factors().next().expect("factor");
    assert_eq!(
        p.check_set_step_factor(tree_id(1), node_id(1001), factor, None),
        Err(error.clone())
    );
    assert_eq!(
        p.check_use_shared_step(tree_id(1), node_id(1001), shared, &provenance()),
        Err(error)
    );
    assert_eq!(
        p.check_unuse_fragment(tree_id(1), node_id(1001), &Provenance::human(" ", 1)),
        Err(ModelError::EmptyActor)
    );
    assert_eq!(
        p.check_unuse_fragment(tree_id(1), node_id(1001), &provenance()),
        Ok(())
    );
    p.check_invariants().expect("valid reference");
}

#[test]
fn incoming_and_outgoing_removal_are_blocked_until_unlinked() {
    let mut p = project(2);
    install(&mut p, 1, 1, 2);
    let before = p.clone();
    assert_eq!(
        p.apply(Op::RemoveAttackTree { id: tree_id(2) }),
        Err(ModelError::FragmentInUse {
            fragment: tree_id(2),
            users: 1
        })
    );
    let error = ModelError::NodeUsesFragment {
        tree: tree_id(1),
        node: node_id(1001),
        fragment: tree_id(2),
    };
    assert_eq!(
        p.apply(Op::RemoveAttackTree { id: tree_id(1) }),
        Err(error.clone())
    );
    assert_eq!(
        p.check_remove_tree_node(tree_id(1), node_id(1001), &provenance()),
        Err(error)
    );
    assert_eq!(p, before);
    clear(&mut p, 1, 1);
    assert!(p.tree_mut(tree_id(1)).expect("tree").step_links.is_none());
    assert_eq!(
        p.check_remove_tree_node(tree_id(1), node_id(1001), &provenance()),
        Ok(())
    );
    p.apply(Op::RemoveAttackTree { id: tree_id(2) })
        .expect("remove target");
    p.apply(Op::RemoveAttackTree { id: tree_id(1) })
        .expect("remove source");
}

#[test]
fn stable_ids_survive_different_arena_layouts_and_node_slot_movement() {
    let mut a = project(2);
    let mut b = Project::new(builtin());
    add(&mut b, 9, TreeRole::Fragment, 1);
    b.apply(Op::RemoveAttackTree { id: tree_id(9) })
        .expect("tombstone");
    add(&mut b, 2, TreeRole::Fragment, 2);
    add(&mut b, 1, TreeRole::Threat, 2);
    install(&mut a, 1, 2, 2);
    install(&mut b, 1, 2, 2);
    assert_eq!(a, b);
    assert!(
        a.diff(&b)
            .expect("diff")
            .attack_trees()
            .modified()
            .is_empty()
    );
    let tree = a.attack_tree_idx(&tree_id(1)).expect("tree");
    let at = a
        .attack_tree(tree)
        .expect("entry")
        .idx_of(&node_id(1002))
        .expect("node");
    assert_eq!(a.step_fragment_id(tree, at), Some(tree_id(2)));
    let entry = a.tree_mut(tree_id(1)).expect("tree");
    entry.insert_links(1);
    assert_eq!(
        entry.fragment_at(3),
        b.attack_tree_idx(&tree_id(2))
            .map(|_| a.attack_tree_idx(&tree_id(2)).expect("target"))
    );
    a.tree_mut(tree_id(1)).expect("tree").remove_links(1);
    assert_eq!(a, b);
}
