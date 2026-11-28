//! Integration tests for bypass tree UI surfaces: explain-why, control
//! inspector/detail, `RelationKey::CONTROL_BYPASS`, `Edit::SetControlBypass`,
//! `Edit::CreateBypassFromSelection`, `Edit::RemoveControl`, `diff_lines`, and
//! `"control off"` hatching on the canvas (143c).

use std::sync::Arc;

use aegis_canvas::access::{describe, sentence};
use aegis_canvas::{Item, Paint, RenderOptions, Size, Viewport, render};
use aegis_client::commands::{Edit, EditError};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::inspector::{explain_selection, inspector_view, reverse_relation_label};
use aegis_client::pages::{
    self, comparison, control_detail, reduced_by_text, risk_rows, threat_detail,
};
use aegis_client::picker::MultiLinkPicker;
use aegis_client::relations::{RelationKey, candidates, related, reverse_related};
use aegis_client::view::{self, Selection, UiState, tree_scene};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AttackTree, ConditionId, ControlId, ControlStatus, DamageId, GateKind, Methodology,
    Node, NodeId, Op, Provenance, ThreatId, TreeId, TreeRole,
};

fn prov() -> Provenance {
    Provenance::human("tester", 1_792_956_600_000)
}

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), "tester", 1_792_956_600_000)
        .expect("edit builds ops");
    ws.apply(ops, Origin::Edit).expect("edit applies");
}

struct Fixture {
    ws: Workspace,
    threat_id: ThreatId,
    tree_id: TreeId,
    step_id: NodeId,
    bypass_id: TreeId,
    bypass_leaf: NodeId,
    ctrl: ControlId,
}

fn setup_bypass_fixture() -> Fixture {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let vocab = pages::vocabulary(&m);
    let mut ws = Workspace::empty(Arc::clone(&m));
    let (asset_id, damage_id, threat_id, tree_id, bypass_id, ctrl) = (
        AssetId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
        TreeId::new_v7(),
        TreeId::new_v7(),
        ControlId::new_v7(),
    );

    let high_idx = vocab.feasibility.len() - 1;
    let medium_idx = vocab
        .feasibility
        .iter()
        .position(|l| l == "Medium")
        .expect("Medium");
    let vlow_idx = 0;

    apply(
        &mut ws,
        &Edit::AddAsset {
            id: asset_id,
            name: "Firmware".into(),
            properties: (0..vocab.properties.len()).collect(),
        },
    );
    apply(
        &mut ws,
        &Edit::AddDamage {
            id: damage_id,
            name: "ECU compromised".into(),
        },
    );
    let safety = vocab
        .impact_categories
        .iter()
        .position(|c| c.label.eq_ignore_ascii_case("safety"))
        .expect("safety");
    apply(
        &mut ws,
        &Edit::SetImpact {
            damage: damage_id,
            category: safety,
            level: Some(vocab.impact_categories[safety].levels.len() - 1),
        },
    );
    apply(
        &mut ws,
        &Edit::AddThreat {
            id: threat_id,
            name: "Flash tampering".into(),
            asset: asset_id,
            category: 0,
        },
    );
    apply(
        &mut ws,
        &Edit::LinkDamage {
            threat: threat_id,
            damage: damage_id,
            linked: true,
        },
    );
    apply(
        &mut ws,
        &Edit::NewTree {
            id: tree_id,
            root: "Compromise ECU".into(),
            threat: Some(threat_id),
        },
    );
    apply(
        &mut ws,
        &Edit::ReplaceTree {
            tree: tree_id,
            outline: "OR Compromise ECU\n  - Flash unsigned firmware\n".into(),
        },
    );
    let tree_idx = ws.project().attack_tree_idx(&tree_id).expect("tree idx");
    let step_id = ws
        .project()
        .attack_tree(tree_idx)
        .expect("tree")
        .iter()
        .find(|(_, _, n)| n.label() == "Flash unsigned firmware")
        .expect("step")
        .1;
    apply(
        &mut ws,
        &Edit::SetStepFeasibility {
            tree: tree_id,
            node: step_id,
            level: Some(high_idx),
        },
    );

    apply(
        &mut ws,
        &Edit::NewFragment {
            id: bypass_id,
            root: "Bypass Secure Boot".into(),
        },
    );
    apply(
        &mut ws,
        &Edit::ReplaceTree {
            tree: bypass_id,
            outline: "OR Bypass Secure Boot\n  - Glitch voltage rail\n".into(),
        },
    );
    let b_idx = ws.project().attack_tree_idx(&bypass_id).expect("b idx");
    let bypass_leaf = ws
        .project()
        .attack_tree(b_idx)
        .expect("b tree")
        .iter()
        .find(|(_, _, n)| n.label() == "Glitch voltage rail")
        .expect("bypass leaf")
        .1;
    apply(
        &mut ws,
        &Edit::SetStepFeasibility {
            tree: bypass_id,
            node: bypass_leaf,
            level: Some(medium_idx),
        },
    );

    apply(
        &mut ws,
        &Edit::AddControl {
            id: ctrl,
            name: "Secure boot".into(),
        },
    );
    apply(
        &mut ws,
        &Edit::SetControlBypass {
            control: ctrl,
            tree: Some(bypass_id),
        },
    );
    apply(
        &mut ws,
        &Edit::SetStepMitigation {
            tree: tree_id,
            node: step_id,
            control: ctrl,
            level: Some(vlow_idx),
        },
    );

    Fixture {
        ws,
        threat_id,
        tree_id,
        step_id,
        bypass_id,
        bypass_leaf,
        ctrl,
    }
}

