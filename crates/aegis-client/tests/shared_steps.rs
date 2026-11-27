//! Shared-step client edits, cleanup, inspector routing, table/grid UI, and exact undo (141e, 141f).

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::explorer::Owner;
use aegis_client::grid::{
    CellEditorKind, CellInput, GridSelection, add_row_edit, cell_editor, delete_rows_edit,
    edit_cell, fill_down, paste_tsv,
};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::inspector::inspector_view;
use aegis_client::pages::{self, vocabulary};
use aegis_client::relations::RelationKey;
use aegis_client::table::{selection_label, table_rows, table_rows_for_owner};
use aegis_client::view::{Page, PageGroup, Selection, TableId, UiState, page_descriptions};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AttackTree, AttackType, ComponentId, ElementId, ElementRef, FactorLevel, FeasibilityFactor,
    FeasibilityLevel, GateKind, InterfaceId, ItemRef, Methodology, Node, NodeId, Op, Project,
    Provenance, SharedStepId, StepTarget, TreeId,
};

fn prov() -> Provenance {
    Provenance::human("shared-test", 1_792_956_600_000)
}

fn apply(ws: &mut Workspace, edit: Edit) {
    let ops = edit.to_ops(ws.project(), "shared-test", 1).expect("ops");
    assert!(!ops.is_empty(), "edit unexpectedly unchanged: {edit:?}");
    ws.apply(ops.clone(), Origin::Edit)
        .unwrap_or_else(|e| panic!("edit applies: {e}\n{ops:?}"));
}

fn exact_undo(ws: &mut Workspace, edit: Edit) {
    let before = ws.project().clone();
    apply(ws, edit);
    assert!(*ws.project() != before, "edit did not change the project");
    ws.undo().expect("single undo");
    assert!(
        *ws.project() == before,
        "single undo did not restore the project"
    );
}

