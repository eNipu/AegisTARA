//! Headless GPUI tests for [`InspectorPanel`] explain-why derivations,
//! trace-spine reverse relations, `Selection::TreeNode` navigation, and the
//! `"go-to-cause"` command across themes and locales (WBX-06, WBX-11, UI-15).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::rc::Rc;
use std::sync::Arc;

use aegis_client::command_table::command;
use aegis_client::commands::Edit;
use aegis_client::explorer::{ExplorerKey, Folder, Owner};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::relations::RelationKey;
use aegis_client::settings::Appearance;
use aegis_client::view::{Selection, TableId};
use aegis_model::{
    AssetId, AttackTree, ComponentId, ConditionId, ControlId, DamageId, ElementId, ElementKind,
    ElementRef, GateKind, GoalId, ItemRef, Methodology, Node, NodeId, Op, Project, Provenance,
    RiskTreatment, SharedStepId, StepTarget, ThreatId, TreeId, TreeRole,
};
use aegis_workbench::app::{InspectorPanel, Workbench, apply_appearance, bind_all};
use gpui::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};

fn prov() -> Provenance {
    Provenance::human("inspector-test", 1_792_956_600_000)
}

fn go_to_cause_keys() -> String {
    command("go-to-cause")
        .and_then(|c| c.keys_for(cfg!(target_os = "macos")))
        .expect("go-to-cause has shortcut keys")
}

struct Fixture {
    project: Project,
    sys: ElementId,
    comp: ComponentId,
    asset: AssetId,
    excluded_asset: AssetId,
    dmg: DamageId,
    threat: ThreatId,
    tree_id: TreeId,
    step_deciding: NodeId,
    ctrl: ControlId,
    goal: GoalId,
    cond: ConditionId,
}