#[test]
fn bypass_ceiling_surfaces_everywhere_as_bypass_level() {
    let fx = setup_bypass_fixture();
    let catalog = Catalog::load(Language::En);
    let base_without_bypass = {
        let mut copy = Workspace::new(fx.ws.project().clone(), 0);
        apply(
            &mut copy,
            &Edit::SetControlBypass {
                control: fx.ctrl,
                tree: None,
            },
        );
        copy.project().clone()
    };

    // 1. Step inspector_view explain lines & causes.
    let step_view = inspector_view(
        &fx.ws,
        Selection::TreeNode(fx.tree_id, fx.step_id),
        &catalog,
    )
    .expect("step inspector");
    let step_explain = step_view.explain.expect("step explain");
    assert!(
        step_explain
            .lines
            .iter()
            .any(|l| l == "Secure boot: bypass Medium"),
        "expected 'Secure boot: bypass Medium', got {:?}",
        step_explain.lines
    );
    assert!(
        step_explain.lines.iter().all(|l| !l.contains("bypass:")),
        "must not put a colon inside 'bypass <Level>': {:?}",
        step_explain.lines
    );
    assert!(step_explain.causes.iter().any(|c| {
        c.role == catalog.tr("inspector-capped-by") && c.target == Selection::Control(fx.ctrl)
    }));
    assert!(step_explain.causes.iter().any(|c| {
        c.role == catalog.tr("inspector-bypass-tree") && c.target == Selection::Tree(fx.bypass_id)
    }));

    // 2. Tree explain causes include bypass tree.
    let tree_explain =
        explain_selection(&fx.ws, Selection::Tree(fx.tree_id), &catalog).expect("tree explain");
    assert!(tree_explain.causes.iter().any(|c| {
        c.role == catalog.tr("inspector-bypass-tree") && c.target == Selection::Tree(fx.bypass_id)
    }));

    // 3. threat_detail & threat inspector_view why + causes.
    let td = threat_detail(&fx.ws, fx.threat_id).expect("threat detail");
    assert!(
        td.why
            .iter()
            .any(|l| l.contains("bypass Medium") && !l.contains("bypass:")),
        "threat_detail why missing 'bypass Medium': {:?}",
        td.why
    );
    let threat_view =
        inspector_view(&fx.ws, Selection::Threat(fx.threat_id), &catalog).expect("threat view");
    let threat_explain = threat_view.explain.expect("threat explain");
    assert!(
        threat_explain
            .lines
            .iter()
            .any(|l| l.contains("bypass Medium") && !l.contains("bypass:")),
        "threat inspector why missing 'bypass Medium': {:?}",
        threat_explain.lines
    );
    assert!(threat_explain.causes.iter().any(|c| {
        c.role == catalog.tr("inspector-bypass-tree") && c.target == Selection::Tree(fx.bypass_id)
    }));

    // 4. risk_rows reduced_by (on + ceiling + reduced_by_text).
    let rows = risk_rows(&fx.ws);
    let risk_row = rows
        .iter()
        .find(|r| r.id == fx.threat_id)
        .expect("risk row");
    assert_eq!(risk_row.reduced_by.len(), 1);
    assert_eq!(risk_row.reduced_by[0].on, "bypass Medium");
    assert_eq!(risk_row.reduced_by[0].ceiling, "Medium");
    let rb_text = reduced_by_text(risk_row).expect("reduced_by_text");
    assert!(rb_text.contains("bypass Medium"), "{rb_text}");

    // 5. control_uses & ControlDetail::bypass.
    let cd = control_detail(&fx.ws, fx.ctrl).expect("control detail");
    assert!(
        cd.steps.iter().any(|s| s.detail.contains("bypass Medium")),
        "control_uses missing 'bypass Medium': {:?}",
        cd.steps
    );
    let bypass_row = cd.bypass.expect("ControlDetail::bypass");
    assert_eq!(bypass_row.target, Selection::Tree(fx.bypass_id));
    assert_eq!(bypass_row.label, "Bypass Secure Boot");
    assert_eq!(bypass_row.detail, "Medium");

    // 6. diff_lines reports "bypass tree changed".
    let cmp = comparison(&base_without_bypass, "base", fx.ws.project()).expect("comparison");
    assert!(
        cmp.lines
            .iter()
            .any(|l| l.text.contains("bypass tree changed")),
        "expected 'bypass tree changed' in {:?}",
        cmp.lines
    );

    // 7. Canvas shield capped tooltip & access sentence.
    let scene = tree_scene(&fx.ws, fx.tree_id, true).expect("residual tree_scene");
    let step_pos = (0..scene.len())
        .map(|i| u32::try_from(i).expect("u32"))
        .find(|&v| scene.label(v) == "Flash unsigned firmware")
        .expect("step on scene");
    let style = scene.style(step_pos).expect("style");
    assert_eq!(style.capped.as_deref(), Some("Secure boot (bypass Medium)"));
    let acc = describe(&scene, step_pos).expect("describe");
    assert_eq!(acc.capped.as_deref(), Some("Secure boot (bypass Medium)"));
    assert!(
        sentence(&acc, false).contains("Capped by Secure boot (bypass Medium)."),
        "{}",
        sentence(&acc, false)
    );

    // 8. Slint view::inspector ceilings.
    let ui = UiState {
        selected_threat: Some(fx.threat_id),
        selected_tree: Some(fx.tree_id),
        selected_node: Some(fx.step_id),
        show_residual: true,
        ..UiState::default()
    };
    let node_insp = view::inspector(&fx.ws, &ui).expect("view::inspector");
    assert!(
        node_insp
            .ceilings
            .iter()
            .any(|c| c == "Secure boot: bypass Medium"),
        "expected 'Secure boot: bypass Medium', got {:?}",
        node_insp.ceilings
    );
}