fn empty() -> Workspace {
    Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

fn add_system(ws: &mut Workspace, name: &str) -> ElementId {
    let id = ElementId::new_v7();
    apply(
        ws,
        Edit::AddSystem {
            id,
            name: name.into(),
        },
    );
    id
}

fn add_component(ws: &mut Workspace, name: &str) -> ComponentId {
    let id = ComponentId::new_v7();
    ws.apply(
        vec![Op::AddComponent {
            id,
            name: name.into(),
            parent: None,
            provenance: prov(),
        }],
        Origin::Edit,
    )
    .expect("component");
    id
}

fn add_tree(ws: &mut Workspace, tree: AttackTree) -> TreeId {
    let id = TreeId::new_v7();
    ws.apply(
        vec![Op::AddAttackTree {
            role: Default::default(),
            id,
            tree,
            step_feasibility: Vec::new(),
            provenance: prov(),
        }],
        Origin::Edit,
    )
    .expect("tree");
    id
}

fn gate_tree(labels: &[&str]) -> AttackTree {
    let mut tree =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Root")).expect("root");
    for label in labels {
        tree.add_child(tree.root(), NodeId::new_v7(), Node::step(*label))
            .expect("child");
    }
    tree
}

fn first_child(tree: &AttackTree) -> NodeId {
    tree.id_of(tree.children(tree.root())[0]).expect("child id")
}

fn rate_local(ws: &mut Workspace, tree: TreeId, node: NodeId, via: ComponentId) {
    let m = ws.project().methodology();
    let factor = m.feasibility_factors().next().expect("factor");
    let level = m.feasibility_level("high").expect("high");
    let factor_level = m.factor_levels(factor).next().expect("factor level");
    let attack_type = m.attack_type("debug-access").expect("attack type");
    let level_index = m
        .feasibility_levels()
        .position(|(l, _, _)| l == level)
        .expect("index");
    let factor_index = m
        .feasibility_factors()
        .position(|f| f == factor)
        .expect("factor index");
    let factor_level_index = m
        .factor_levels(factor)
        .position(|l| l == factor_level)
        .expect("level index");
    let attack_index = m
        .attack_types()
        .position(|a| a == attack_type)
        .expect("attack index");
    for edit in [
        Edit::SetStepFeasibility {
            tree,
            node,
            level: Some(level_index),
        },
        Edit::SetStepFactor {
            tree,
            node,
            factor: factor_index,
            level: Some(factor_level_index),
        },
        Edit::SetStepVia {
            tree,
            node,
            via: Some(ItemRef::Component(via)),
        },
        Edit::SetStepAttackType {
            tree,
            node,
            attack_type: Some(attack_index),
        },
    ] {
        apply(ws, edit);
    }
    let t = ws.project().attack_tree_idx(&tree).expect("tree");
    let n = ws
        .project()
        .attack_tree(t)
        .expect("tree")
        .idx_of(&node)
        .expect("node");
    assert_eq!(ws.project().step_feasibility(t, n), Some(level));
    assert_eq!(ws.project().step_factor(t, n, factor), Some(factor_level));
    assert_eq!(ws.project().step_via(t, n), Some(ItemRef::Component(via)));
    assert_eq!(ws.project().step_attack_type(t, n), Some(attack_type));
}

#[allow(clippy::too_many_arguments)]
fn assert_ratings(
    project: &Project,
    tree: TreeId,
    node: NodeId,
    feasibility: Option<FeasibilityLevel>,
    factor: FeasibilityFactor,
    factor_level: Option<FactorLevel>,
    via: Option<ItemRef>,
    attack_type: Option<AttackType>,
) {
    let t = project.attack_tree_idx(&tree).expect("tree");
    let n = project
        .attack_tree(t)
        .expect("tree")
        .idx_of(&node)
        .expect("node");
    assert_eq!(project.step_feasibility(t, n), feasibility);
    assert_eq!(project.step_factor(t, n, factor), factor_level);
    assert_eq!(project.step_via(t, n), via);
    assert_eq!(project.step_attack_type(t, n), attack_type);
}

fn linked_workspace(extra_trees: usize) -> (Workspace, TreeId, NodeId, SharedStepId, ComponentId) {
    let mut ws = empty();
    let component = add_component(&mut ws, "Gateway");
    let tree = gate_tree(&["Linked"]);
    let tree_id = add_tree(&mut ws, tree.clone());
    let node = first_child(&tree);
    rate_local(&mut ws, tree_id, node, component);
    let shared = SharedStepId::new_v7();
    apply(
        &mut ws,
        Edit::MakeShared {
            tree: tree_id,
            node,
            id: shared,
        },
    );
    for _ in 0..extra_trees {
        let tree = gate_tree(&["Other"]);
        let id = add_tree(&mut ws, tree.clone());
        apply(
            &mut ws,
            Edit::UseSharedStep {
                tree: id,
                node: first_child(&tree),
                shared_step: shared,
            },
        );
    }
    (ws, tree_id, node, shared, component)
}

#[test]
fn shared_note_counts_distinct_trees_and_localizes_singular_plural() {
    for (tree_count, first_children, expected) in [
        (1, 2, "Shared step L00, used in 1 tree"),
        (2, 1, "Shared step L00, used in 2 trees"),
        (5, 1, "Shared step L00, used in 5 trees"),
    ] {
        let mut ws = empty();
        let shared = SharedStepId::new_v7();
        apply(
            &mut ws,
            Edit::AddSharedStep {
                id: shared,
                name: "Reusable".into(),
            },
        );
        let mut first_node = None;
        for tree_index in 0..tree_count {
            let count = if tree_index == 0 { first_children } else { 1 };
            let tree = gate_tree(&vec!["Step"; count]);
            let tree_id = add_tree(&mut ws, tree.clone());
            for child in tree.children(tree.root()) {
                let node = tree.id_of(*child).expect("node");
                apply(
                    &mut ws,
                    Edit::UseSharedStep {
                        tree: tree_id,
                        node,
                        shared_step: shared,
                    },
                );
                if tree_index == 0 {
                    first_node = Some(node);
                }
            }
        }
        let view = inspector_view(
            &ws,
            Selection::TreeNode(
                ws.project().attack_trees().next().expect("tree").1,
                first_node.expect("first node"),
            ),
            &Catalog::load(Language::En),
        )
        .expect("linked view");
        assert_eq!(view.shared_note.as_deref(), Some(expected));
        let ja =
            inspector_view(&ws, view.selection, &Catalog::load(Language::Ja)).expect("linked view");
        assert_eq!(
            ja.shared_note.as_deref(),
            Some(match tree_count {
                1 => "共通ステップ L00、1 件のツリーで使用",
                2 => "共通ステップ L00、2 件のツリーで使用",
                _ => "共通ステップ L00、5 件のツリーで使用",
            })
        );
    }
}

#[test]
fn make_shared_and_unshare_preserve_all_rating_fields_with_exact_undo() {
    let (mut ws, tree, node, shared, component) = linked_workspace(0);
    let m = ws.project().methodology();
    let factor = m.feasibility_factors().next().expect("factor");
    let high = m.feasibility_level("high").expect("high");
    let factor_level = m.factor_levels(factor).next().expect("factor level");
    let attack_type = m.attack_type("debug-access").expect("attack type");
    let via = ItemRef::Component(component);

    let before = ws.project().clone();
    exact_undo(&mut ws, Edit::Unshare { tree, node });
    assert_eq!(ws.project(), &before);
    apply(&mut ws, Edit::Unshare { tree, node });
    assert_ratings(
        ws.project(),
        tree,
        node,
        Some(high),
        factor,
        Some(factor_level),
        Some(via),
        Some(attack_type),
    );
    let shared_idx = ws.project().shared_step_idx(&shared).expect("shared");
    let s = ws.project().shared_step(shared_idx).expect("shared");
    assert_eq!(s.feasibility(), Some(high));
    assert_eq!(s.factor(factor), Some(factor_level));
    assert_eq!(ws.project().shared_step_via(shared_idx), Some(via));
    assert_eq!(s.attack_type(), Some(attack_type));
}

#[test]
fn make_shared_is_one_exactly_undoable_edit() {
    let mut ws = empty();
    let component = add_component(&mut ws, "Gateway");
    let tree = gate_tree(&["Local"]);
    let tree_id = add_tree(&mut ws, tree.clone());
    let node = first_child(&tree);
    rate_local(&mut ws, tree_id, node, component);
    let shared = SharedStepId::new_v7();
    exact_undo(
        &mut ws,
        Edit::MakeShared {
            tree: tree_id,
            node,
            id: shared,
        },
    );
    assert!(ws.project().shared_step_idx(&shared).is_none());
    apply(
        &mut ws,
        Edit::MakeShared {
            tree: tree_id,
            node,
            id: shared,
        },
    );
    let t = ws.project().attack_tree_idx(&tree_id).expect("tree");
    let n = ws
        .project()
        .attack_tree(t)
        .expect("tree")
        .idx_of(&node)
        .expect("node");
    assert_eq!(ws.project().step_shared_id(t, n), Some(shared));
}

#[test]
fn remove_shared_step_restores_description_translation_and_every_user() {
    let (mut ws, _, _, shared, _) = linked_workspace(2);
    apply(
        &mut ws,
        Edit::SetDescription {
            target: ElementRef::SharedStep(shared),
            text: "Reusable description".into(),
        },
    );
    apply(
        &mut ws,
        Edit::SetDescriptionTranslation {
            target: ElementRef::SharedStep(shared),
            lang: "ja".into(),
            text: "共通の説明".into(),
        },
    );
    assert!(
        ws.project().shared_step_idx(&shared).is_some(),
        "shared step disappeared before removal"
    );
    exact_undo(
        &mut ws,
        Edit::RemoveSharedStep {
            shared_step: shared,
        },
    );
    apply(
        &mut ws,
        Edit::RemoveSharedStep {
            shared_step: shared,
        },
    );
    assert!(ws.project().shared_step_idx(&shared).is_none());
    for (t, _, tree) in ws.project().attack_trees() {
        for (n, _, _) in tree.iter() {
            assert!(ws.project().step_shared(t, n).is_none());
        }
    }
}

#[test]
fn removing_direct_step_gate_subtree_and_tree_undo_exactly() {
    let (mut ws, tree, node, shared, _) = linked_workspace(0);
    exact_undo(&mut ws, Edit::RemoveTreeNode { tree, node });
    apply(&mut ws, Edit::RemoveTreeNode { tree, node });
    let shared_idx = ws.project().shared_step_idx(&shared).expect("shared");
    assert_eq!(ws.project().shared_step_users_count(shared_idx), 0);
    ws.undo().expect("restore direct step");

    let mut nested =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Root")).expect("root");
    let gate = NodeId::new_v7();
    let child = NodeId::new_v7();
    let gate_idx = nested
        .add_child(nested.root(), gate, Node::gate(GateKind::And, "Gate"))
        .expect("gate");
    nested
        .add_child(gate_idx, child, Node::step("Nested"))
        .expect("child");
    let nested_tree = add_tree(&mut ws, nested.clone());
    apply(
        &mut ws,
        Edit::UseSharedStep {
            tree: nested_tree,
            node: child,
            shared_step: shared,
        },
    );
    exact_undo(
        &mut ws,
        Edit::RemoveTreeNode {
            tree: nested_tree,
            node: gate,
        },
    );
    apply(
        &mut ws,
        Edit::RemoveTreeNode {
            tree: nested_tree,
            node: gate,
        },
    );
    assert_eq!(ws.project().shared_step_users_count(shared_idx), 1);
    ws.undo().expect("restore gate subtree");

    apply(&mut ws, Edit::RemoveTree { tree: nested_tree });
    exact_undo(&mut ws, Edit::RemoveTree { tree });
    apply(&mut ws, Edit::RemoveTree { tree });
    assert!(ws.project().attack_tree_idx(&tree).is_none());
    assert_eq!(ws.project().shared_step_users_count(shared_idx), 0);
}

#[test]
fn replace_tree_transfers_and_drops_linked_steps_with_exact_undo() {
    let (mut ws, tree, node, shared, _) = linked_workspace(0);
    let moved = "OR Root\n  AND Empty\n  OR Destination\n    - Linked\n";
    exact_undo(
        &mut ws,
        Edit::ReplaceTree {
            tree,
            outline: moved.into(),
        },
    );
    apply(
        &mut ws,
        Edit::ReplaceTree {
            tree,
            outline: moved.into(),
        },
    );
    let t = ws.project().attack_tree_idx(&tree).expect("tree");
    let n = ws
        .project()
        .attack_tree(t)
        .expect("tree")
        .idx_of(&node)
        .expect("node");
    assert_eq!(ws.project().step_shared_id(t, n), Some(shared));
    ws.undo().expect("restore before drop");
    exact_undo(
        &mut ws,
        Edit::ReplaceTree {
            tree,
            outline: "OR Root\n  AND Empty\n".into(),
        },
    );
    apply(
        &mut ws,
        Edit::ReplaceTree {
            tree,
            outline: "OR Root\n  AND Empty\n".into(),
        },
    );
    assert_eq!(
        ws.project()
            .shared_step_users_count(ws.project().shared_step_idx(&shared).expect("shared"),),
        0
    );
}

#[test]
fn step_edits_and_outline_notes_route_through_the_shared_step() {
    let (mut ws, tree, node, shared, _) = linked_workspace(0);
    let second_component = add_component(&mut ws, "Brake ECU");
    let m = Arc::clone(ws.project().methodology());
    let factor = m.feasibility_factors().next().expect("factor");
    let low = m.feasibility_level("low").expect("low");
    let factor_level = m.factor_levels(factor).nth(1).expect("factor level");
    let attack_type = m.attack_types().nth(1).expect("attack type");
    let low_index = m
        .feasibility_levels()
        .position(|(l, _, _)| l == low)
        .expect("low index");
    let factor_index = m
        .feasibility_factors()
        .position(|f| f == factor)
        .expect("factor index");
    let factor_level_index = m
        .factor_levels(factor)
        .position(|l| l == factor_level)
        .expect("level index");
    let attack_index = m
        .attack_types()
        .position(|a| a == attack_type)
        .expect("attack index");
    for edit in [
        Edit::SetStepFeasibility {
            tree,
            node,
            level: Some(low_index),
        },
        Edit::SetStepFactor {
            tree,
            node,
            factor: factor_index,
            level: Some(factor_level_index),
        },
        Edit::SetStepVia {
            tree,
            node,
            via: Some(ItemRef::Component(second_component)),
        },
        Edit::SetStepAttackType {
            tree,
            node,
            attack_type: Some(attack_index),
        },
    ] {
        apply(&mut ws, edit);
    }
    let idx = ws.project().shared_step_idx(&shared).expect("shared");
    let s = ws.project().shared_step(idx).expect("shared");
    assert_eq!(s.feasibility(), Some(low));
    assert_eq!(s.factor(factor), Some(factor_level));
    assert_eq!(
        ws.project().shared_step_via(idx),
        Some(ItemRef::Component(second_component))
    );
    assert_eq!(s.attack_type(), Some(attack_type));

    apply(
        &mut ws,
        Edit::ReplaceTree {
            tree,
            outline: "OR Root\n  - Linked | through: - | type: -\n".into(),
        },
    );
    let s = ws.project().shared_step(idx).expect("shared");
    assert_eq!(ws.project().shared_step_via(idx), None);
    assert_eq!(s.attack_type(), None);
}

#[test]
fn component_removal_clears_shared_via_once_and_unshared_via() {
    let mut ws = empty();
    let component = add_component(&mut ws, "Gateway");
    let shared = SharedStepId::new_v7();
    apply(
        &mut ws,
        Edit::AddSharedStep {
            id: shared,
            name: "Reusable".into(),
        },
    );
    apply(
        &mut ws,
        Edit::SetSharedStepVia {
            shared_step: shared,
            via: Some(ItemRef::Component(component)),
        },
    );
    let unused = SharedStepId::new_v7();
    apply(
        &mut ws,
        Edit::AddSharedStep {
            id: unused,
            name: "Unused".into(),
        },
    );
    apply(
        &mut ws,
        Edit::SetSharedStepVia {
            shared_step: unused,
            via: Some(ItemRef::Component(component)),
        },
    );
    let tree = gate_tree(&["Shared A", "Shared B", "Local"]);
    let tree_id = add_tree(&mut ws, tree.clone());
    let nodes: Vec<_> = tree
        .children(tree.root())
        .iter()
        .map(|n| tree.id_of(*n).expect("node"))
        .collect();
    for node in &nodes[..2] {
        apply(
            &mut ws,
            Edit::UseSharedStep {
                tree: tree_id,
                node: *node,
                shared_step: shared,
            },
        );
    }
    apply(
        &mut ws,
        Edit::SetStepVia {
            tree: tree_id,
            node: nodes[2],
            via: Some(ItemRef::Component(component)),
        },
    );
    let ops = Edit::RemoveComponent { component }
        .to_ops(ws.project(), "shared-test", 1)
        .expect("ops");
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op,
                Op::SetStepVia {
                    target: StepTarget::Shared(_),
                    via: None,
                    ..
                }
            ))
            .count(),
        2
    );
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op,
                Op::SetStepVia {
                    target: StepTarget::Step(_, _),
                    via: None,
                    ..
                }
            ))
            .count(),
        1
    );
    exact_undo(&mut ws, Edit::RemoveComponent { component });
    apply(&mut ws, Edit::RemoveComponent { component });
    assert_eq!(
        ws.project()
            .shared_step_via(ws.project().shared_step_idx(&shared).expect("shared"),),
        None
    );
    assert_eq!(
        ws.project()
            .shared_step_via(ws.project().shared_step_idx(&unused).expect("unused"),),
        None
    );
}

