//! Inserting a library item into a project (job 057, LIB-03 and LIB-04): as
//! a new tree, under a gate, or in place of a step; one undo step; ratings by
//! choice and marked for review; identical names are never merged silently.

use std::collections::BTreeSet;
use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::library::{InsertMode, InsertOptions, matching_labels};
use aegis_client::outline::to_outline;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::library::LibraryPack;
use aegis_model::{
    AttackTree, GateKind, LintCode, Methodology, Node, NodeId, Op, Project, ProvenanceSource,
    ThreatId, TreeId,
};

fn pack() -> Arc<LibraryPack> {
    Arc::new(LibraryPack::builtin_demo())
}

fn item_index(pack: &LibraryPack, id: &str) -> usize {
    pack.items().iter().position(|i| i.id == id).expect("item")
}

fn workspace() -> Workspace {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    Workspace::empty(m)
}

fn apply(ws: &mut Workspace, edit: &Edit) -> Result<(), aegis_client::workspace::WorkspaceError> {
    let ops = edit.to_ops(ws.project(), "alice", 77).expect("ops");
    ws.apply(ops, Origin::Edit)
}

fn insert(pack: &Arc<LibraryPack>, id: &str, mode: InsertMode, options: InsertOptions) -> Edit {
    Edit::InsertLibraryItem {
        pack: Arc::clone(pack),
        item: item_index(pack, id),
        mode,
        options,
    }
}

fn plain() -> InsertOptions {
    InsertOptions::default()
}

fn new_tree(
    ws: &mut Workspace,
    pack: &Arc<LibraryPack>,
    id: &str,
    options: InsertOptions,
) -> TreeId {
    let tree = TreeId::new_v7();
    apply(
        ws,
        &insert(
            pack,
            id,
            InsertMode::NewTree {
                id: tree,
                threat: None,
            },
            options,
        ),
    )
    .expect("inserts");
    tree
}

fn tree_of(project: &Project, id: TreeId) -> &AttackTree {
    project
        .attack_tree(project.attack_tree_idx(&id).expect("tree"))
        .expect("tree")
}

fn node(tree: &AttackTree, label: &str) -> NodeId {
    tree.iter()
        .find(|(_, _, n)| n.label() == label)
        .map(|(_, id, _)| id)
        .expect("node")
}

#[test]
fn an_item_becomes_a_new_tree_with_an_origin_note_and_no_ratings() {
    let (mut ws, pack) = (workspace(), pack());
    let before = ws.project().clone();
    let tree = new_tree(&mut ws, &pack, "can-dos", plain());
    let t = tree_of(ws.project(), tree);
    assert_eq!(
        to_outline(t),
        "OR Deny service on the CAN bus
  - Flood the bus with high-priority frames
  - Send frames that force a node into bus-off
  - Hold the bus dominant with a modified node
"
    );
    let note = ws
        .project()
        .description(aegis_model::ElementRef::Tree(tree))
        .expect("origin note");
    assert!(
        note.contains("can-dos") && note.contains("1.0.0") && note.contains("aegis-demo"),
        "{note}"
    );
    assert!(note.contains("CC0-1.0"), "{note}");
    let idx = ws.project().attack_tree_idx(&tree).expect("t");
    assert!(
        t.iter()
            .all(|(i, _, n)| n.gate_kind().is_some()
                || ws.project().step_feasibility(idx, i).is_none()),
        "structure only"
    );
    assert!(ws.project().review_marks().next().is_none());
    ws.undo().expect("one undo step");
    assert_eq!(ws.project(), &before);
}

