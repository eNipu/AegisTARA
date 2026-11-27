//! Typed fragment edits and structure-preserving loaded-project undo.
#[path = "common/fragments.rs"]
mod fragments;

use aegis_client::commands::{Edit, EditError};
use aegis_client::relations::{RelationKey, candidates, related};
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{GateKind, NodeId, TreeId, TreeRole};
use fragments::{loaded, saved};

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "new-editor", 900).expect("ops");
    assert!(!ops.is_empty(), "{edit:?}");
    ws.apply(ops, Origin::Edit).expect("apply");
}

fn exact(ws: &mut Workspace, edit: &Edit) {
    let before = ws.project().clone();
    let before_bytes = saved(&before);
    apply(ws, edit);
    let after = ws.project().clone();
    let after_bytes = saved(&after);
    assert_ne!(after_bytes, before_bytes);
    ws.undo().expect("one undo");
    assert!(ws.project() == &before, "{edit:?}: project undo");
    assert_eq!(saved(ws.project()), before_bytes, "{edit:?}: bytes undo");
    ws.redo().expect("one redo");
    assert!(ws.project() == &after, "{edit:?}: project redo");
    assert_eq!(saved(ws.project()), after_bytes, "{edit:?}: bytes redo");
}

#[test]
fn typed_fragment_edits_have_exact_loaded_undo_and_provenance() {
    let (mut ws, i) = loaded();
    let new = TreeId::new_v7();
    let edit = Edit::NewFragment {
        id: new,
        root: "New fragment".into(),
    };
    assert_eq!(edit.created(), Some(Selection::Tree(new)));
    exact(&mut ws, &edit);
    let idx = ws.project().attack_tree_idx(&new).expect("new");
    assert_eq!(ws.project().attack_tree_role(idx), Some(TreeRole::Fragment));
    let t = ws.project().attack_tree(idx).expect("tree");
    assert_eq!(
        t.get(t.root()).expect("root").gate_kind(),
        Some(GateKind::Or)
    );
    for edit in [
        Edit::UseFragment {
            tree: i.first,
            node: i.ordinary,
            fragment: i.a,
        },
        Edit::UnuseFragment {
            tree: i.first,
            node: i.ordinary,
        },
        Edit::RenameFragment {
            tree: i.a,
            name: "Renamed A".into(),
        },
    ] {
        exact(&mut ws, &edit);
    }
    let no_change = Edit::RenameFragment {
        tree: i.a,
        name: " Renamed A ".into(),
    };
    assert!(
        no_change
            .to_ops(ws.project(), "new-editor", 901)
            .expect("unchanged")
            .is_empty()
    );
    assert!(
        matches!(Edit::Batch(vec![no_change]).changing(ws.project()), Edit::Batch(v) if v.is_empty())
    );
    assert_eq!(
        Edit::RenameFragment {
            tree: i.first,
            name: "Wrong role".into()
        }
        .to_ops(ws.project(), "new-editor", 901),
        Err(EditError::Missing),
    );
}

#[test]
fn loaded_reference_structure_edits_restore_bytes_and_links() {
    for case in 0..7 {
        let (mut ws, i) = loaded();
        let child = NodeId::new_v7();
        let edit = match case {
            0 => Edit::RemoveTreeNode {
                tree: i.first,
                node: i.user_a,
            },
            1 => Edit::RemoveTree { tree: i.first },
            2 => Edit::ReplaceTree {
                tree: i.first,
                outline: "OR First\n  Ordinary\n".into(),
            },
            3 => Edit::BranchStepToGate {
                tree: i.first,
                node: i.user_a,
                id: child,
                gate: GateKind::And,
                child_label: "Reference child".into(),
            },
            4 => Edit::MoveTreeNode {
                tree: i.first,
                node: i.user_a,
                delta: -1,
            },
            5 => Edit::RemoveTreeNode {
                tree: i.first,
                node: i.ordinary,
            },
            _ => Edit::InsertParentGate {
                tree: i.first,
                node: i.user_a,
                id: child,
                gate: GateKind::And,
                label: "Parent".into(),
            },
        };
        exact(&mut ws, &edit);
        if case >= 3 {
            let node = if case == 3 { child } else { i.user_a };
            let t = ws.project().attack_tree_idx(&i.first).expect("tree");
            let n = ws
                .project()
                .attack_tree(t)
                .expect("tree")
                .idx_of(&node)
                .expect("node");
            assert_eq!(ws.project().step_fragment_id(t, n), Some(i.a));
            assert_eq!(ws.project().step_condition_link_ids(i.first, node).len(), 1);
            assert_eq!(ws.project().step_mitigations(t, n).len(), 1);
            assert!(ws.project().review_marked(i.first, node));
        }
    }
}

#[test]
fn protected_deletion_and_invalid_attachment_are_atomic() {
    let (mut ws, i) = loaded();
    for edit in [
        Edit::RemoveTree { tree: i.a },
        Edit::UseFragment {
            tree: i.first,
            node: i.shared,
            fragment: i.a,
        },
        Edit::UseFragment {
            tree: i.first,
            node: i.ordinary,
            fragment: i.first,
        },
    ] {
        let before = ws.project().clone();
        let bytes = saved(&before);
        let ops = edit
            .to_ops(ws.project(), "new-editor", 900)
            .expect("typed ops");
        assert!(ws.apply(ops, Origin::Edit).is_err(), "{edit:?}");
        assert!(ws.project() == &before);
        assert_eq!(saved(ws.project()), bytes);
        assert!(!ws.can_undo());
    }
    let level = ws
        .project()
        .methodology()
        .feasibility_level("low")
        .expect("low");
    ws.apply(
        vec![aegis_model::Op::SetStepFeasibility {
            target: aegis_model::StepTarget::Step(i.first, i.ordinary),
            level: Some(level),
            provenance: aegis_model::Provenance::human("rater", 71),
        }],
        Origin::Edit,
    )
    .expect("rate");
    let before = saved(ws.project());
    let ops = Edit::UseFragment {
        tree: i.first,
        node: i.ordinary,
        fragment: i.a,
    }
    .to_ops(ws.project(), "new-editor", 900)
    .expect("ops");
    assert_eq!(ops.len(), 1, "attachment never clears ratings");
    assert!(ws.apply(ops, Origin::Edit).is_err());
    assert_eq!(saved(ws.project()), before);
}

#[test]
fn direct_users_and_threat_candidates_are_independently_named() {
    let (ws, i) = loaded();
    let mut expected = [(i.first, i.user_a), (i.second, i.user_b), (i.b, i.user_c)];
    expected.sort();
    assert_eq!(
        related(
            ws.project(),
            RelationKey::FRAGMENT_USED_IN,
            Selection::Tree(i.a)
        ),
        expected
            .into_iter()
            .map(|(t, n)| Selection::TreeNode(t, n))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        candidates(ws.project(), RelationKey::THREAT_TREE),
        vec![Selection::Tree(i.first), Selection::Tree(i.second)]
    );
    for tree in [i.unused, i.first, TreeId::new_v7()] {
        assert!(
            related(
                ws.project(),
                RelationKey::FRAGMENT_USED_IN,
                Selection::Tree(tree)
            )
            .is_empty()
        );
    }
}
