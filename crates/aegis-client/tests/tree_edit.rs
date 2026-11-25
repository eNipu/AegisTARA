//! The outline editor on node-level ops (job 065): an outline edit sends
//! only what changed, keeps the nodes that did not, and is one undo step.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::outline::{parse_outline, to_outline};
use aegis_client::workspace::{MAX_SUBMIT_OPS, Origin, Workspace, WorkspaceError};
use aegis_model::{AttackTree, Methodology, NodeId, Op, Project, StepTarget, TreeId};

const START: &str = "OR Goal
  SEQ Chain
    - Step one
    - Step two
  - Step three
";

fn workspace_with(outline: &str) -> (Workspace, TreeId) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let tree_id = TreeId::new_v7();
    let ops = Edit::NewTree {
        id: tree_id,
        root: "Goal".into(),
        threat: None,
    }
    .to_ops(ws.project(), "alice", 1)
    .expect("ops");
    ws.apply(ops, Origin::Edit).expect("applies");
    replace(&mut ws, tree_id, outline).expect("outline applies");
    (ws, tree_id)
}

fn replace(ws: &mut Workspace, tree: TreeId, outline: &str) -> Result<Vec<Op>, WorkspaceError> {
    let ops = Edit::ReplaceTree {
        tree,
        outline: outline.to_owned(),
    }
    .to_ops(ws.project(), "alice", 2)
    .expect("ops");
    ws.apply(ops.clone(), Origin::Edit)?;
    Ok(ops)
}

fn tree_of(project: &Project, id: TreeId) -> &AttackTree {
    let idx = project.attack_tree_idx(&id).expect("tree");
    project.attack_tree(idx).expect("tree")
}

fn id_of(tree: &AttackTree, label: &str) -> NodeId {
    tree.iter()
        .find(|(_, _, n)| n.label() == label)
        .map(|(_, id, _)| id)
        .expect("node")
}

#[test]
fn an_edit_sends_only_what_changed_and_keeps_the_rest() {
    let (mut ws, tree) = workspace_with(START);
    assert_eq!(to_outline(tree_of(ws.project(), tree)), START);
    let before = tree_of(ws.project(), tree).clone();
    let kept = id_of(&before, "Step one");

    let ops = replace(
        &mut ws,
        tree,
        "OR Goal
  SEQ Chain
    - Step one
    - Step two
    - Step four
  - Step three
",
    )
    .expect("applies");
    assert_eq!(ops.len(), 1, "{ops:?}");
    assert!(matches!(&ops[0], Op::AddTreeNode { node, .. } if node.label() == "Step four"));
    let after = tree_of(ws.project(), tree);
    assert_eq!(
        id_of(after, "Step one"),
        kept,
        "unchanged nodes keep their identity"
    );

    // One undo step restores the old tree exactly.
    ws.undo().expect("undo");
    assert_eq!(tree_of(ws.project(), tree), &before);
}

#[test]
fn removed_steps_lose_their_ratings_and_kept_steps_keep_them() {
    let (mut ws, tree) = workspace_with(START);
    let t = tree_of(ws.project(), tree).clone();
    let m = ws.project().methodology().clone();
    let low = m.feasibility_level("low").expect("level");
    for label in ["Step one", "Step three"] {
        ws.apply(
            vec![Op::SetStepFeasibility {
                target: StepTarget::Step(tree, id_of(&t, label)),
                level: Some(low),
                provenance: aegis_model::Provenance::default(),
            }],
            Origin::Edit,
        )
        .expect("rate");
    }
    let before = ws.project().clone();
    // Drop "Step three" and move "Step one" under the root.
    let ops = replace(
        &mut ws,
        tree,
        "OR Goal
  SEQ Chain
    - Step two
  - Step one
",
    )
    .expect("applies");
    assert!(ops.len() < 12, "{} ops", ops.len());
    let after = tree_of(ws.project(), tree);
    assert_eq!(
        to_outline(after),
        "OR Goal
  SEQ Chain
    - Step two
  - Step one
"
    );
    let idx = ws.project().attack_tree_idx(&tree).expect("tree");
    let one = after.idx_of(&id_of(after, "Step one")).expect("idx");
    assert_eq!(
        ws.project().step_feasibility(idx, one),
        Some(low),
        "a step that only moved keeps its rating"
    );
    ws.undo().expect("one undo step");
    assert_eq!(ws.project(), &before);
}