fn build_fixture() -> Fixture {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin methodology"));
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");
    let safety = m
        .impact_category("safety")
        .or_else(|| m.impact_category("safety-health"))
        .expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let high = m.feasibility_level("high").expect("high");
    let low = m.feasibility_level("low").expect("low");
    let very_low = m.feasibility_level("very-low").expect("very-low");

    let mut project = Project::new(Arc::clone(&m));
    let sys = ElementId::new_v7();
    let comp = ComponentId::new_v7();
    let asset = AssetId::new_v7();
    let excluded_asset = AssetId::new_v7();
    let dmg = DamageId::new_v7();
    let threat = ThreatId::new_v7();
    let tree_id = TreeId::new_v7();
    let root = NodeId::new_v7();
    let step_deciding = NodeId::new_v7();
    let step_other = NodeId::new_v7();
    let ctrl = ControlId::new_v7();
    let goal = GoalId::new_v7();
    let cond = ConditionId::new_v7();

    let mut tree =
        AttackTree::new(root, Node::gate(GateKind::Or, "Spoof brake command")).expect("root");
    let r = tree.root();
    tree.add_child(r, step_deciding, Node::step("Inject forged CAN frame"))
        .expect("step_deciding");
    tree.add_child(r, step_other, Node::step("Extract key from HSM"))
        .expect("step_other");

    for op in [
        Op::AddElement {
            id: sys,
            kind: ElementKind::System,
            name: "Braking".into(),
            provenance: prov(),
        },
        Op::AddComponent {
            id: comp,
            name: "Brake ECU".into(),
            parent: None,
            provenance: prov(),
        },
        Op::SetComponentSystem {
            id: comp,
            system: Some(sys),
            provenance: prov(),
        },
        Op::AddAsset {
            id: asset,
            name: "Brake command".into(),
            properties: [integrity, authenticity].into_iter().collect(),
            provenance: prov(),
        },
        Op::SetAssetComponent {
            asset,
            component: Some(comp),
            provenance: prov(),
        },
        Op::AddAsset {
            id: excluded_asset,
            name: "Debug UART".into(),
            properties: [integrity].into_iter().collect(),
            provenance: prov(),
        },
        Op::SetAssetComponent {
            asset: excluded_asset,
            component: Some(comp),
            provenance: prov(),
        },
        Op::AddDamageScenario {
            id: dmg,
            name: "Unintended braking loss".into(),
            impacts: vec![(safety, severe)],
            provenance: prov(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tree_id,
            tree,
            step_feasibility: vec![(step_deciding, high), (step_other, very_low)],
            provenance: prov(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree_id, step_deciding),
            via: Some(ItemRef::Component(comp)),
            provenance: prov(),
        },
        Op::AddSecurityControl {
            id: ctrl,
            name: "SecOC CMAC authentication".into(),
            provenance: prov(),
        },
        Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Component(comp),
            provenance: prov(),
        },
        Op::SetStepMitigation {
            tree: tree_id,
            node: step_deciding,
            control: ctrl,
            feasibility: Some(low),
            provenance: prov(),
        },
        Op::AddCybersecurityGoal {
            id: goal,
            name: "Ensure brake command authenticity".into(),
            controls: vec![ctrl],
            provenance: prov(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Spoofed brake command".into(),
            asset,
            category: spoofing,
            catalog_refs: Vec::new(),
            damages: vec![dmg],
            provenance: prov(),
        },
        Op::SetThreatAttackTree {
            threat,
            tree: Some(tree_id),
            provenance: prov(),
        },
        Op::SetRiskTreatment {
            threat,
            treatment: Some(RiskTreatment::Reduce),
            provenance: prov(),
        },
        Op::SetThreatGoal {
            threat,
            goal: Some(goal),
            provenance: prov(),
        },
        Op::AddCondition {
            id: cond,
            name: "Development harness fitted".into(),
            description: "Only active on bench units".into(),
            present: false,
            provenance: prov(),
        },
        Op::NeedCondition {
            element: ElementRef::Asset(excluded_asset),
            condition: cond,
            provenance: prov(),
        },
        Op::SetDescription {
            target: ElementRef::Threat(threat),
            text: "CAN bus injection against braking".into(),
            provenance: prov(),
        },
    ] {
        project.apply(op).expect("fixture op applies");
    }

    Fixture {
        project,
        sys,
        comp,
        asset,
        excluded_asset,
        dmg,
        threat,
        tree_id,
        step_deciding,
        ctrl,
        goal,
        cond,
    }
}

fn open_workbench(
    cx: &mut TestAppContext,
    language: Language,
    appearance: Appearance,
) -> (
    Entity<Workbench>,
    Entity<InspectorPanel>,
    &mut VisualTestContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        bind_all(cx);
    });
    let catalog = Rc::new(Catalog::load(language));
    let (wb, vcx) = cx.add_window_view(|window, cx| {
        apply_appearance(appearance, window, cx);
        Workbench::new(catalog, appearance, None, window, cx)
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let inspector = vcx.read_entity(&wb, |w, _| w.inspector().clone());
    (wb, inspector, vcx)
}

#[gpui::test]
fn risk_table_selection_and_go_to_cause_chain_jumps_to_step_and_capping_control(
    cx: &mut TestAppContext,
) {
    let fx = build_fixture();
    let (wb, inspector, vcx) = open_workbench(cx, Language::En, Appearance::Light);
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(fx.project, "Inspector fixture", None, window, cx);
    });

    // Open the Risks table in the center dock and select the first risk row via keyboard navigation.
    let risks_key = ExplorerKey::Folder(Owner::System(fx.sys), Folder::Risks);
    wb.update_in(vcx, |w, window, cx| w.open(risks_key, window, cx));
    let risks_tab = vcx.read_entity(&wb, |w, _| {
        w.table_tab(risks_key).expect("Risks TableTab is open")
    });
    assert_eq!(
        vcx.read_entity(&risks_tab, |t, _| t.table()),
        TableId::Risks
    );
    risks_tab.update_in(vcx, |t, window, cx| {
        assert!(t.handle_grid_key("down", false, window, cx));
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    // Selecting the risk row populates InspectorPanel with fields, explain-why, and primary_cause.
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Threat(fx.threat)));
        let view = i.view().expect("inspector view populated");
        assert_eq!(view.systems, vec!["Braking".to_owned()]);
        assert_eq!(view.description, "CAN bus injection against braking");
        assert!(view.fields.iter().filter(|f| f.tone.is_some()).count() >= 2);
        assert!(!i.relation_rows().is_empty());
        let explain = i.explain().expect("threat has explain-why");
        assert!(!explain.lines.is_empty());
        assert_eq!(
            explain.causes.first().map(|c| c.target),
            Some(Selection::TreeNode(fx.tree_id, fx.step_deciding))
        );
        assert_eq!(
            i.primary_cause(),
            Some(Selection::TreeNode(fx.tree_id, fx.step_deciding))
        );
    });

    let tree_key = ExplorerKey::Element(
        Owner::System(fx.sys),
        Folder::Trees,
        Selection::Tree(fx.tree_id),
    );
    assert!(!vcx.read_entity(&wb, |w, cx| w.has_tab(tree_key, cx)));

    // Run "go-to-cause" via keyboard shortcut -> jumps to Selection::TreeNode and opens the tree tab.
    vcx.update(|window, cx| {
        let fh = wb.read(cx).inspector().focus_handle(cx);
        window.focus(&fh, cx);
    });
    vcx.simulate_keystrokes(&go_to_cause_keys());
    vcx.run_until_parked();
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(vcx.read_entity(&wb, |w, cx| w.has_tab(tree_key, cx)));
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.state().selected),
        Some(tree_key)
    );
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(
            i.selection(),
            Some(Selection::TreeNode(fx.tree_id, fx.step_deciding))
        );
        let explain = i.explain().expect("deciding step has explain-why");
        assert!(
            explain
                .lines
                .iter()
                .any(|l| l.contains("SecOC CMAC authentication"))
        );
        assert_eq!(i.primary_cause(), Some(Selection::Control(fx.ctrl)));
        let via_row = i
            .relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::STEP_VIA)
            .expect("STEP_VIA relation present");
        assert_eq!(via_row.chips[0].target, Selection::Component(fx.comp));
    });

    // Chain "go-to-cause" a second time from the capped step -> jumps to the capping Control.
    let second = inspector.update_in(vcx, |i, window, cx| i.go_to_cause(window, cx));
    assert_eq!(second, Some(Selection::Control(fx.ctrl)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Control(fx.ctrl)));
        let rev_steps = i
            .reverse_relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::STEP_MITIGATIONS)
            .expect("control lists mitigated steps in reverse relations");
        assert_eq!(
            rev_steps.chips[0].target,
            Selection::TreeNode(fx.tree_id, fx.step_deciding)
        );
        let rev_goals = i
            .reverse_relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::GOAL_CONTROLS)
            .expect("control lists supporting goals in reverse relations");
        assert_eq!(rev_goals.chips[0].target, Selection::Goal(fx.goal));
    });
}

