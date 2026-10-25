//! The TARA pages end to end, without a server: a whole analysis is built
//! through `Edit` commands only (as the window sends them), and every page,
//! the comparison with a baseline, and every export are checked against it.

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::outline::to_outline;
use aegis_client::pages::{self, comparison};
use aegis_client::report::{ReportContext, report_html, risk_register_csv, tree_svg};
use aegis_client::view::{Selection, UiState, inspector, shown_tree};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ClaimId, ControlId, DamageId, GoalId, Methodology, NodeId, Project, ThreatId, TreeId,
};

struct Tara {
    ws: Workspace,
    asset: AssetId,
    damage: DamageId,
    threat: ThreatId,
    second: ThreatId,
    tree: TreeId,
    control: ControlId,
    goal: GoalId,
    claim: ClaimId,
}

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), "mira", 1_792_956_600_000)
        .expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn try_apply(ws: &mut Workspace, edit: &Edit) -> Result<(), String> {
    let ops = edit
        .to_ops(ws.project(), "mira", 1_792_956_600_000)
        .map_err(|e| e.to_string())?;
    ws.apply(ops, Origin::Edit).map_err(|e| e.to_string())
}

fn pos(list: &[String], label: &str) -> usize {
    list.iter()
        .position(|l| l.eq_ignore_ascii_case(label))
        .unwrap_or_else(|| panic!("{label} not in {list:?}"))
}

/// A brake ECU TARA: one asset, one damage, two threats, a tree with a
/// control, a goal and a claim.
fn tara() -> Tara {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let vocab = pages::vocabulary(&m);
    let mut ws = Workspace::empty(Arc::clone(&m));
    let (asset, damage, threat, second) = (
        AssetId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
        ThreatId::new_v7(),
    );
    let (tree, control, goal, claim) = (
        TreeId::new_v7(),
        ControlId::new_v7(),
        GoalId::new_v7(),
        ClaimId::new_v7(),
    );
    let integrity = pos(&vocab.properties, "Integrity");
    let authenticity = pos(&vocab.properties, "Authenticity");
    apply(
        &mut ws,
        &Edit::AddAsset {
            id: asset,
            name: "Brake request on CAN-2".into(),
            properties: vec![integrity, authenticity],
        },
    );
    apply(
        &mut ws,
        &Edit::AddDamage {
            id: damage,
            name: "Vehicle brakes hard at speed".into(),
        },
    );
    let safety = vocab
        .impact_categories
        .iter()
        .position(|c| c.label.eq_ignore_ascii_case("safety"))
        .expect("safety");
    let severe = vocab.impact_categories[safety].levels.len() - 1;
    apply(
        &mut ws,
        &Edit::SetImpact {
            damage,
            category: safety,
            level: Some(severe),
        },
    );
    let spoofing = pos(&vocab.threat_categories, "Spoofing");
    let tampering = pos(&vocab.threat_categories, "Tampering");
    for (id, name, cat) in [
        (threat, "Spoofed brake request", spoofing),
        (second, "Tampered brake firmware", tampering),
    ] {
        apply(
            &mut ws,
            &Edit::AddThreat {
                id,
                name: name.into(),
                asset,
                category: cat,
            },
        );
        apply(
            &mut ws,
            &Edit::LinkDamage {
                threat: id,
                damage,
                linked: true,
            },
        );
    }
    apply(
        &mut ws,
        &Edit::AddCatalogRef {
            threat,
            scheme: 0,
            text: "4.1".into(),
        },
    );
    apply(
        &mut ws,
        &Edit::SetThreatFeasibility {
            threat: second,
            level: Some(1),
        },
    );
    apply(
        &mut ws,
        &Edit::NewTree {
            id: tree,
            root: "Unintended braking".into(),
            threat: Some(threat),
        },
    );
    apply(
        &mut ws,
        &Edit::ReplaceTree {
            tree,
            outline: "OR Unintended braking\n  SEQ Physical via OBD\n    - Gain access to OBD port\n    - Open diagnostic session\n  - Remote via telematics\n".into(),
        },
    );
    let high = vocab.feasibility.len() - 1;
    let t = ws.project().attack_tree_idx(&tree).expect("tree");
    let nodes: Vec<(NodeId, String)> = ws
        .project()
        .attack_tree(t)
        .expect("tree")
        .iter()
        .map(|(_, id, n)| (id, n.label().to_owned()))
        .collect();
    for (id, label) in &nodes {
        if label.starts_with("Gain") || label.starts_with("Open") || label.starts_with("Remote") {
            apply(
                &mut ws,
                &Edit::SetStepFeasibility {
                    tree,
                    node: *id,
                    level: Some(if label.starts_with("Remote") { 0 } else { high }),
                },
            );
        }
    }
    apply(
        &mut ws,
        &Edit::AddControl {
            id: control,
            name: "Authenticated diagnostics".into(),
        },
    );
    let open_session = nodes
        .iter()
        .find(|(_, l)| l.starts_with("Open"))
        .expect("step")
        .0;
    apply(
        &mut ws,
        &Edit::SetStepMitigation {
            tree,
            node: open_session,
            control,
            level: Some(0),
        },
    );
    apply(
        &mut ws,
        &Edit::AddGoal {
            id: goal,
            name: "Only authenticated testers open sessions".into(),
        },
    );
    apply(
        &mut ws,
        &Edit::LinkGoalControl {
            goal,
            control,
            linked: true,
        },
    );
    apply(
        &mut ws,
        &Edit::SetTreatment {
            threat,
            treatment: Some(1),
        },
    );
    apply(
        &mut ws,
        &Edit::SetThreatGoal {
            threat,
            goal: Some(goal),
        },
    );
    apply(
        &mut ws,
        &Edit::AddClaim {
            id: claim,
            name: "Firmware is signed by the supplier".into(),
        },
    );
    apply(
        &mut ws,
        &Edit::SetTreatment {
            threat: second,
            treatment: Some(3),
        },
    );
    apply(
        &mut ws,
        &Edit::SetThreatClaim {
            threat: second,
            claim: Some(claim),
        },
    );
    Tara {
        ws,
        asset,
        damage,
        threat,
        second,
        tree,
        control,
        goal,
        claim,
    }
}