#[test]
fn control_bypass_relation_picker_remove_and_duplicate() {
    let mut fx = setup_bypass_fixture();
    let catalog = Catalog::load(Language::En);

    // Candidates must include only TreeRole::Fragment trees, never Threat trees.
    let cand = candidates(fx.ws.project(), RelationKey::CONTROL_BYPASS);
    assert_eq!(cand, vec![Selection::Tree(fx.bypass_id)]);
    assert!(!cand.contains(&Selection::Tree(fx.tree_id)));

    // Reverse relation and label.
    assert_eq!(
        reverse_relation_label(RelationKey::CONTROL_BYPASS, &catalog),
        "Bypass tree for"
    );
    assert_eq!(
        reverse_related(
            fx.ws.project(),
            RelationKey::CONTROL_BYPASS,
            Selection::Tree(fx.bypass_id)
        ),
        vec![Selection::Control(fx.ctrl)]
    );

    // Unchanged SetControlBypass emits no ops; changing it emits one op.
    assert!(
        Edit::SetControlBypass {
            control: fx.ctrl,
            tree: Some(fx.bypass_id),
        }
        .to_ops(fx.ws.project(), "tester", 1)
        .expect("unchanged")
        .is_empty()
    );
    assert!(
        !Edit::SetControlBypass {
            control: fx.ctrl,
            tree: None,
        }
        .to_ops(fx.ws.project(), "tester", 1)
        .expect("changed")
        .is_empty()
    );

    // DuplicateControl does not copy bypass.
    let dup_id = ControlId::new_v7();
    apply(
        &mut fx.ws,
        &Edit::DuplicateControl {
            id: dup_id,
            source: fx.ctrl,
            suffix: "copy".into(),
        },
    );
    let dup_detail = control_detail(&fx.ws, dup_id).expect("dup detail");
    assert_eq!(dup_detail.bypass, None);

    // MultiLinkPicker toggles CONTROL_BYPASS cleanly on the duplicate control.
    let from = Selection::Control(dup_id);
    let mut picker = MultiLinkPicker::open(fx.ws.project(), RelationKey::CONTROL_BYPASS, from);
    assert_eq!(picker.options().len(), 1);
    assert!(picker.chips().is_empty());
    picker.move_cursor(0);
    let pick_edit = picker.toggle_active().expect("pick edit");
    assert_eq!(
        pick_edit,
        Edit::SetControlBypass {
            control: dup_id,
            tree: Some(fx.bypass_id),
        }
    );
    apply(&mut fx.ws, &pick_edit);
    assert_eq!(
        fx.ws.undo_label().expect("undo label").imperative(),
        "change bypass tree of security control 'Secure boot copy'"
    );
    assert_eq!(
        related(fx.ws.project(), RelationKey::CONTROL_BYPASS, from),
        vec![Selection::Tree(fx.bypass_id)]
    );
    let mut linked_picker =
        MultiLinkPicker::open(fx.ws.project(), RelationKey::CONTROL_BYPASS, from);
    assert_eq!(linked_picker.chips().len(), 1);
    linked_picker.move_cursor(0);
    let unpick_edit = linked_picker.toggle_active().expect("unpick edit");
    assert_eq!(
        unpick_edit,
        Edit::SetControlBypass {
            control: dup_id,
            tree: None,
        }
    );
    apply(&mut fx.ws, &unpick_edit);
    assert!(related(fx.ws.project(), RelationKey::CONTROL_BYPASS, from).is_empty());

    // RemoveControl removes a control with a bypass in one undo step and undoes cleanly.
    let before_remove = fx.ws.project().clone();
    apply(&mut fx.ws, &Edit::RemoveControl { control: fx.ctrl });
    assert!(fx.ws.project().security_control_idx(&fx.ctrl).is_none());
    assert!(
        fx.ws.project().attack_tree_idx(&fx.bypass_id).is_some(),
        "removing the control keeps the fragment tree"
    );
    fx.ws.undo().expect("undo RemoveControl");
    assert!(
        fx.ws.project() == &before_remove,
        "one undo must restore the control, its bypass link, and its step mitigations"
    );
}