#[test]
fn interface_removal_clears_an_unused_shared_via() {
    let mut ws = empty();
    let component = add_component(&mut ws, "Gateway");
    let interface = InterfaceId::new_v7();
    let kind = ws
        .project()
        .methodology()
        .interface_kinds()
        .next()
        .expect("interface kind");
    ws.apply(
        vec![Op::AddInterface {
            id: interface,
            name: "Debug".into(),
            component,
            kind,
            provenance: prov(),
        }],
        Origin::Edit,
    )
    .expect("interface");
    let shared = SharedStepId::new_v7();
    apply(
        &mut ws,
        Edit::AddSharedStep {
            id: shared,
            name: "Unused".into(),
        },
    );
    apply(
        &mut ws,
        Edit::SetSharedStepVia {
            shared_step: shared,
            via: Some(ItemRef::Interface(interface)),
        },
    );
    exact_undo(&mut ws, Edit::RemoveInterface { interface });
    apply(&mut ws, Edit::RemoveInterface { interface });
    assert_eq!(
        ws.project()
            .shared_step_via(ws.project().shared_step_idx(&shared).expect("shared")),
        None
    );
}

#[test]
fn missing_shared_targets_are_errors_not_noops() {
    let mut ws = empty();
    let missing = SharedStepId::new_v7();
    assert!(matches!(
        Edit::RemoveSharedStep {
            shared_step: missing,
        }
        .to_ops(ws.project(), "shared-test", 1),
        Err(EditError::Missing)
    ));
    let tree = TreeId::new_v7();
    let node = NodeId::new_v7();
    assert!(matches!(
        Edit::UnuseSharedStep { tree, node }.to_ops(ws.project(), "shared-test", 1),
        Err(EditError::Missing)
    ));
    let tree = add_tree(&mut ws, gate_tree(&["Local"]));
    let node = first_child(
        ws.project()
            .attack_tree_idx(&tree)
            .and_then(|i| ws.project().attack_tree(i))
            .expect("tree"),
    );
    assert!(matches!(
        Edit::UnuseSharedStep { tree, node }.to_ops(ws.project(), "shared-test", 1),
        Err(EditError::Missing)
    ));
}