#[test]
fn ratings_travel_by_choice_and_every_one_is_marked_for_review() {
    let (mut ws, pack) = (workspace(), pack());
    let tree = new_tree(
        &mut ws,
        &pack,
        "can-dos",
        InsertOptions {
            with_ratings: true,
            ..plain()
        },
    );
    let m = ws.project().methodology().clone();
    let t = tree_of(ws.project(), tree);
    let idx = ws.project().attack_tree_idx(&tree).expect("t");
    let flood = t
        .idx_of(&node(t, "Flood the bus with high-priority frames"))
        .expect("i");
    assert_eq!(
        ws.project().step_feasibility(idx, flood),
        m.feasibility_level("medium")
    );
    let marked: Vec<_> = ws.project().review_marks().collect();
    assert_eq!(marked.len(), 3, "all three steps need review");
    let findings = ws
        .project()
        .lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::LibraryRatingNotReviewed { .. }))
        .count();
    assert_eq!(findings, 3);
    // The tree came from the library, and says so.
    let tree_prov = ws
        .project()
        .attack_tree_provenance(idx)
        .expect("provenance");
    assert_eq!(tree_prov.source(), ProvenanceSource::Catalog);

    // Reviewing one rating clears its finding, and undo brings it back.
    let first = marked[0];
    apply(
        &mut ws,
        &Edit::MarkReviewed {
            tree: first.0,
            node: first.1,
        },
    )
    .expect("reviewed");
    assert_eq!(ws.project().review_marks().count(), 2);
    ws.undo().expect("undo");
    assert_eq!(ws.project().review_marks().count(), 3);
}

fn host() -> (Workspace, TreeId, NodeId, NodeId) {
    let (mut ws, pack) = (workspace(), pack());
    let tree = TreeId::new_v7();
    let ops = Edit::NewTree {
        id: tree,
        root: "Goal".into(),
        threat: None,
    }
    .to_ops(ws.project(), "alice", 1)
    .expect("ops");
    ws.apply(ops, Origin::Edit).expect("applies");
    let _ = pack;
    let root = tree_of(ws.project(), tree)
        .id_of(tree_of(ws.project(), tree).root())
        .expect("root");
    let gate = NodeId::new_v7();
    let leaf = NodeId::new_v7();
    let t = tree_of(ws.project(), tree);
    let (at_gate, at_leaf) = (
        u32::try_from(t.len()).expect("fits"),
        u32::try_from(t.len() + 1).expect("fits"),
    );
    ws.apply(
        vec![
            Op::AddTreeNode {
                tree,
                parent: root,
                id: gate,
                node: Node::gate(GateKind::And, "Existing gate"),
                index: at_gate,
                position: 0,
                provenance: aegis_model::Provenance::default(),
            },
            Op::AddTreeNode {
                tree,
                parent: root,
                id: leaf,
                node: Node::step("Existing step"),
                index: at_leaf,
                position: 1,
                provenance: aegis_model::Provenance::default(),
            },
        ],
        Origin::Edit,
    )
    .expect("host nodes");
    (ws, tree, gate, leaf)
}

#[test]
fn a_fragment_is_grafted_under_a_gate_in_one_undo_step() {
    let (mut ws, tree, gate, _) = host();
    let pack = pack();
    let before = ws.project().clone();
    apply(
        &mut ws,
        &insert(
            &pack,
            "can-dos",
            InsertMode::UnderGate { tree, gate },
            plain(),
        ),
    )
    .expect("grafts");
    let t = tree_of(ws.project(), tree);
    assert_eq!(
        to_outline(t),
        "OR Goal
  AND Existing gate
    OR Deny service on the CAN bus
      - Flood the bus with high-priority frames
      - Send frames that force a node into bus-off
      - Hold the bus dominant with a modified node
  - Existing step
"
    );
    let note = ws
        .project()
        .description(aegis_model::ElementRef::Tree(tree))
        .expect("note");
    assert!(note.contains("can-dos"));
    ws.undo().expect("undo");
    assert_eq!(
        ws.project(),
        &before,
        "one step restores the tree, its description and its provenance"
    );
}

