use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::configuration::{ExcludedCounts, configuration_view, excluded_counts};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::inspector::inspector_view;
use aegis_client::pages;
use aegis_client::relations::{self, RelationKey};
use aegis_client::table::table_rows;
use aegis_client::view::{ColumnKind, Page, PageGroup, PageLayout, Selection, TableId, UiState};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AssumptionId, AttackTree, ClaimId, ComponentId, ConditionId, DamageId, ElementRef,
    GateKind, GoalId, InterfaceId, Methodology, Node, NodeId, Op, Provenance, ThreatId, TreeId,
};

const NOW: u64 = 1_792_956_600_000;

fn prov() -> Provenance {
    Provenance::human("tester", NOW)
}

fn apply_edit(ws: &mut Workspace, edit: Edit) {
    let ops = edit
        .to_ops(ws.project(), "tester", NOW)
        .expect("edit to_ops");
    ws.apply(ops, Origin::Edit).expect("apply edit");
}

struct Fixture {
    ws: Workspace,
    component: ComponentId,
    interface: InterfaceId,
    asset: AssetId,
    threat: ThreatId,
    tree: TreeId,
    step_a: NodeId,
    goal: GoalId,
    claim: ClaimId,
    cf00: ConditionId,
    cf01: ConditionId,
}

fn fixture() -> Fixture {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");
    let safety = m
        .impact_category("safety")
        .or_else(|| m.impact_category("safety-health"))
        .expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let high = m.feasibility_level("high").expect("high");
    let iface_kind = m.interface_kinds().next().expect("interface kind");

    let mut ws = Workspace::empty(Arc::clone(&m));
    let component = ComponentId::new_v7();
    let interface = InterfaceId::new_v7();
    let asset = AssetId::new_v7();
    let damage = DamageId::new_v7();
    let threat = ThreatId::new_v7();
    let tree_id = TreeId::new_v7();
    let root = NodeId::new_v7();
    let step_a = NodeId::new_v7();
    let step_b = NodeId::new_v7();
    let goal = GoalId::new_v7();
    let claim = ClaimId::new_v7();
    let cf00 = ConditionId::new_v7();
    let cf01 = ConditionId::new_v7();

    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Compromise ECU")).expect("root");
    let r = tree.root();
    tree.add_child(r, step_a, Node::step("Probe debug header"))
        .expect("step_a");
    tree.add_child(r, step_b, Node::step("Replay CAN frame"))
        .expect("step_b");

    ws.apply(
        vec![
            Op::AddComponent {
                id: component,
                name: "Gateway ECU".into(),
                parent: None,
                provenance: prov(),
            },
            Op::AddInterface {
                id: interface,
                component,
                name: "Debug UART".into(),
                kind: iface_kind,
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset,
                name: "Firmware image".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: damage,
                name: "Loss of steering".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree_id,
                tree,
                step_feasibility: vec![(step_a, high), (step_b, high)],
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: threat,
                name: "Flash tampered firmware".into(),
                asset,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![damage],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat,
                tree: Some(tree_id),
                provenance: prov(),
            },
            Op::AddCybersecurityGoal {
                id: goal,
                name: "Verify firmware signature".into(),
                controls: Vec::new(),
                provenance: prov(),
            },
            Op::AddCybersecurityClaim {
                id: claim,
                name: "Debug header disabled in production".into(),
                provenance: prov(),
            },
            Op::AddCondition {
                id: cf00,
                name: "Debug port fitted".into(),
                description: "JTAG header populated on board".into(),
                present: false,
                provenance: prov(),
            },
            Op::AddCondition {
                id: cf01,
                name: "OTA enabled".into(),
                description: "".into(),
                present: true,
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup fixture");

    Fixture {
        ws,
        component,
        interface,
        asset,
        threat,
        tree: tree_id,
        step_a,
        goal,
        claim,
        cf00,
        cf01,
    }
}

#[test]
fn condition_edits_apply_undo_and_validate() {
    let mut fx = fixture();

    // AddCondition
    let before_add = fx.ws.project().clone();
    let new_id = ConditionId::new_v7();
    let add = Edit::AddCondition {
        id: new_id,
        name: "  V2X module  ".into(),
    };
    assert_eq!(add.created(), Some(Selection::Condition(new_id)));
    apply_edit(&mut fx.ws, add);
    let idx = fx
        .ws
        .project()
        .condition_idx(&new_id)
        .expect("added condition");
    let added = fx.ws.project().condition(idx).expect("condition");
    assert_eq!(added.name(), "V2X module");
    assert!(added.present());

    fx.ws.undo().expect("undo add");
    assert!(fx.ws.project().condition_idx(&new_id).is_none());
    assert!(before_add.diff(fx.ws.project()).expect("diff").is_empty());

    // RenameCondition
    let before_rename = fx.ws.project().clone();
    apply_edit(
        &mut fx.ws,
        Edit::RenameCondition {
            condition: fx.cf00,
            name: "  JTAG header fitted  ".into(),
        },
    );
    let idx = fx.ws.project().condition_idx(&fx.cf00).expect("cf00");
    assert_eq!(
        fx.ws.project().condition(idx).expect("cf00").name(),
        "JTAG header fitted"
    );
    // Unchanged rename emits no ops.
    let noop_rename = Edit::RenameCondition {
        condition: fx.cf00,
        name: " JTAG header fitted ".into(),
    }
    .to_ops(fx.ws.project(), "tester", NOW)
    .expect("unchanged rename");
    assert!(noop_rename.is_empty());

    fx.ws.undo().expect("undo rename");
    let idx = fx.ws.project().condition_idx(&fx.cf00).expect("cf00");
    assert_eq!(
        fx.ws.project().condition(idx).expect("cf00").name(),
        "Debug port fitted"
    );
    assert!(
        before_rename
            .diff(fx.ws.project())
            .expect("diff")
            .is_empty()
    );

    // SetConditionPresent
    let before_toggle = fx.ws.project().clone();
    apply_edit(
        &mut fx.ws,
        Edit::SetConditionPresent {
            condition: fx.cf00,
            present: true,
        },
    );
    let idx = fx.ws.project().condition_idx(&fx.cf00).expect("cf00");
    assert!(fx.ws.project().condition(idx).expect("cf00").present());
    // Unchanged toggle emits no ops.
    let noop_toggle = Edit::SetConditionPresent {
        condition: fx.cf00,
        present: true,
    }
    .to_ops(fx.ws.project(), "tester", NOW)
    .expect("unchanged toggle");
    assert!(noop_toggle.is_empty());

    fx.ws.undo().expect("undo toggle");
    let idx = fx.ws.project().condition_idx(&fx.cf00).expect("cf00");
    assert!(!fx.ws.project().condition(idx).expect("cf00").present());
    assert!(
        before_toggle
            .diff(fx.ws.project())
            .expect("diff")
            .is_empty()
    );

    // Validation: blank name -> EmptyName, unknown condition -> Missing.
    let blank_add = Edit::AddCondition {
        id: ConditionId::new_v7(),
        name: "   ".into(),
    }
    .to_ops(fx.ws.project(), "tester", NOW);
    assert_eq!(blank_add, Err(EditError::EmptyName));

    let blank_rename = Edit::RenameCondition {
        condition: fx.cf00,
        name: " \t ".into(),
    }
    .to_ops(fx.ws.project(), "tester", NOW);
    assert_eq!(blank_rename, Err(EditError::EmptyName));

    let unknown = ConditionId::new_v7();
    assert_eq!(
        Edit::RenameCondition {
            condition: unknown,
            name: "Valid".into(),
        }
        .to_ops(fx.ws.project(), "tester", NOW),
        Err(EditError::Missing)
    );
    assert_eq!(
        Edit::SetConditionPresent {
            condition: unknown,
            present: false,
        }
        .to_ops(fx.ws.project(), "tester", NOW),
        Err(EditError::Missing)
    );
}

#[test]
fn configuration_page_metadata_and_selection() {
    let fx = fixture();
    assert_eq!(Page::ALL.len(), 19);
    assert_eq!(Page::ALL[9], Page::SharedBlocks);
    assert_eq!(Page::ALL[10], Page::AttackerProfile);
    assert_eq!(Page::SharedBlocks.group(), Some(PageGroup::Feasibility));
    assert_eq!(Page::SharedBlocks.key(), "shared-blocks");
    assert_eq!(Page::SharedBlocks.label(), "Shared building blocks");
    assert_eq!(Page::SharedBlocks.tables(), [TableId::SharedSteps]);
    assert_eq!(
        Page::SharedBlocks.layout(),
        PageLayout::Table(TableId::SharedSteps.columns())
    );
    assert_eq!(Page::AttackerProfile.group(), Some(PageGroup::Feasibility));
    assert_eq!(Page::AttackerProfile.key(), "attacker-profile");
    assert_eq!(Page::AttackerProfile.label(), "Attacker profile");
    assert!(Page::AttackerProfile.tables().is_empty());
    assert_eq!(Page::AttackerProfile.layout(), PageLayout::Custom);
    assert_eq!(Page::Configuration.group(), Some(PageGroup::ItemDefinition));
    assert_eq!(Page::Configuration.key(), "configuration");
    assert_eq!(Page::Configuration.label(), "Configuration");
    assert!(Page::Configuration.tables().is_empty());
    assert_eq!(
        pages::page_of(Selection::Condition(fx.cf00)),
        Page::Configuration
    );

    assert_eq!(Page::ALL[4], Page::Assumptions);
    assert_eq!(Page::Assumptions.group(), Some(PageGroup::ItemDefinition));
    assert_eq!(Page::Assumptions.key(), "assumptions");
    assert_eq!(Page::Assumptions.label(), "Assumptions");
    assert!(Page::Assumptions.tables().is_empty());
    assert_eq!(Page::Assumptions.layout(), PageLayout::Custom);
    let as_id = AssumptionId::new_v7();
    assert_eq!(
        pages::page_of(Selection::Assumption(as_id)),
        Page::Assumptions
    );

    let mut ui = UiState::default();
    assert_eq!(ui.selected_condition, None);
    pages::select(&mut ui, Selection::Condition(fx.cf00));
    assert_eq!(ui.selected_condition, Some(fx.cf00));
    assert_eq!(ui.selected_assumption, None);
    pages::select(&mut ui, Selection::Assumption(as_id));
    assert_eq!(ui.selected_assumption, Some(as_id));
    assert_eq!(ui.page, Page::Assumptions);
}

#[test]
fn configuration_view_counts_and_explainer() {
    let mut fx = fixture();
    let catalog = Catalog::load(Language::En);

    // Link CF00 (off) to the asset and to one step. Because the asset is
    // excluded, its threat and the threat's attack tree are also excluded.
    apply_edit(
        &mut fx.ws,
        Edit::SetConditionLink {
            element: ElementRef::Asset(fx.asset),
            condition: fx.cf00,
            linked: true,
        },
    );
    apply_edit(
        &mut fx.ws,
        Edit::SetStepConditionLink {
            tree: fx.tree,
            node: fx.step_a,
            condition: fx.cf00,
            linked: true,
        },
    );

    let view = configuration_view(&fx.ws, &catalog);
    assert_eq!(view.rows.len(), 2);
    assert_eq!(view.rows[0].id, fx.cf00);
    assert_eq!(view.rows[0].visual_id, "CF00");
    assert_eq!(view.rows[0].name, "Debug port fitted");
    assert_eq!(view.rows[0].description, "JTAG header populated on board");
    assert!(!view.rows[0].present);
    assert_eq!(view.rows[0].needed_by_elements, 1);
    assert_eq!(view.rows[0].needed_by_steps, 1);

    assert_eq!(view.rows[1].id, fx.cf01);
    assert_eq!(view.rows[1].visual_id, "CF01");
    assert!(view.rows[1].present);
    assert_eq!(view.rows[1].needed_by_elements, 0);
    assert_eq!(view.rows[1].needed_by_steps, 0);

    // Count excluded elements and leaf steps independently by hand.
    let project = fx.ws.project();
    let app = project.applicability();
    let manual_elements = app.excluded_elements().count();
    let mut manual_steps = 0;
    for (tree_id, node_id, _) in app.excluded_nodes() {
        let Some(t_idx) = project.attack_tree_idx(&tree_id) else {
            continue;
        };
        let Some(tree) = project.attack_tree(t_idx) else {
            continue;
        };
        if tree
            .iter()
            .any(|(_, nid, node)| nid == node_id && node.gate_kind().is_none())
        {
            manual_steps += 1;
        }
    }
    assert!(manual_elements > 0);
    assert!(manual_steps > 0);
    assert_eq!(
        view.excluded,
        ExcludedCounts {
            elements: manual_elements,
            steps: manual_steps,
        }
    );
    assert_eq!(excluded_counts(project), view.excluded);
    let total = manual_elements + manual_steps;
    assert_eq!(
        view.explainer,
        format!(
            "Switching a condition off excludes the elements and steps that need it. {total} excluded."
        )
    );

    // Switching every condition on clears exclusions and uses the -none message.
    apply_edit(
        &mut fx.ws,
        Edit::SetConditionPresent {
            condition: fx.cf00,
            present: true,
        },
    );
    let all_on = configuration_view(&fx.ws, &catalog);
    assert_eq!(
        all_on.excluded,
        ExcludedCounts {
            elements: 0,
            steps: 0
        }
    );
    assert_eq!(
        all_on.explainer,
        "Switching a condition off excludes the elements and steps that need it. Nothing is excluded."
    );
}

#[test]
fn inspector_candidates_and_table_condition_columns() {
    let mut fx = fixture();
    let catalog = Catalog::load(Language::En);
    let vocab = pages::vocabulary(fx.ws.project().methodology());

    let eight_kinds = [
        (Selection::Asset(fx.asset), RelationKey::ASSET_CONDITIONS),
        (
            Selection::Component(fx.component),
            RelationKey::COMPONENT_CONDITIONS,
        ),
        (
            Selection::Interface(fx.interface),
            RelationKey::INTERFACE_CONDITIONS,
        ),
        (Selection::Threat(fx.threat), RelationKey::THREAT_CONDITIONS),
        (Selection::Tree(fx.tree), RelationKey::TREE_CONDITIONS),
        (Selection::Goal(fx.goal), RelationKey::GOAL_CONDITIONS),
        (Selection::Claim(fx.claim), RelationKey::CLAIM_CONDITIONS),
        (
            Selection::TreeNode(fx.tree, fx.step_a),
            RelationKey::STEP_CONDITIONS,
        ),
    ];

    for (sel, key) in eight_kinds {
        let view = inspector_view(&fx.ws, sel, &catalog).expect("inspector view");
        assert!(
            view.relations.iter().any(|r| r.key == key),
            "inspector for {sel:?} missing relation {key:?}"
        );
        let cands = relations::candidates(fx.ws.project(), key);
        assert!(cands.contains(&Selection::Condition(fx.cf00)));
        assert!(cands.contains(&Selection::Condition(fx.cf01)));
    }

    let four_tables = [
        (TableId::Assets, RelationKey::ASSET_CONDITIONS),
        (TableId::Threats, RelationKey::THREAT_CONDITIONS),
        (TableId::Goals, RelationKey::GOAL_CONDITIONS),
        (TableId::Claims, RelationKey::CLAIM_CONDITIONS),
    ];
    for (table, key) in four_tables {
        let col = table
            .columns()
            .iter()
            .find(|c| c.key == "conditions")
            .expect("conditions column");
        assert!(!col.visible, "{table:?} conditions column must be hidden");
        assert_eq!(col.kind, ColumnKind::Links(key));
    }

    apply_edit(
        &mut fx.ws,
        Edit::SetConditionLink {
            element: ElementRef::Asset(fx.asset),
            condition: fx.cf00,
            linked: true,
        },
    );
    let col_idx = TableId::Assets
        .columns()
        .iter()
        .position(|c| c.key == "conditions")
        .expect("assets conditions column");
    let rows = table_rows(&fx.ws, TableId::Assets, &vocab, &catalog);
    assert_eq!(rows.len(), 1);
    let cell = &rows[0].cells[col_idx];
    assert_eq!(cell.text, "");
    assert_eq!(cell.chips.len(), 1);
    assert_eq!(cell.chips[0].target, Selection::Condition(fx.cf00));
    assert_eq!(cell.chips[0].visual_id, "CF00");

    // Verify Threats, Goals and Claims also populate empty text rather than "[conditions]".
    for table in [TableId::Threats, TableId::Goals, TableId::Claims] {
        let idx = table
            .columns()
            .iter()
            .position(|c| c.key == "conditions")
            .expect("conditions column");
        let rows = table_rows(&fx.ws, table, &vocab, &catalog);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cells[idx].text, "");
    }
}

#[test]
fn remove_condition_detaches_all_eight_kinds_and_undoes_cleanly() {
    let mut fx = fixture();

    for element in [
        ElementRef::Asset(fx.asset),
        ElementRef::Component(fx.component),
        ElementRef::Interface(fx.interface),
        ElementRef::Threat(fx.threat),
        ElementRef::Tree(fx.tree),
        ElementRef::Goal(fx.goal),
        ElementRef::Claim(fx.claim),
    ] {
        apply_edit(
            &mut fx.ws,
            Edit::SetConditionLink {
                element,
                condition: fx.cf00,
                linked: true,
            },
        );
    }
    apply_edit(
        &mut fx.ws,
        Edit::SetStepConditionLink {
            tree: fx.tree,
            node: fx.step_a,
            condition: fx.cf00,
            linked: true,
        },
    );

    let all_keys = [
        (RelationKey::ASSET_CONDITIONS, Selection::Asset(fx.asset)),
        (
            RelationKey::COMPONENT_CONDITIONS,
            Selection::Component(fx.component),
        ),
        (
            RelationKey::INTERFACE_CONDITIONS,
            Selection::Interface(fx.interface),
        ),
        (RelationKey::THREAT_CONDITIONS, Selection::Threat(fx.threat)),
        (RelationKey::TREE_CONDITIONS, Selection::Tree(fx.tree)),
        (RelationKey::GOAL_CONDITIONS, Selection::Goal(fx.goal)),
        (RelationKey::CLAIM_CONDITIONS, Selection::Claim(fx.claim)),
        (
            RelationKey::STEP_CONDITIONS,
            Selection::TreeNode(fx.tree, fx.step_a),
        ),
    ];

    let before = fx.ws.project().clone();
    apply_edit(&mut fx.ws, Edit::RemoveCondition { condition: fx.cf00 });

    assert!(fx.ws.project().condition_idx(&fx.cf00).is_none());
    for (key, _) in all_keys {
        assert!(
            relations::reverse_related(fx.ws.project(), key, Selection::Condition(fx.cf00))
                .is_empty(),
            "expected no reverse links for {key:?} after RemoveCondition"
        );
    }

    fx.ws.undo().expect("undo RemoveCondition");
    assert!(fx.ws.project().condition_idx(&fx.cf00).is_some());
    for (key, expected) in all_keys {
        assert_eq!(
            relations::reverse_related(fx.ws.project(), key, Selection::Condition(fx.cf00)),
            vec![expected],
            "expected link restored for {key:?}"
        );
    }
    assert!(before.diff(fx.ws.project()).expect("diff").is_empty());
}

#[test]
fn assumption_edits_unchanged_conditions_and_claims_column() {
    let mut fx = fixture();
    let catalog = Catalog::load(Language::En);
    let vocab = pages::vocabulary(fx.ws.project().methodology());
    let as_id = AssumptionId::new_v7();

    let add = Edit::AddAssumption {
        id: as_id,
        name: "  Cabin locked  ".into(),
        holds: true,
    };
    assert_eq!(add.created(), Some(Selection::Assumption(as_id)));
    apply_edit(&mut fx.ws, add);
    let a = fx
        .ws
        .project()
        .assumption(fx.ws.project().assumption_idx(&as_id).expect("idx"))
        .expect("assumption");
    assert_eq!(a.name(), "Cabin locked");
    assert!(a.holds());

    // Unchanged no-ops emit empty op lists.
    assert!(
        Edit::RenameAssumption {
            assumption: as_id,
            name: " Cabin locked ".into(),
        }
        .to_ops(fx.ws.project(), "tester", NOW)
        .expect("noop rename")
        .is_empty()
    );
    assert!(
        Edit::SetAssumptionHolds {
            assumption: as_id,
            holds: true,
        }
        .to_ops(fx.ws.project(), "tester", NOW)
        .expect("noop holds")
        .is_empty()
    );
    assert!(
        Edit::SetAssumptionExclude {
            assumption: as_id,
            target: Selection::Tree(fx.tree),
            excluded: false,
        }
        .to_ops(fx.ws.project(), "tester", NOW)
        .expect("noop unexclude tree")
        .is_empty()
    );
    assert!(
        Edit::LinkClaimAssumption {
            claim: fx.claim,
            assumption: as_id,
            linked: false,
        }
        .to_ops(fx.ws.project(), "tester", NOW)
        .expect("noop unlink")
        .is_empty()
    );
    assert!(
        Edit::RemoveAssumption {
            assumption: AssumptionId::new_v7(),
        }
        .to_ops(fx.ws.project(), "tester", NOW)
        .expect("missing assumption removal is no-op")
        .is_empty()
    );

    // Excludes (tree + step), condition link, claim link, and hidden Claims column chip.
    apply_edit(
        &mut fx.ws,
        Edit::SetAssumptionExclude {
            assumption: as_id,
            target: Selection::Tree(fx.tree),
            excluded: true,
        },
    );
    apply_edit(
        &mut fx.ws,
        Edit::SetAssumptionExclude {
            assumption: as_id,
            target: Selection::TreeNode(fx.tree, fx.step_a),
            excluded: true,
        },
    );
    assert!(
        Edit::SetAssumptionExclude {
            assumption: as_id,
            target: Selection::TreeNode(fx.tree, fx.step_a),
            excluded: true,
        }
        .to_ops(fx.ws.project(), "tester", NOW)
        .expect("noop exclude step")
        .is_empty()
    );
    apply_edit(
        &mut fx.ws,
        Edit::SetConditionLink {
            element: ElementRef::Assumption(as_id),
            condition: fx.cf01,
            linked: true,
        },
    );
    apply_edit(
        &mut fx.ws,
        Edit::LinkClaimAssumption {
            claim: fx.claim,
            assumption: as_id,
            linked: true,
        },
    );
    assert!(
        Edit::LinkClaimAssumption {
            claim: fx.claim,
            assumption: as_id,
            linked: true,
        }
        .to_ops(fx.ws.project(), "tester", NOW)
        .expect("noop link")
        .is_empty()
    );

    let cfg = configuration_view(&fx.ws, &catalog);
    let cf01_row = cfg.rows.iter().find(|r| r.id == fx.cf01).expect("cf01");
    assert_eq!(cf01_row.needed_by_elements, 1);

    let col_idx = TableId::Claims
        .columns()
        .iter()
        .position(|c| c.key == "assumptions")
        .expect("assumptions column");
    let rows = table_rows(&fx.ws, TableId::Claims, &vocab, &catalog);
    assert_eq!(rows[0].cells[col_idx].text, "");
    assert_eq!(rows[0].cells[col_idx].chips.len(), 1);
    assert_eq!(
        rows[0].cells[col_idx].chips[0].target,
        Selection::Assumption(as_id)
    );

    // RemoveCondition detaches assumption's condition and undoes cleanly.
    let before_rm_cond = fx.ws.project().clone();
    apply_edit(&mut fx.ws, Edit::RemoveCondition { condition: fx.cf01 });
    assert!(
        fx.ws
            .project()
            .condition_link_ids(ElementRef::Assumption(as_id))
            .is_empty()
    );
    fx.ws.undo().expect("undo RemoveCondition");
    assert!(
        before_rm_cond
            .diff(fx.ws.project())
            .expect("diff")
            .is_empty()
    );

    // Invalid target on SetAssumptionExclude is rejected.
    assert_eq!(
        Edit::SetAssumptionExclude {
            assumption: as_id,
            target: Selection::Asset(fx.asset),
            excluded: true,
        }
        .to_ops(fx.ws.project(), "tester", NOW),
        Err(EditError::UnknownChoice)
    );
}