#[gpui::test]
fn reverse_trace_spine_neighbours_on_component_and_damage_navigate_cleanly(
    cx: &mut TestAppContext,
) {
    let fx = build_fixture();
    let (wb, inspector, vcx) = open_workbench(cx, Language::En, Appearance::Light);
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(fx.project, "Reverse neighbours", None, window, cx);
    });

    // Select Component -> reverse relations list ASSET_COMPONENT, CONTROL_TARGETS, and STEP_VIA.
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Component(fx.comp)), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let step_target = vcx.read_entity(&inspector, |i, _| {
        let rev = i.reverse_relation_rows();
        let assets = rev
            .iter()
            .find(|r| r.key == RelationKey::ASSET_COMPONENT)
            .expect("ASSET_COMPONENT reverse row");
        assert!(
            assets
                .chips
                .iter()
                .any(|c| c.target == Selection::Asset(fx.asset))
        );
        let steps = rev
            .iter()
            .find(|r| r.key == RelationKey::STEP_VIA)
            .expect("STEP_VIA reverse row");
        steps.chips[0].target
    });
    assert_eq!(
        step_target,
        Selection::TreeNode(fx.tree_id, fx.step_deciding)
    );

    // Navigating to a reverse STEP_VIA chip jumps to the step without double-leasing.
    inspector.update_in(vcx, |i, window, cx| {
        i.navigate_to(step_target, window, cx);
    });
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(step_target));
    });

    // Select Damage -> reverse relations list THREAT_DAMAGES, and navigate_to jumps to the Threat.
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Damage(fx.dmg)), cx);
    });
    let threat_target = vcx.read_entity(&inspector, |i, _| {
        let threats = i
            .reverse_relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::THREAT_DAMAGES)
            .expect("THREAT_DAMAGES reverse row");
        threats.chips[0].target
    });
    assert_eq!(threat_target, Selection::Threat(fx.threat));

    inspector.update_in(vcx, |i, window, cx| {
        i.navigate_to(threat_target, window, cx);
    });
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Threat(fx.threat)));
    });
}