#[test]
fn unuse_shared_step_preserves_rating_fields_and_undoes_exactly() {
    let (mut ws, tree, node, shared, _) = linked_workspace(0);
    let shared_idx = ws.project().shared_step_idx(&shared).expect("shared");
    let factor = ws
        .project()
        .methodology()
        .feasibility_factors()
        .next()
        .expect("factor");
    let step = ws.project().shared_step(shared_idx).expect("shared");
    let expected = (
        step.feasibility(),
        step.factor(factor),
        ws.project().shared_step_via(shared_idx),
        step.attack_type(),
    );
    let edit = Edit::UnuseSharedStep { tree, node };
    exact_undo(&mut ws, edit.clone());
    apply(&mut ws, edit);
    let t = ws.project().attack_tree_idx(&tree).expect("tree");
    let n = ws
        .project()
        .attack_tree(t)
        .expect("tree")
        .idx_of(&node)
        .expect("node");
    assert_eq!(ws.project().step_shared(t, n), None);
    assert_eq!(
        (
            ws.project().step_feasibility(t, n),
            ws.project().step_factor(t, n, factor),
            ws.project().step_via(t, n),
            ws.project().step_attack_type(t, n),
        ),
        expected
    );
}

#[test]
fn shared_step_selection_exposes_both_forward_relations() {
    let (ws, _, _, shared, _component) = linked_workspace(1);
    let view = inspector_view(
        &ws,
        Selection::SharedStep(shared),
        &Catalog::load(Language::En),
    )
    .expect("shared view");
    assert_eq!(view.visual_id, "L00");
    assert_eq!(view.kind, "Shared step");
    assert!(
        view.relations
            .iter()
            .any(|r| r.key == RelationKey::SHARED_STEP_VIA)
    );
    let used = view
        .relations
        .iter()
        .find(|r| r.key == RelationKey::SHARED_STEP_USED_IN)
        .expect("used in");
    assert_eq!(used.chips.len(), 2);
}