#[test]
fn a_new_first_line_renames_the_root_and_shared_steps_are_linked_and_unlinked() {
    let (mut ws, tree) = workspace_with(START);
    let root = tree_of(ws.project(), tree)
        .id_of(tree_of(ws.project(), tree).root())
        .expect("root");
    let ops = replace(
        &mut ws,
        tree,
        "AND Renamed goal
  SEQ Chain
    - Step one
    - Step two
  - Step three
  OR Alternative
    - Step one
",
    )
    .expect("applies");
    assert!(
        ops.iter()
            .any(|o| matches!(o, Op::RenameTreeNode { id, .. } if *id == root))
    );
    assert!(
        ops.iter()
            .any(|o| matches!(o, Op::SetTreeNodeGate { id, .. } if *id == root))
    );
    assert!(
        ops.iter().any(|o| matches!(o, Op::LinkTreeNode { .. })),
        "Step one is shared"
    );
    let t = tree_of(ws.project(), tree);
    assert_eq!(
        t.id_of(t.root()),
        Some(root),
        "still the same tree and root"
    );

    let ops = replace(
        &mut ws,
        tree,
        "AND Renamed goal
  SEQ Chain
    - Step one
    - Step two
  - Step three
  OR Alternative
    - Step two
",
    )
    .expect("applies");
    assert!(ops.iter().any(|o| matches!(o, Op::UnlinkTreeNode { .. })));
    assert!(ops.iter().any(|o| matches!(o, Op::LinkTreeNode { .. })));
}

#[test]
fn reordering_children_gives_the_new_order() {
    let (mut ws, tree) = workspace_with(START);
    replace(
        &mut ws,
        tree,
        "OR Goal
  - Step three
  SEQ Chain
    - Step two
    - Step one
",
    )
    .expect("applies");
    assert_eq!(
        to_outline(tree_of(ws.project(), tree)),
        "OR Goal
  - Step three
  SEQ Chain
    - Step two
    - Step one
"
    );
}

/// A small deterministic generator.
struct Rng(u64);
impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % n.max(1)
    }
}

/// A random outline: gates and steps, some step text used under two parents
/// (shared steps). Never the same text twice under one parent.
fn random_outline(rng: &mut Rng, nodes: usize) -> String {
    // (indent, text, index of the parent line)
    let mut lines: Vec<(usize, String, usize)> = vec![(0, "OR Root".to_owned(), 0)];
    // Gates that can take children now: (indent, line index).
    let mut open: Vec<(usize, usize)> = vec![(0, 0)];
    let mut gate_names = vec!["Root".to_owned()];
    let mut used: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();
    for _ in 0..nodes {
        let pick = rng.next(open.len());
        let (parent_indent, parent_line) = open[pick];
        open.truncate(pick + 1);
        let indent = parent_indent + 1;
        let (text, is_gate, name) = if rng.next(5) == 0 {
            let name = format!("gate {}", rng.next(12));
            let kind = ["OR", "AND", "SEQ"][rng.next(3)];
            (format!("{kind} {name}"), true, name)
        } else {
            let name = format!("step {}", rng.next(18));
            (format!("- {name}"), false, name)
        };
        if is_gate && gate_names.contains(&name) {
            continue;
        }
        if !used.insert((parent_line, name.clone())) {
            continue;
        }
        lines.push((indent, text, parent_line));
        if is_gate {
            gate_names.push(name);
            open.push((indent, lines.len() - 1));
        }
    }
    let mut out = String::new();
    for (indent, text, _) in lines {
        out.push_str(&"  ".repeat(indent));
        out.push_str(&text);
        out.push('\n');
    }
    out
}