#[gpui::test]
fn excluded_element_shows_exclusion_causes_and_go_to_cause_jumps_to_off_condition(
    cx: &mut TestAppContext,
) {
    let fx = build_fixture();
    let (wb, inspector, vcx) = open_workbench(cx, Language::En, Appearance::Light);
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(fx.project, "Excluded fixture", None, window, cx);
    });

    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Asset(fx.excluded_asset)), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    vcx.read_entity(&inspector, |i, _| {
        let view = i.view().expect("excluded asset view");
        assert!(
            view.excluded_reason
                .as_deref()
                .is_some_and(|r| r.contains("Development harness fitted"))
        );
        assert_eq!(view.exclusion_causes.len(), 1);
        assert_eq!(
            view.exclusion_causes[0].target,
            Selection::Condition(fx.cond)
        );
        assert_eq!(i.primary_cause(), Some(Selection::Condition(fx.cond)));
    });

    // Running "go-to-cause" via Workbench::run jumps to the excluding off Condition.
    wb.update_in(vcx, |w, window, cx| w.run("go-to-cause", window, cx));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Condition(fx.cond)));
        let view = i.view().expect("condition view");
        assert_eq!(view.description, "Only active on bench units");
        assert!(i.reverse_relation_rows().iter().any(|r| {
            r.chips
                .iter()
                .any(|c| c.target == Selection::Asset(fx.excluded_asset))
        }));
    });
}

#[gpui::test]
fn inspector_renders_across_light_dark_themes_and_en_ja_pseudo_locales(cx: &mut TestAppContext) {
    for language in [Language::En, Language::Ja, Language::Pseudo] {
        let fx = build_fixture();
        let (wb, inspector, vcx) = open_workbench(cx, language, Appearance::Light);
        wb.update_in(vcx, |w, window, cx| {
            w.show_project(fx.project, "Theme/Locale test", None, window, cx);
        });
        for appearance in [Appearance::Light, Appearance::Dark] {
            if appearance == Appearance::Dark {
                wb.update_in(vcx, |w, window, cx| w.run("toggle-theme", window, cx));
            }
            for sel in [
                Selection::Threat(fx.threat),
                Selection::TreeNode(fx.tree_id, fx.step_deciding),
                Selection::Control(fx.ctrl),
                Selection::Asset(fx.excluded_asset),
            ] {
                wb.update(vcx, |w, cx| w.select_element(Some(sel), cx));
                vcx.update(|window, cx| window.draw(cx).clear(cx));
                vcx.read_entity(&inspector, |i, _| {
                    let view = i.view().expect("view exists");
                    assert_eq!(view.selection, sel);
                    assert!(!view.kind.is_empty());
                });
            }
        }
    }
}