#[test]
fn shared_blocks_page_table_and_description_select_the_shared_step() {
    let (ws, _, _, shared, component) = linked_workspace(1);
    assert_eq!(Page::SharedBlocks.index(), 9);
    assert_eq!(Page::SharedBlocks.group(), Some(PageGroup::Feasibility));
    assert_eq!(
        Page::SharedBlocks.tables(),
        [TableId::SharedSteps, TableId::Fragments]
    );
    assert_eq!(
        TableId::SharedSteps
            .columns()
            .iter()
            .map(|c| c.key)
            .collect::<Vec<_>>(),
        ["id", "name", "feasibility", "via", "attack-type", "used-in"]
    );

    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());
    let rows = table_rows(&ws, TableId::SharedSteps, &vocab, &catalog);
    let row = rows
        .iter()
        .find(|r| r.target == Some(Selection::SharedStep(shared)))
        .expect("shared table row");
    assert_eq!(row.cells[0].text, "L00");
    assert_eq!(
        row.cells[3].chips[0].target,
        Selection::Component(component)
    );
    assert_eq!(row.cells[5].chips.len(), 2);
    assert!(
        row.cells[5]
            .chips
            .iter()
            .all(|chip| matches!(chip.target, Selection::TreeNode(_, _)))
    );

    let mut ui = UiState::default();
    pages::select(&mut ui, Selection::SharedStep(shared));
    assert_eq!(ui.page, Page::SharedBlocks);
    assert_eq!(ui.selected_shared_step, Some(shared));
    let descriptions = page_descriptions(&ws, &ui);
    assert_eq!(descriptions.len(), 1);
    assert_eq!(descriptions[0].target, ElementRef::SharedStep(shared));
    assert_eq!(descriptions[0].kind, "shared-step");
    assert!(
        pages::palette_elements(&ws)
            .iter()
            .any(|e| e.target == Selection::SharedStep(shared))
    );
}