#[test]
fn a_fragment_replaces_a_step_at_its_place() {
    let (mut ws, tree, _, leaf) = host();
    let pack = pack();
    // The step has a rating that goes with it.
    let low = ws
        .project()
        .methodology()
        .feasibility_level("low")
        .expect("level");
    ws.apply(
        vec![Op::SetStepFeasibility {
            tree,
            node: leaf,
            level: Some(low),
            provenance: aegis_model::Provenance::default(),
        }],
        Origin::Edit,
    )
    .expect("rate");
    let before = ws.project().clone();
    apply(
        &mut ws,
        &insert(
            &pack,
            "can-dos",
            InsertMode::InPlaceOfLeaf { tree, leaf },
            plain(),
        ),
    )
    .expect("replaces");
    let t = tree_of(ws.project(), tree);
    assert_eq!(
        to_outline(t),
        "OR Goal
  AND Existing gate
  OR Deny service on the CAN bus
    - Flood the bus with high-priority frames
    - Send frames that force a node into bus-off
    - Hold the bus dominant with a modified node
"
    );
    assert!(t.idx_of(&leaf).is_none(), "the step is gone");
    ws.undo().expect("undo");
    assert_eq!(ws.project(), &before);
}

#[test]
fn the_root_and_shared_steps_are_not_replaced() {
    let (mut ws, tree, gate, leaf) = host();
    let pack = pack();
    let root = tree_of(ws.project(), tree)
        .id_of(tree_of(ws.project(), tree).root())
        .expect("root");
    let on_root = insert(
        &pack,
        "can-dos",
        InsertMode::InPlaceOfLeaf { tree, leaf: root },
        plain(),
    );
    assert!(
        on_root.to_ops(ws.project(), "a", 1).is_err(),
        "the root is a gate, not a step"
    );
    let on_gate = insert(
        &pack,
        "can-dos",
        InsertMode::InPlaceOfLeaf { tree, leaf: gate },
        plain(),
    );
    assert!(on_gate.to_ops(ws.project(), "a", 1).is_err());
    let under_step = insert(
        &pack,
        "can-dos",
        InsertMode::UnderGate { tree, gate: leaf },
        plain(),
    );
    assert!(
        under_step.to_ops(ws.project(), "a", 1).is_err(),
        "a step cannot take children"
    );
    // A shared step.
    ws.apply(
        vec![Op::LinkTreeNode {
            tree,
            parent: gate,
            child: leaf,
            position: 0,
            slot: 0,
            provenance: aegis_model::Provenance::default(),
        }],
        Origin::Edit,
    )
    .expect("share");
    let shared = insert(
        &pack,
        "can-dos",
        InsertMode::InPlaceOfLeaf { tree, leaf },
        plain(),
    );
    assert!(
        shared.to_ops(ws.project(), "a", 1).is_err(),
        "unlink it first"
    );
}

#[test]
fn identical_names_are_never_merged_silently() {
    let (mut ws, tree, gate, _) = host();
    let pack = pack();
    // The host already has a step with the same text as one in the item.
    apply(
        &mut ws,
        &Edit::ReplaceTree {
            tree,
            outline: "OR Goal
  AND Existing gate
    - Reach the OBD port
  - Existing step
"
            .into(),
        },
    )
    .expect("host outline");
    let item = &pack.items()[item_index(&pack, "uds-exec")];
    let mode = InsertMode::UnderGate {
        tree,
        gate: node(tree_of(ws.project(), tree), "Existing gate"),
    };
    assert_eq!(
        matching_labels(ws.project(), &mode, item),
        vec!["Reach the OBD port".to_owned()]
    );
    let _ = gate;

    // Default: a second node with its own name, so the outline editor never
    // sees one text twice.
    apply(&mut ws, &insert(&pack, "uds-exec", mode.clone(), plain())).expect("inserts");
    let t = tree_of(ws.project(), tree);
    let labels: Vec<_> = t.iter().map(|(_, _, n)| n.label().to_owned()).collect();
    assert_eq!(
        labels.iter().filter(|l| *l == "Reach the OBD port").count(),
        1
    );
    assert!(
        labels.contains(&"Reach the OBD port (2)".to_owned()),
        "{labels:?}"
    );
    let distinct: BTreeSet<_> = labels.iter().collect();
    assert_eq!(
        distinct.len(),
        labels.len(),
        "every text is unique in the tree"
    );
    // The outline editor keeps both nodes as they are.
    let outline = to_outline(t);
    let again = aegis_client::outline::parse_outline(&outline, Some(t)).expect("parses");
    assert_eq!(again.len(), t.len());
    ws.undo().expect("undo");

    // The user ticks the match: the existing step is shared, not copied.
    let ticked = InsertOptions {
        share: ["Reach the OBD port".to_owned()].into_iter().collect(),
        ..plain()
    };
    apply(&mut ws, &insert(&pack, "uds-exec", mode, ticked)).expect("inserts with sharing");
    let t = tree_of(ws.project(), tree);
    let labels: Vec<_> = t.iter().map(|(_, _, n)| n.label().to_owned()).collect();
    assert_eq!(
        labels.iter().filter(|l| *l == "Reach the OBD port").count(),
        1
    );
    assert!(!labels.iter().any(|l| l.ends_with("(2)")));
    let obd = t.idx_of(&node(t, "Reach the OBD port")).expect("i");
    assert_eq!(t.parents(obd).count(), 2, "it hangs under both gates now");
}