#[gpui::test]
fn shared_step_note_renders_above_fields_for_five_distinct_trees(cx: &mut TestAppContext) {
    let mut fx = build_fixture();
    let shared = SharedStepId::new_v7();
    let high = fx
        .project
        .methodology()
        .feasibility_level("high")
        .expect("high");
    for op in [
        Op::AddSharedStep {
            id: shared,
            name: "Reusable injection".into(),
            provenance: prov(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: Some(high),
            provenance: prov(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(fx.tree_id, fx.step_deciding),
            level: None,
            provenance: prov(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(fx.tree_id, fx.step_deciding),
            via: None,
            provenance: prov(),
        },
        Op::UseSharedStep {
            tree: fx.tree_id,
            node: fx.step_deciding,
            step: shared,
            provenance: prov(),
        },
    ] {
        fx.project.apply(op).expect("shared setup");
    }
    for index in 0..4 {
        let tree_id = TreeId::new_v7();
        let root = NodeId::new_v7();
        let step = NodeId::new_v7();
        let mut tree =
            AttackTree::new(root, Node::gate(GateKind::Or, format!("Tree {index}"))).expect("root");
        tree.add_child(tree.root(), step, Node::step("Reusable injection"))
            .expect("step");
        fx.project
            .apply(Op::AddAttackTree {
                role: Default::default(),
                id: tree_id,
                tree,
                step_feasibility: Vec::new(),
                provenance: prov(),
            })
            .expect("tree");
        fx.project
            .apply(Op::UseSharedStep {
                tree: tree_id,
                node: step,
                step: shared,
                provenance: prov(),
            })
            .expect("use shared");
    }

    let (wb, inspector, vcx) = open_workbench(cx, Language::En, Appearance::Light);
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(fx.project, "Shared note fixture", None, window, cx);
        w.select_element(Some(Selection::TreeNode(fx.tree_id, fx.step_deciding)), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.read_entity(&inspector, |i, _| {
        let view = i.view().expect("linked step view");
        assert_eq!(
            view.shared_note.as_deref(),
            Some("Shared step L00, used in 5 trees")
        );
        assert!(!view.fields.is_empty());
    });
    let note = vcx
        .debug_bounds("inspector-shared-step-note")
        .expect("note row rendered");
    let fields = vcx
        .debug_bounds("inspector-fields")
        .expect("fields rendered");
    assert!(
        note.bottom() <= fields.top(),
        "note {note:?} is not above fields {fields:?}"
    );
}

#[gpui::test]
fn control_inspector_creates_bypass_from_selection_with_read_only_and_undo_guards(
    cx: &mut TestAppContext,
) {
    let mut fx = build_fixture();
    let medium = fx
        .project
        .methodology()
        .feasibility_level("medium")
        .expect("medium");
    let medium_label = fx
        .project
        .methodology()
        .feasibility_level_label(medium)
        .expect("medium label")
        .to_owned();

    let src_tree_id = TreeId::new_v7();
    let src_root = NodeId::new_v7();
    let src_subgate = NodeId::new_v7();
    let src_step = NodeId::new_v7();
    let mut src_tree =
        AttackTree::new(src_root, Node::gate(GateKind::Or, "Outer root")).expect("r");
    let sub_idx = src_tree
        .add_child(
            src_tree.root(),
            src_subgate,
            Node::gate(GateKind::And, "Clone subgate"),
        )
        .expect("subgate");
    src_tree
        .add_child(sub_idx, src_step, Node::step("Replay bus frame"))
        .expect("step");
    fx.project
        .apply(Op::AddAttackTree {
            role: Default::default(),
            id: src_tree_id,
            tree: src_tree,
            step_feasibility: vec![(src_step, medium)],
            provenance: prov(),
        })
        .expect("add source tree");
    let src_snapshot = fx
        .project
        .attack_tree(fx.project.attack_tree_idx(&src_tree_id).expect("src idx"))
        .expect("src tree")
        .clone();

    let catalog = Catalog::load(Language::En);
    let bypass_label = catalog.tr("relation-control-bypass");

    let (wb, inspector, vcx) = open_workbench(cx, Language::En, Appearance::Light);
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(fx.project, "Bypass from selection", None, window, cx);
    });

    // Initially no tree or node has been selected, so creating fails cleanly with no mutation.
    wb.update(vcx, |w, cx| {
        assert_eq!(w.last_tree_selection(), None);
        w.select_element(Some(Selection::Control(fx.ctrl)), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let before_empty = vcx.read_entity(&wb, |w, _| w.workspace().project().clone());
    assert!(!inspector.update(vcx, |i, cx| i.create_bypass_from_selection(cx)));
    assert!(vcx.read_entity(&wb, |w, _| w.workspace().project() == &before_empty));
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.note().map(str::to_owned)),
        None
    );

    // 1. Select a tree node, then inspect the SecurityControl: last_tree_selection stays set,
    //    and the "Bypass tree" field + CONTROL_BYPASS relation row are displayed.
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::TreeNode(src_tree_id, src_subgate)), cx);
        w.select_element(Some(Selection::Control(fx.ctrl)), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let tree_sel = Some(Selection::TreeNode(src_tree_id, src_subgate));
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.last_tree_selection()),
        tree_sel
    );
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Control(fx.ctrl)));
        let view = i.view().expect("control view");
        assert!(view.fields.iter().any(|f| {
            f.label == bypass_label && f.value == catalog.tr("none") && f.tone.is_none()
        }));
        let row = i
            .relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::CONTROL_BYPASS)
            .expect("CONTROL_BYPASS relation row");
        assert_eq!(row.label, bypass_label);
        assert!(row.chips.is_empty());
    });

    // 3. When read-only is set, create_bypass_from_selection returns false and mutates nothing.
    wb.update(vcx, |w, cx| {
        w.set_read_only(Some("Read-only mode".to_owned()), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let before_ro = vcx.read_entity(&wb, |w, _| w.workspace().project().clone());
    assert!(!inspector.update(vcx, |i, cx| i.create_bypass_from_selection(cx)));
    assert!(vcx.read_entity(&wb, |w, _| w.workspace().project() == &before_ro));
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.note().map(str::to_owned)),
        Some("Read-only mode".to_owned())
    );

    wb.update(vcx, |w, cx| {
        w.set_read_only(None, cx);
    });
    let before_create = vcx.read_entity(&wb, |w, _| w.workspace().project().clone());

    // 2. Invoking create_bypass_from_selection creates a Fragment subtree copy attached to ctrl,
    //    updates Control's Bypass tree field + chip and step explain-why, and leaves source intact.
    assert!(inspector.update(vcx, |i, cx| i.create_bypass_from_selection(cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let frag_id = vcx.read_entity(&wb, |w, _| {
        let project = w.workspace().project();
        let ctrl_idx = project.security_control_idx(&fx.ctrl).expect("ctrl idx");
        let ctrl = project.security_control(ctrl_idx).expect("ctrl");
        let frag_idx = ctrl.bypass().expect("ctrl bypass attached");
        assert_eq!(project.attack_tree_role(frag_idx), Some(TreeRole::Fragment));
        let frag_tree = project.attack_tree(frag_idx).expect("fragment tree");
        assert_eq!(frag_tree.len(), 2);
        assert_eq!(
            frag_tree.get(frag_tree.root()).map(|n| n.label()),
            Some("Clone subgate")
        );
        let src = project
            .attack_tree(project.attack_tree_idx(&src_tree_id).expect("src idx"))
            .expect("src tree");
        assert_eq!(src, &src_snapshot);
        project.attack_tree_id(frag_idx).expect("fragment id")
    });

    vcx.read_entity(&inspector, |i, _| {
        let view = i.view().expect("updated control view");
        let field = view
            .fields
            .iter()
            .find(|f| f.label == bypass_label)
            .expect("bypass field");
        assert!(field.value.contains("Clone subgate"), "{field:?}");
        assert!(field.value.contains(&medium_label), "{field:?}");
        assert!(field.tone.is_some(), "{field:?}");
        let row = i
            .relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::CONTROL_BYPASS)
            .expect("CONTROL_BYPASS row");
        assert_eq!(row.chips.len(), 1);
        assert_eq!(row.chips[0].target, Selection::Tree(frag_id));
    });

    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::TreeNode(fx.tree_id, fx.step_deciding)), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.read_entity(&inspector, |i, _| {
        let explain = i.explain().expect("step explain-why");
        assert!(
            explain.lines.iter().any(|l| l.contains("bypass ")),
            "expected bypass note in explain lines: {:?}",
            explain.lines
        );
    });

    // 4a. Selecting the created fragment tree then undoing restores the exact pre-creation project,
    //     removes the fragment, and clears last_tree_selection() because the remembered tree is gone.
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Tree(frag_id)), cx);
    });
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.last_tree_selection()),
        Some(Selection::Tree(frag_id))
    );
    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    vcx.read_entity(&wb, |w, _| {
        let project = w.workspace().project();
        assert!(project == &before_create);
        assert!(project.attack_tree_idx(&frag_id).is_none());
        let ctrl_idx = project.security_control_idx(&fx.ctrl).expect("ctrl idx");
        assert_eq!(
            project.security_control(ctrl_idx).expect("ctrl").bypass(),
            None
        );
        assert_eq!(w.last_tree_selection(), None);
    });

    // 4b. Deleting the remembered source tree also causes last_tree_selection() to return None.
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::TreeNode(src_tree_id, src_subgate)), cx);
        w.select_element(Some(Selection::Control(fx.ctrl)), cx);
    });
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.last_tree_selection()),
        tree_sel
    );
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(&Edit::RemoveTree { tree: src_tree_id }, cx));
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(vcx.read_entity(&wb, |w, _| w.last_tree_selection()), None);
    assert!(!inspector.update(vcx, |i, cx| i.create_bypass_from_selection(cx)));
}