#[test]
fn shared_step_owner_filtering_uses_system_usage() {
    let mut ws = empty();
    let brake = add_system(&mut ws, "Brake");
    let adas = add_system(&mut ws, "ADAS");
    let caliper = add_component(&mut ws, "Caliper");
    let camera = add_component(&mut ws, "Camera");
    for (component, system) in [(caliper, brake), (camera, adas)] {
        apply(
            &mut ws,
            Edit::MoveToSystem {
                components: vec![component],
                system: Some(system),
            },
        );
    }
    let shared_brake = SharedStepId::new_v7();
    let shared_adas = SharedStepId::new_v7();
    for (id, name, component) in [
        (shared_brake, "Replay brake frame", caliper),
        (shared_adas, "Blind the camera", camera),
    ] {
        apply(
            &mut ws,
            Edit::AddSharedStep {
                id,
                name: name.into(),
            },
        );
        apply(
            &mut ws,
            Edit::SetSharedStepVia {
                shared_step: id,
                via: Some(ItemRef::Component(component)),
            },
        );
    }
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());
    let targets = |owner| {
        table_rows_for_owner(&ws, TableId::SharedSteps, owner, &vocab, &catalog)
            .into_iter()
            .map(|r| r.target)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        targets(Owner::Project),
        vec![
            Some(Selection::SharedStep(shared_brake)),
            Some(Selection::SharedStep(shared_adas))
        ]
    );
    assert_eq!(
        targets(Owner::System(brake)),
        vec![Some(Selection::SharedStep(shared_brake))]
    );
    assert_eq!(
        targets(Owner::System(adas)),
        vec![Some(Selection::SharedStep(shared_adas))]
    );
    assert!(targets(Owner::Unassigned).is_empty());
}