#[test]
fn create_bypass_from_selection_copies_tree_or_subtree_and_undoes_cleanly() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let (tree_id, ctrl) = (TreeId::new_v7(), ControlId::new_v7());
    apply(
        &mut ws,
        &Edit::NewTree {
            id: tree_id,
            root: "Attack Root".into(),
            threat: None,
        },
    );
    apply(
        &mut ws,
        &Edit::ReplaceTree {
            tree: tree_id,
            outline: "OR Attack Root\n  AND Subgate\n    - Leaf A\n    - Leaf B\n  - Leaf C\n"
                .into(),
        },
    );
    let t_idx = ws.project().attack_tree_idx(&tree_id).expect("tree idx");
    let subgate = ws
        .project()
        .attack_tree(t_idx)
        .expect("tree")
        .iter()
        .find(|(_, _, n)| n.label() == "Subgate")
        .expect("subgate")
        .1;
    let leaf_a = ws
        .project()
        .attack_tree(t_idx)
        .expect("tree")
        .iter()
        .find(|(_, _, n)| n.label() == "Leaf A")
        .expect("leaf_a")
        .1;
    apply(
        &mut ws,
        &Edit::SetStepFeasibility {
            tree: tree_id,
            node: leaf_a,
            level: Some(2),
        },
    );
    apply(
        &mut ws,
        &Edit::AddControl {
            id: ctrl,
            name: "Firewall".into(),
        },
    );

    // Invalid selection and missing control return Missing.
    let bad_sel = Edit::CreateBypassFromSelection {
        control: ctrl,
        source: Selection::Control(ctrl),
        id: TreeId::new_v7(),
    };
    assert_eq!(
        bad_sel.to_ops(ws.project(), "tester", 1),
        Err(EditError::Missing)
    );
    let missing_ctrl = Edit::CreateBypassFromSelection {
        control: ControlId::new_v7(),
        source: Selection::Tree(tree_id),
        id: TreeId::new_v7(),
    };
    assert_eq!(
        missing_ctrl.to_ops(ws.project(), "tester", 1),
        Err(EditError::Missing)
    );

    // From Selection::TreeNode(subgate): creates a Fragment tree of Subgate + Leaf A + Leaf B.
    let source_snapshot = ws
        .project()
        .attack_tree(ws.project().attack_tree_idx(&tree_id).expect("t"))
        .expect("tree")
        .clone();
    let before_sub = ws.project().clone();
    let frag_sub = TreeId::new_v7();
    let edit_sub = Edit::CreateBypassFromSelection {
        control: ctrl,
        source: Selection::TreeNode(tree_id, subgate),
        id: frag_sub,
    };
    assert_eq!(edit_sub.created(), Some(Selection::Control(ctrl)));
    assert!(
        !edit_sub
            .to_ops(ws.project(), "tester", 1)
            .expect("create bypass ops")
            .is_empty()
    );
    apply(&mut ws, &edit_sub);

    let sub_idx = ws
        .project()
        .attack_tree_idx(&frag_sub)
        .expect("fragment created");
    assert_eq!(
        ws.project().attack_tree_role(sub_idx),
        Some(TreeRole::Fragment)
    );
    let sub_tree = ws.project().attack_tree(sub_idx).expect("fragment tree");
    assert_eq!(sub_tree.len(), 3);
    assert_eq!(
        sub_tree.get(sub_tree.root()).map(Node::label),
        Some("Subgate")
    );
    assert_eq!(
        ws.project()
            .security_control(ws.project().security_control_idx(&ctrl).expect("c"))
            .expect("ctrl")
            .bypass(),
        Some(sub_idx)
    );
    // Source tree stays untouched.
    assert_eq!(
        ws.project()
            .attack_tree(ws.project().attack_tree_idx(&tree_id).expect("t"))
            .expect("src"),
        &source_snapshot
    );
    ws.undo().expect("undo CreateBypassFromSelection subtree");
    assert!(ws.project() == &before_sub);

    // From Selection::Tree(tree_id): copies the whole tree root.
    let before_whole = ws.project().clone();
    let frag_whole = TreeId::new_v7();
    apply(
        &mut ws,
        &Edit::CreateBypassFromSelection {
            control: ctrl,
            source: Selection::Tree(tree_id),
            id: frag_whole,
        },
    );
    let whole_idx = ws
        .project()
        .attack_tree_idx(&frag_whole)
        .expect("whole fragment");
    assert_eq!(
        ws.project().attack_tree_role(whole_idx),
        Some(TreeRole::Fragment)
    );
    assert_eq!(
        ws.project().attack_tree(whole_idx).expect("tree").len(),
        source_snapshot.len()
    );
    ws.undo().expect("undo CreateBypassFromSelection tree");
    assert!(ws.project() == &before_whole);
}