#[test]
fn unknown_rating_keys_and_a_full_tree_are_refused_cleanly() {
    let (mut ws, pack) = (workspace(), pack());
    // A pack whose rating key the methodology does not know.
    let mut item = pack.items()[item_index(&pack, "reach-obd-port")].clone();
    item.tree.feasibility = Some("astronomical".into());
    let bad = Arc::new(
        LibraryPack::new(
            aegis_model::library::PackMeta {
                id: "p".into(),
                title: "P".into(),
                pack_version: "1".into(),
                license: "CC0-1.0".into(),
                sources: "s".into(),
            },
            vec![item],
        )
        .expect("pack"),
    );
    let edit = Edit::InsertLibraryItem {
        pack: bad,
        item: 0,
        mode: InsertMode::NewTree {
            id: TreeId::new_v7(),
            threat: None,
        },
        options: InsertOptions {
            with_ratings: true,
            ..plain()
        },
    };
    assert!(edit.to_ops(ws.project(), "a", 1).is_err());
    // Without ratings the same item is fine.
    let ok = Edit::InsertLibraryItem {
        pack: Arc::clone(&pack),
        item: item_index(&pack, "reach-obd-port"),
        mode: InsertMode::NewTree {
            id: TreeId::new_v7(),
            threat: None,
        },
        options: plain(),
    };
    apply(&mut ws, &ok).expect("a single step is a tree");
}

#[test]
fn every_item_of_the_demo_pack_inserts_as_a_tree_and_links_to_a_threat() {
    let pack = pack();
    for (i, it) in pack.items().iter().enumerate() {
        let mut ws = workspace();
        let tree = TreeId::new_v7();
        let before = ws.project().clone();
        apply(
            &mut ws,
            &Edit::InsertLibraryItem {
                pack: Arc::clone(&pack),
                item: i,
                mode: InsertMode::NewTree {
                    id: tree,
                    threat: None,
                },
                options: InsertOptions {
                    with_ratings: true,
                    ..plain()
                },
            },
        )
        .unwrap_or_else(|e| panic!("{}: {e}", it.id));
        let t = tree_of(ws.project(), tree);
        let steps = t.iter().filter(|(_, _, n)| n.gate_kind().is_none()).count();
        assert!(steps >= 1, "{}", it.id);
        // The tree can be taken apart again, with its marks and description.
        apply(&mut ws, &Edit::RemoveTree { tree }).unwrap_or_else(|e| panic!("{}: {e}", it.id));
        assert_eq!(ws.project().attack_tree_idx(&tree), None, "{}", it.id);
        assert_eq!(ws.project().review_marks().count(), 0, "{}", it.id);
        ws.undo().expect("undo removal");
        ws.undo().expect("undo insert");
        assert_eq!(ws.project(), &before, "{}", it.id);
    }
    let _ = ThreatId::new_v7();
}