#[test]
fn every_page_shows_the_analysis() {
    let t = tara();
    let ws = &t.ws;
    let assets = pages::asset_rows(ws);
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].threats, 2);
    assert_eq!(assets[0].properties.iter().filter(|p| **p).count(), 2);
    let a = pages::asset_detail(ws, t.asset).expect("asset");
    assert_eq!(a.threats.len(), 2);
    assert!(a.changed.contains("mira"), "{}", a.changed);

    let damages = pages::damage_rows(ws);
    assert_eq!(damages[0].threats, 2);
    assert!(
        damages[0].impacts_text.contains("Safety"),
        "{}",
        damages[0].impacts_text
    );
    assert_eq!(
        pages::damage_detail(ws, t.damage)
            .expect("damage")
            .threats
            .len(),
        2
    );

    let d = pages::threat_detail(ws, t.threat).expect("threat");
    assert_eq!(d.tree, Some(t.tree));
    assert_eq!(d.goal, Some(t.goal));
    assert_eq!(d.catalog_refs.len(), 1);
    assert!(d.inherent.is_some() && d.residual.is_some());
    assert!(
        d.why.iter().any(|w| w.contains("Vehicle brakes hard")),
        "{:?}",
        d.why
    );
    // The control lowers the OBD path, so the residual risk is not higher.
    assert!(d.residual.as_ref().map(|r| r.tone) <= d.inherent.as_ref().map(|r| r.tone));

    let risks = pages::risk_rows(ws);
    assert_eq!(risks.len(), 2);
    assert!(
        risks[0].inherent.as_ref().map(|r| r.tone) >= risks[1].inherent.as_ref().map(|r| r.tone)
    );
    assert!(risks.iter().any(|r| r.backing.starts_with("Goal:")));
    assert!(risks.iter().any(|r| r.backing.starts_with("Claim:")));

    let trees = pages::tree_rows(ws);
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0].nodes, 5);
    assert_eq!(trees[0].threats, "Spoofed brake request");

    let controls = pages::control_rows(ws);
    assert_eq!((controls[0].steps, controls[0].goals), (1, 1));
    let c = pages::control_detail(ws, t.control).expect("control");
    assert_eq!(c.steps.len(), 1);
    assert_eq!(c.steps[0].target, Selection::Tree(t.tree));

    assert_eq!(pages::goal_rows(ws)[0].controls, 1);
    assert_eq!(
        pages::goal_detail(ws, t.goal).expect("goal").threats.len(),
        1
    );
    assert_eq!(
        pages::claim_detail(ws, t.claim)
            .expect("claim")
            .threats
            .len(),
        1
    );

    let ov = pages::overview(ws);
    assert_eq!(ov.counts[2].count, 2);
    assert_eq!(ov.inherent.iter().map(|c| c.count).sum::<usize>(), 2);
    assert_eq!(ov.top.len(), 2);
}