#[test]
fn tree_scene_hatches_control_off_bypass_trees_unless_active_or_fragment_used() {
    let mut fx = setup_bypass_fixture();
    let view = Viewport::new(Size::new(1600.0, 700.0));

    let assert_control_off_hatched = |ws: &Workspace, expected_hatched: bool| {
        let scene = tree_scene(ws, fx.bypass_id, true).expect("bypass scene");
        let list = render(&scene, &view, &RenderOptions::default());
        for i in 0..scene.len() {
            let v = u32::try_from(i).expect("u32");
            let acc = describe(&scene, v).expect("describe");
            let spoken = sentence(&acc, false);
            if expected_hatched {
                assert_eq!(acc.excluded.as_deref(), Some("control off"));
                assert!(spoken.contains("Excluded: control off."), "{spoken}");
            } else {
                assert_eq!(acc.excluded, None);
                assert!(!spoken.contains("control off"), "{spoken}");
            }
        }
        let has_control_off_text = list.items.iter().any(|it| {
            matches!(
                it,
                Item::Text {
                    text,
                    paint: Paint::Graphite,
                    bold: true,
                    ..
                } if text == "control off"
            )
        });
        let hatch_lines = list
            .items
            .iter()
            .filter(|it| matches!(it, Item::Polyline { points, .. } if points.len() == 2))
            .count();
        assert_eq!(has_control_off_text, expected_hatched);
        if expected_hatched {
            assert!(hatch_lines >= 4, "expected hatch lines, got {hatch_lines}");
        } else {
            assert_eq!(hatch_lines, 0, "expected no hatch lines, got {hatch_lines}");
        }
    };

    // Planned -> unhatched.
    assert_control_off_hatched(&fx.ws, false);

    // Implemented -> unhatched.
    apply(
        &mut fx.ws,
        &Edit::SetControlStatus {
            control: fx.ctrl,
            status: ControlStatus::Implemented,
        },
    );
    assert_control_off_hatched(&fx.ws, false);

    // Off (and no UseFragment users) -> hatched with "control off".
    apply(
        &mut fx.ws,
        &Edit::SetControlStatus {
            control: fx.ctrl,
            status: ControlStatus::Off,
        },
    );
    assert_control_off_hatched(&fx.ws, true);

    // If a step also references this fragment via UseFragment, it is NOT hatched as control off.
    let host_id = TreeId::new_v7();
    let host_root = NodeId::new_v7();
    let host_ref = NodeId::new_v7();
    let mut host = AttackTree::new(host_root, Node::gate(GateKind::Or, "Host")).expect("host");
    host.add_child(host.root(), host_ref, Node::step("Use frag"))
        .expect("child");
    fx.ws
        .apply(
            vec![
                Op::AddAttackTree {
                    role: TreeRole::Threat,
                    id: host_id,
                    tree: host,
                    step_feasibility: Vec::new(),
                    provenance: prov(),
                },
                Op::UseFragment {
                    tree: host_id,
                    node: host_ref,
                    fragment: fx.bypass_id,
                    provenance: prov(),
                },
            ],
            Origin::Edit,
        )
        .expect("use fragment");
    assert_control_off_hatched(&fx.ws, false);

    // Detach UseFragment -> returns to hatched "control off", and an excluded step preserves its
    // applicability reason while its sibling shows "control off".
    apply(
        &mut fx.ws,
        &Edit::UnuseFragment {
            tree: host_id,
            node: host_ref,
        },
    );
    assert_control_off_hatched(&fx.ws, true);

    let cond = ConditionId::new_v7();
    fx.ws
        .apply(
            vec![
                Op::AddCondition {
                    id: cond,
                    name: "Debug port".into(),
                    description: "".into(),
                    present: false,
                    provenance: prov(),
                },
                Op::NeedStepCondition {
                    tree: fx.bypass_id,
                    node: fx.bypass_leaf,
                    condition: cond,
                    provenance: prov(),
                },
            ],
            Origin::Edit,
        )
        .expect("exclude bypass leaf");
    let mixed_scene = tree_scene(&fx.ws, fx.bypass_id, true).expect("mixed scene");
    let leaf_pos = (0..mixed_scene.len())
        .map(|i| u32::try_from(i).expect("u32"))
        .find(|&v| mixed_scene.label(v) == "Glitch voltage rail")
        .expect("leaf");
    let leaf_acc = describe(&mixed_scene, leaf_pos).expect("leaf acc");
    assert!(
        leaf_acc
            .excluded
            .as_deref()
            .is_some_and(|s| s.contains("Debug port")),
        "applicability exclusion must take precedence: {:?}",
        leaf_acc.excluded
    );
}