#[test]
fn random_outline_edits_always_end_as_the_outline_says_and_undo_in_one_step() {
    let mut rng = Rng(42);
    let mut checked = 0;
    for round in 0..150 {
        let a = random_outline(&mut rng, 8 + round % 25);
        let b = random_outline(&mut rng, 8 + (round * 7) % 25);
        if let Err(e) = parse_outline(&a, None) {
            assert!(round >= 3, "a, round {round}: {e}\n{a}");
            continue;
        }
        let (mut ws, tree) = workspace_with(&a);
        let old = tree_of(ws.project(), tree).clone();
        let expected = match parse_outline(&b, Some(&old)) {
            Ok(t) => t,
            Err(e) => {
                assert!(round >= 3, "b, round {round}: {e}\n{b}");
                continue;
            }
        };
        let before = ws.project().clone();
        match replace(&mut ws, tree, &b) {
            Ok(_) => {}
            Err(e) => panic!("round {round}: {e}\nfrom:\n{a}\nto:\n{b}"),
        }
        assert_eq!(
            to_outline(tree_of(ws.project(), tree)),
            to_outline(&expected),
            "round {round}\nfrom:\n{a}\nto:\n{b}"
        );
        ws.undo().expect("undo");
        assert_eq!(ws.project(), &before, "round {round}: one undo step");
        checked += 1;
    }
    assert!(checked > 100, "{checked} rounds checked");
}

#[test]
fn a_big_tree_grows_in_steps_and_one_edit_is_limited_to_one_submission() {
    let (mut ws, tree) = workspace_with(START);
    // 900 more steps in one edit is fine (one op each).
    let mut text = String::from(START);
    for i in 0..900 {
        text.push_str(&format!("  - extra {i}\n"));
    }
    replace(&mut ws, tree, &text).expect("900 nodes in one edit");
    assert_eq!(tree_of(ws.project(), tree).len(), 905);
    // Another thousand in the same edit would need more than one submission.
    for i in 900..2_100 {
        text.push_str(&format!("  - extra {i}\n"));
    }
    let ops = Edit::ReplaceTree {
        tree,
        outline: text.clone(),
    }
    .to_ops(ws.project(), "alice", 3)
    .expect("ops");
    assert!(ops.len() > MAX_SUBMIT_OPS);
    assert!(matches!(
        ws.apply(ops, Origin::Edit),
        Err(WorkspaceError::TooLarge { .. })
    ));
    // The project is unchanged by the refused edit.
    assert_eq!(tree_of(ws.project(), tree).len(), 905);
}