#[test]
fn the_inspector_lists_and_offers_controls() {
    let t = tara();
    let project = t.ws.project();
    let t_idx = project.attack_tree_idx(&t.tree).expect("tree");
    let tree = project.attack_tree(t_idx).expect("tree");
    let (_, step, _) = tree
        .iter()
        .find(|(_, _, n)| n.label().starts_with("Open"))
        .expect("step");
    let ui = UiState {
        selected_tree: Some(t.tree),
        selected_node: Some(step),
        ..UiState::default()
    };
    assert_eq!(shown_tree(&t.ws, &ui), Some(t.tree));
    let i = inspector(&t.ws, &ui).expect("inspector");
    assert_eq!(i.mitigations.len(), 1);
    assert!(i.attachable.is_empty());
    assert!(i.residual.is_some(), "the control changes this step");
}

#[test]
fn editing_the_outline_keeps_ratings_controls_and_links() {
    let mut t = tara();
    let before = pages::threat_detail(&t.ws, t.threat).expect("threat");
    let project = t.ws.project();
    let outline = to_outline(
        project
            .attack_tree(project.attack_tree_idx(&t.tree).expect("tree"))
            .expect("tree"),
    );
    // Add a step and rename the root: the rated steps and the control stay.
    let edited = outline
        .replace("OR Unintended braking", "OR Unintended braking at speed")
        .replace(
            "  - Remote via telematics\n",
            "  - Remote via telematics\n  - Malicious update\n",
        );
    apply(
        &mut t.ws,
        &Edit::ReplaceTree {
            tree: t.tree,
            outline: edited,
        },
    );
    let after = pages::threat_detail(&t.ws, t.threat).expect("threat");
    assert_eq!(after.tree, Some(t.tree), "the threat stays linked");
    assert_eq!(
        pages::control_detail(&t.ws, t.control)
            .expect("c")
            .steps
            .len(),
        1
    );
    let rows = pages::tree_rows(&t.ws);
    assert_eq!(
        (rows[0].nodes, rows[0].title.as_str()),
        (6, "Unintended braking at speed")
    );
    // An unrated step makes the tree not computable (the model's rule and a
    // blocking problem). Rated at the lowest feasibility, the OR still takes
    // the easiest path: the risk is as before.
    assert_eq!(after.inherent, None);
    let p = t.ws.project();
    let tree = p
        .attack_tree(p.attack_tree_idx(&t.tree).expect("tree"))
        .expect("tree");
    let (_, new_step, _) = tree
        .iter()
        .find(|(_, _, n)| n.label() == "Malicious update")
        .expect("new step");
    apply(
        &mut t.ws,
        &Edit::SetStepFeasibility {
            tree: t.tree,
            node: new_step,
            level: Some(0),
        },
    );
    let rated = pages::threat_detail(&t.ws, t.threat).expect("threat");
    assert_eq!(rated.inherent, before.inherent);
    t.ws.undo().expect("undo the rating");
    // One undo brings the old tree back.
    t.ws.undo().expect("undo");
    assert_eq!(pages::tree_rows(&t.ws)[0].nodes, 5);
    // A broken outline is refused with its line, and nothing changes.
    let err = Edit::ReplaceTree {
        tree: t.tree,
        outline: "OR Root\n  - a\n    - under a step\n".into(),
    }
    .to_ops(t.ws.project(), "mira", 1)
    .expect_err("refused");
    assert!(matches!(err, EditError::Outline(_)));
    assert!(err.to_string().contains("line 3"), "{err}");
}