#[test]
fn shared_steps_grid_add_edit_fill_paste_and_delete_are_undoable() {
    let mut ws = empty();
    let component = add_component(&mut ws, "Gateway");
    let (add, first) = add_row_edit(
        ws.project(),
        TableId::SharedSteps,
        Owner::Project,
        "Reusable request",
    )
    .expect("add shared row");
    assert!(matches!(add, Edit::AddSharedStep { .. }));
    assert!(matches!(first, Selection::SharedStep(_)));
    apply(&mut ws, add);
    let (add, second) = add_row_edit(
        ws.project(),
        TableId::SharedSteps,
        Owner::Project,
        "Second request",
    )
    .expect("add second shared row");
    apply(&mut ws, add);

    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());
    let col = |key| {
        *TableId::SharedSteps
            .columns()
            .iter()
            .find(|c| c.key == key)
            .expect("shared column")
    };
    assert!(matches!(
        cell_editor(
            ws.project(),
            TableId::SharedSteps,
            Some(first),
            &col("name"),
            &vocab,
            &catalog
        ),
        CellEditorKind::Text { .. }
    ));
    assert!(matches!(
        cell_editor(
            ws.project(),
            TableId::SharedSteps,
            Some(first),
            &col("via"),
            &vocab,
            &catalog
        ),
        CellEditorKind::LinkPicker {
            relation: RelationKey::SHARED_STEP_VIA
        }
    ));
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::SharedSteps,
            Some(first),
            &col("used-in"),
            &vocab,
            &catalog
        ),
        CellEditorKind::ReadOnly
    );
    for edit in [
        edit_cell(
            ws.project(),
            TableId::SharedSteps,
            first,
            &col("name"),
            &CellInput::Text("Renamed request".into()),
        )
        .expect("rename shared"),
        edit_cell(
            ws.project(),
            TableId::SharedSteps,
            first,
            &col("feasibility"),
            &CellInput::SingleChoice(Some(0)),
        )
        .expect("rate shared"),
        edit_cell(
            ws.project(),
            TableId::SharedSteps,
            first,
            &col("attack-type"),
            &CellInput::SingleChoice(Some(0)),
        )
        .expect("type shared"),
    ] {
        apply(&mut ws, edit);
    }

    let rows = table_rows(&ws, TableId::SharedSteps, &vocab, &catalog);
    let src = rows
        .iter()
        .position(|r| r.target == Some(first))
        .expect("source row");
    let dst = rows
        .iter()
        .position(|r| r.target == Some(second))
        .expect("destination row");
    let mut selection = GridSelection::single(src, 2);
    selection.extend_to(dst, 2);
    let columns = TableId::SharedSteps.columns();
    let fill = fill_down(
        ws.project(),
        TableId::SharedSteps,
        &rows,
        &columns.iter().collect::<Vec<_>>(),
        selection,
    )
    .expect("fill shared rating");
    apply(&mut ws, fill);

    let tsv = format!(
        "\tPasted shared\t{}\t{}\t{}\t",
        vocab.feasibility[0],
        selection_label(ws.project(), Selection::Component(component)),
        vocab.attack_types[0]
    );
    let rows = table_rows(&ws, TableId::SharedSteps, &vocab, &catalog);
    let paste = paste_tsv(
        ws.project(),
        TableId::SharedSteps,
        Owner::Project,
        &rows,
        &columns.iter().collect::<Vec<_>>(),
        rows.len(),
        0,
        &tsv,
        &vocab,
        &catalog,
    )
    .expect("paste shared row");
    apply(&mut ws, paste);
    let (idx, id, pasted) = ws
        .project()
        .shared_steps()
        .find(|(_, _, step)| step.name() == "Pasted shared")
        .expect("pasted shared row");
    let m = ws.project().methodology();
    assert_eq!(
        pasted.feasibility(),
        Some(m.feasibility_levels().next().unwrap().0)
    );
    assert_eq!(pasted.attack_type(), Some(m.attack_types().next().unwrap()));
    assert_eq!(
        ws.project().shared_step_via(idx),
        Some(ItemRef::Component(component))
    );
    let delete = delete_rows_edit(&[Selection::SharedStep(id)]).expect("delete shared row");
    exact_undo(&mut ws, delete);
}