#[test]
fn interactive_node_expansion_at_end_and_intermediate_points_preserves_ratings_and_undoes() {
    let (mut ws, tree) = workspace_with(START);
    let m = ws.project().methodology().clone();
    let high = m.feasibility_level("high").expect("level");
    let step_three = id_of(tree_of(ws.project(), tree), "Step three");
    ws.apply(
        vec![
            Op::SetStepFeasibility {
                target: StepTarget::Step(tree, step_three),
                level: Some(high),
                provenance: aegis_model::Provenance::default(),
            },
            Op::SetReviewMark {
                tree,
                node: step_three,
                marked: true,
                provenance: aegis_model::Provenance::default(),
            },
        ],
        Origin::Edit,
    )
    .expect("rate step three");

    // 1. End-of-node expansion: branch leaf "Step three" into an AND gate and
    // transfer its rating and review mark onto its new child step.
    let before_branch = ws.project().clone();
    let sub_step = NodeId::new_v7();
    let ops = Edit::BranchStepToGate {
        tree,
        node: step_three,
        id: sub_step,
        gate: aegis_model::GateKind::And,
        child_label: "Sub-step A".into(),
    }
    .to_ops(ws.project(), "alice", 10)
    .expect("branch ops");
    ws.apply(ops, Origin::Edit).expect("branch applies");
    assert_eq!(
        to_outline(tree_of(ws.project(), tree)),
        "OR Goal
  SEQ Chain
    - Step one
    - Step two
  AND Step three
    - Sub-step A
"
    );
    let t_idx = ws.project().attack_tree_idx(&tree).expect("tree");
    let sub_idx = tree_of(ws.project(), tree).idx_of(&sub_step).expect("sub");
    assert_eq!(ws.project().step_feasibility(t_idx, sub_idx), Some(high));
    assert!(ws.project().review_marked(tree, sub_step));
    ws.undo().expect("undo branch");
    assert_eq!(ws.project(), &before_branch);
    ws.redo().expect("redo branch");

    // 2. End-of-node expansion on the newly created gate: append a second step.
    let sub_step_b = NodeId::new_v7();
    let ops = Edit::AddTreeChild {
        tree,
        parent: step_three,
        id: sub_step_b,
        gate: None,
        label: "Sub-step B".into(),
    }
    .to_ops(ws.project(), "alice", 11)
    .expect("add child ops");
    assert_eq!(ops.len(), 1, "adding a child to a gate is a single op");
    ws.apply(ops, Origin::Edit).expect("add child applies");

    // 3. Intermediate-point expansion: insert a parent OR gate above "SEQ Chain"
    // wrapping its entire subtree without losing "Sub-step A"'s rating/review mark.
    let chain = id_of(tree_of(ws.project(), tree), "Chain");
    let inter_gate = NodeId::new_v7();
    let before_inter = ws.project().clone();
    let ops = Edit::InsertParentGate {
        tree,
        node: chain,
        id: inter_gate,
        gate: aegis_model::GateKind::Or,
        label: "Intermediate gate".into(),
    }
    .to_ops(ws.project(), "alice", 12)
    .expect("insert parent gate ops");
    ws.apply(ops, Origin::Edit).expect("insert parent gate");
    assert_eq!(
        to_outline(tree_of(ws.project(), tree)),
        "OR Goal
  OR Intermediate gate
    SEQ Chain
      - Step one
      - Step two
  AND Step three
    - Sub-step A
    - Sub-step B
"
    );
    ws.undo().expect("undo intermediate gate");
    assert_eq!(ws.project(), &before_inter);
    ws.redo().expect("redo intermediate gate");

    // 4. Add a sibling step next to "Chain" under "Intermediate gate".
    let sibling = NodeId::new_v7();
    let ops = Edit::AddSiblingNode {
        tree,
        node: chain,
        id: sibling,
        gate: None,
        label: "Direct bypass".into(),
    }
    .to_ops(ws.project(), "alice", 13)
    .expect("sibling ops");
    assert_eq!(ops.len(), 1);
    ws.apply(ops, Origin::Edit).expect("sibling applies");

    // 5. Rename "Sub-step B" in-place and change "Step three" gate to SEQ.
    let ops = Edit::RenameTreeNode {
        tree,
        node: sub_step_b,
        label: "Sub-step B renamed".into(),
    }
    .to_ops(ws.project(), "alice", 14)
    .expect("rename ops");
    ws.apply(ops, Origin::Edit).expect("rename applies");
    let ops = Edit::SetTreeNodeGate {
        tree,
        node: step_three,
        gate: aegis_model::GateKind::SeqAnd,
    }
    .to_ops(ws.project(), "alice", 15)
    .expect("set gate ops");
    ws.apply(ops, Origin::Edit).expect("set gate applies");

    // 6. Reorder "Sub-step B renamed" above "Sub-step A" using LIS diffing:
    // "Sub-step A" stays untouched in place, only "Sub-step B renamed" moves!
    let ops = Edit::MoveTreeNode {
        tree,
        node: sub_step_b,
        delta: -1,
    }
    .to_ops(ws.project(), "alice", 16)
    .expect("move ops");
    assert_eq!(
        ops.len(),
        2,
        "LIS keeps Sub-step A untouched; only Sub-step B is removed and re-added: {ops:?}"
    );
    ws.apply(ops, Origin::Edit).expect("move applies");
    assert_eq!(
        to_outline(tree_of(ws.project(), tree)),
        "OR Goal
  OR Intermediate gate
    SEQ Chain
      - Step one
      - Step two
    - Direct bypass
  SEQ Step three
    - Sub-step B renamed
    - Sub-step A
"
    );

    // 7. Remove "Intermediate gate" subtree in one undoable step.
    let before_remove = ws.project().clone();
    let ops = Edit::RemoveTreeNode {
        tree,
        node: inter_gate,
    }
    .to_ops(ws.project(), "alice", 17)
    .expect("remove ops");
    ws.apply(ops, Origin::Edit).expect("remove applies");
    assert_eq!(
        to_outline(tree_of(ws.project(), tree)),
        "OR Goal
  SEQ Step three
    - Sub-step B renamed
    - Sub-step A
"
    );
    ws.undo().expect("undo remove");
    assert_eq!(ws.project(), &before_remove);
}