#[test]
fn removals_cascade_and_undo_restores_everything() {
    let mut t = tara();
    let full: Project = t.ws.project().clone();
    // An asset with threats is not removed.
    let refused = try_apply(&mut t.ws, &Edit::RemoveAsset { asset: t.asset }).expect_err("in use");
    assert!(refused.contains("2 threat scenario"), "{refused}");

    for edit in [
        Edit::RemoveControl { control: t.control },
        Edit::RemoveGoal { goal: t.goal },
        Edit::RemoveClaim { claim: t.claim },
        Edit::RemoveTree { tree: t.tree },
        Edit::RemoveDamage { damage: t.damage },
        Edit::RemoveThreat { threat: t.threat },
        Edit::RemoveThreat { threat: t.second },
        Edit::RemoveAsset { asset: t.asset },
    ] {
        try_apply(&mut t.ws, &edit).unwrap_or_else(|e| panic!("{edit:?}: {e}"));
    }
    let p = t.ws.project();
    assert_eq!(
        (
            p.asset_count(),
            p.threat_scenario_count(),
            p.attack_tree_count(),
            p.security_control_count()
        ),
        (0, 0, 0, 0)
    );
    for _ in 0..8 {
        t.ws.undo().expect("undo");
    }
    assert!(
        t.ws.project().diff(&full).expect("diff").is_empty(),
        "undo restores the project"
    );
}

#[test]
fn problems_name_what_is_wrong_and_where() {
    let mut t = tara();
    apply(
        &mut t.ws,
        &Edit::AddDamage {
            id: DamageId::new_v7(),
            name: "Unused damage".into(),
        },
    );
    let problems = pages::problems(&t.ws);
    assert!(!problems.is_empty());
    let unused = problems
        .iter()
        .find(|p| p.text.contains("\"Unused damage\""))
        .expect("named");
    assert!(matches!(unused.target, Some(Selection::Damage(_))));
    // Errors come first.
    let first_warning = problems
        .iter()
        .position(|p| !p.error)
        .unwrap_or(problems.len());
    assert!(problems[first_warning..].iter().all(|p| !p.error));
}

#[test]
fn comparing_with_a_baseline_lists_changes_and_moved_risks() {
    let mut t = tara();
    let base = t.ws.project().clone();
    let same = comparison(&base, "B1", t.ws.project()).expect("compare");
    assert!(same.lines.is_empty());
    assert_eq!(same.summary, "Nothing changed since this baseline.");
    apply(
        &mut t.ws,
        &Edit::SetThreatFeasibility {
            threat: t.second,
            level: Some(3),
        },
    );
    apply(
        &mut t.ws,
        &Edit::RenameAsset {
            asset: t.asset,
            name: "Brake request".into(),
        },
    );
    let c = comparison(&base, "B1", t.ws.project()).expect("compare");
    assert!(
        c.lines
            .iter()
            .any(|l| l.kind == "Changed" && l.text.contains("Brake request"))
    );
    assert!(
        c.lines
            .iter()
            .any(|l| l.kind == "Risk" && l.text.contains("Tampered brake firmware"))
    );
    assert!(c.summary.contains("1 risk(s) moved"), "{}", c.summary);
}

#[test]
fn exports_are_complete_escaped_and_deterministic() {
    let mut t = tara();
    apply(
        &mut t.ws,
        &Edit::RenameThreat {
            threat: t.threat,
            name: "Spoofed <script>alert(1)</script>, \"quoted\"".into(),
        },
    );
    let ctx = ReportContext {
        project: "Brake ECU".into(),
        user: "Mira Chen".into(),
        now_ms: 1_792_956_600_000,
        seq: 42,
        unsaved: false,
    };
    let html = report_html(&t.ws, &ctx);
    assert_eq!(html, report_html(&t.ws, &ctx), "deterministic");
    assert!(html.contains("Exported by Mira Chen on 25 Oct 2026 19:30 UTC"));
    assert!(!html.contains("<script>alert"), "user text is escaped");
    assert!(html.contains("&lt;script&gt;"));
    for section in [
        "Executive summary",
        "Assets",
        "Damage scenarios",
        "Attack trees",
        "Open problems",
    ] {
        assert!(html.contains(section), "{section}");
    }
    assert!(html.contains("<svg"), "the tree diagram is embedded");

    let csv = risk_register_csv(&t.ws, &ctx);
    assert_eq!(csv.lines().count(), 4, "{csv}");
    assert!(
        csv.contains("\"Spoofed <script>alert(1)</script>, \"\"quoted\"\"\""),
        "{csv}"
    );

    let svg = tree_svg(&t.ws, t.tree, true).expect("svg");
    assert!(
        svg.starts_with("<svg") || svg.starts_with("<?xml"),
        "{}",
        &svg[..40]
    );
}
