//! The TARA pages end to end, without a server: a whole analysis is built
//! through `Edit` commands only (as the window sends them), and every page,
//! the comparison with a baseline, and every export are checked against it.

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::outline::to_outline;
use aegis_client::pages::{self, comparison};
use aegis_client::report::{Block, Figure, report_doc, report_pdf};
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

// ---- the PDF report (job 050) --------------------------------------------

fn context(unsaved: bool) -> ReportContext {
    ReportContext {
        project: "Brake ECU".into(),
        user: "Mira Chen".into(),
        now_ms: 1_792_956_600_000,
        seq: 42,
        unsaved,
    }
}

/// The text of a PDF as `pdftotext` reads it, one string per page. The
/// tests fail instead of skipping on CI, where poppler is installed.
fn pdf_pages(pdf: &[u8]) -> Option<Vec<String>> {
    use std::io::Write as _;
    let child = std::process::Command::new("pdftotext")
        .args(["-enc", "UTF-8", "-", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        assert!(
            std::env::var_os("CI").is_none(),
            "pdftotext is missing on CI (poppler-utils)"
        );
        eprintln!("pdftotext is not installed: skipping the text checks");
        return None;
    };
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(pdf)
        .expect("write");
    let out = child.wait_with_output().expect("pdftotext");
    assert!(out.status.success(), "pdftotext refused the file");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    let mut pages: Vec<String> = text.split('\x0c').map(str::to_owned).collect();
    if pages.last().is_some_and(|p| p.trim().is_empty()) {
        pages.pop();
    }
    Some(pages)
}

fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_pdf_report_has_every_section_and_is_deterministic() {
    let mut t = tara();
    apply(
        &mut t.ws,
        &Edit::RenameThreat {
            threat: t.threat,
            name: "Spoofed <script>alert(1)</script>, \"quoted\" and Bremsensteuergerät Größe"
                .into(),
        },
    );
    let ctx = context(false);
    let pdf = report_pdf(&t.ws, &ctx).expect("pdf");
    assert!(pdf.starts_with(b"%PDF-1."), "{:?}", &pdf[..8]);
    assert!(
        pdf.ends_with(b"%%EOF\n") || pdf.ends_with(b"%%EOF"),
        "complete file"
    );
    assert_eq!(
        pdf,
        report_pdf(&t.ws, &ctx).expect("pdf"),
        "same input, same bytes"
    );

    // Kept for a human to open (like the window screenshots).
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/report-samples");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("tara-report.pdf"), &pdf).expect("write");

    let Some(pages) = pdf_pages(&pdf) else { return };
    let all = words(&pages.join(" "));
    for section in [
        "Brake ECU",
        "1. Executive summary",
        "2. Assets",
        "3. Damage scenarios",
        "4. Threat scenarios",
        "5. Attack trees",
        "6. Risk register and treatment",
        "7. Security controls",
        "8. Cybersecurity goals and claims",
        "9. Open problems",
        "Brake request on CAN-2",
        "Confidential. Exported by Mira Chen on 25 Oct 2026 19:30 UTC.",
    ] {
        assert!(all.contains(section), "missing {section:?} in:\n{all}");
    }
    // Unicode survives (the font is embedded), and text is printed as it is.
    assert!(all.contains("Bremsensteuergerät Größe"), "{all}");
    assert!(
        all.contains("<script>alert(1)</script>"),
        "plain text, not markup"
    );
    assert!(!all.contains("Draft:"), "saved state has no draft note");
    // The attack tree is in the report as its outline.
    assert!(
        all.contains("OR") || all.contains("AND"),
        "tree outline: {all}"
    );
}

#[test]
fn every_page_has_the_watermark_and_a_page_number_and_tables_continue() {
    let mut t = tara();
    for i in 0..120 {
        apply(
            &mut t.ws,
            &Edit::AddAsset {
                id: AssetId::new_v7(),
                name: format!("Gateway interface number {i}"),
                properties: vec![0],
            },
        );
    }
    let pdf = report_pdf(&t.ws, &context(true)).expect("pdf");
    let Some(pages) = pdf_pages(&pdf) else { return };
    assert!(pages.len() >= 3, "{} pages", pages.len());
    let total = pages.len();
    for (i, page) in pages.iter().enumerate() {
        let text = words(page);
        assert!(
            text.contains("Confidential. Exported by Mira Chen"),
            "page {}",
            i + 1
        );
        assert!(
            text.contains(&format!("Page {} of {total}", i + 1)),
            "page {}: {text}",
            i + 1
        );
    }
    // The first page says it is a draft; a long table repeats its header row.
    assert!(words(&pages[0]).contains("Draft:"));
    // Header words are never cut in the middle, even in narrow columns.
    let with_header = pages
        .iter()
        .filter(|p| words(p).contains("Non-repudiation"))
        .count();
    assert!(
        with_header >= 2,
        "the header row repeats on a new page ({with_header})"
    );
    // Nothing is lost between pages.
    let all = words(&pages.join(" "));
    for i in [0, 59, 119] {
        assert!(
            all.contains(&format!("Gateway interface number {i}")),
            "row {i}"
        );
    }
}

#[test]
fn very_long_words_and_lines_are_wrapped_not_lost() {
    let mut t = tara();
    // One word far wider than the page, then many short ones (256 characters
    // is the longest name the model takes).
    let long = format!("{} {}", "Ab".repeat(100), "word ".repeat(10).trim_end());
    apply(
        &mut t.ws,
        &Edit::RenameThreat {
            threat: t.threat,
            name: long.clone(),
        },
    );
    let pdf = report_pdf(&t.ws, &context(false)).expect("pdf");
    let Some(pages) = pdf_pages(&pdf) else { return };
    let all = words(&pages.join(" ")).replace(' ', "");
    assert!(
        all.contains(&long.replace(' ', "")),
        "the whole name is on the pages"
    );
}

#[test]
fn the_pdf_and_the_html_report_have_the_same_sections() {
    let t = tara();
    let ctx = context(false);
    let html = report_html(&t.ws, &ctx);
    let doc = report_doc(&t.ws, &ctx);
    let from_html: Vec<String> = html
        .split("<h2>")
        .skip(1)
        .map(|s| s.split("</h2>").next().unwrap_or_default().to_owned())
        .collect();
    let from_doc: Vec<String> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading(2, text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(from_doc, from_html);
}

fn figures(doc: &aegis_client::report::ReportDoc) -> Vec<&Figure> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            Block::Figure(f) => Some(f),
            _ => None,
        })
        .collect()
}

fn set_outline(t: &mut Tara, outline: String) {
    let tree = t.tree;
    apply(&mut t.ws, &Edit::ReplaceTree { tree, outline });
}

/// Writes a sample for a human to open.
fn keep_sample(name: &str, pdf: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/report-samples");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join(name), pdf).expect("write");
}

#[test]
fn a_small_tree_is_drawn_as_one_picture_before_its_outline() {
    let t = tara();
    let ctx = context(false);
    let doc = report_doc(&t.ws, &ctx);
    let figs = figures(&doc);
    assert_eq!(figs.len(), 1, "one tree, one figure");
    assert_eq!(figs[0].panels.len(), 1);
    assert_eq!(figs[0].omitted, 0);
    assert_eq!(figs[0].panels[0].caption, "Attack tree");
    let drawn: Vec<&str> = figs[0].panels[0]
        .list
        .items
        .iter()
        .filter_map(|i| match i {
            aegis_canvas::Item::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        drawn.iter().any(|t| t.contains("Unintended")),
        "the root title is drawn: {drawn:?}"
    );
    // The figure comes before the outline of the same tree.
    let fig_at = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Figure(_)))
        .expect("figure");
    assert!(matches!(doc.blocks[fig_at + 1], Block::Preformatted(_)));

    let pdf = report_pdf(&t.ws, &ctx).expect("pdf");
    keep_sample("tara-report.pdf", &pdf);
    let Some(pages) = pdf_pages(&pdf) else { return };
    let all = words(&pages.join(" "));
    assert!(all.contains("Attack tree"), "caption: {all}");
    assert!(all.contains("Gain access to OBD port") || all.contains("Gain access"));
}

#[test]
fn a_tall_tree_gets_an_overview_and_detail_panels_in_the_pdf() {
    let mut t = tara();
    // 30 siblings: the most the model allows is 32 nodes, and this is about
    // 2300 world pixels tall, far too much for one readable page.
    let mut outline = String::from("OR Unintended braking\n");
    for i in 0..30 {
        outline.push_str(&format!("  - Attack step number {i}\n"));
    }
    set_outline(&mut t, outline);
    let ctx = context(false);
    let doc = report_doc(&t.ws, &ctx);
    let figs = figures(&doc);
    assert_eq!(figs.len(), 1);
    let f = figs[0];
    assert!(
        f.panels.len() > 2,
        "overview plus detail: {}",
        f.panels.len()
    );
    assert!(f.panels[0].caption.starts_with("Overview"));
    assert!(f.panels[1].caption.starts_with("Detail: row 1 of"));
    // The overview draws no text (too small to read); a detail panel does.
    let has_text = |p: &aegis_client::report::Panel| {
        p.list
            .items
            .iter()
            .any(|i| matches!(i, aegis_canvas::Item::Text { .. }))
    };
    assert!(!has_text(&f.panels[0]));
    assert!(has_text(&f.panels[1]));
    // No panel is taller than a page can hold.
    assert!(
        f.panels
            .iter()
            .all(|p| p.size.1 <= 560.0 && p.size.0 <= 500.0)
    );

    let pdf = report_pdf(&t.ws, &ctx).expect("pdf");
    keep_sample("tara-report-tall-tree.pdf", &pdf);
    assert_eq!(pdf, report_pdf(&t.ws, &ctx).expect("pdf"), "deterministic");
    let Some(pages) = pdf_pages(&pdf) else { return };
    let all = words(&pages.join(" "));
    assert!(all.contains("Overview of the whole tree"));
    assert!(all.contains("Detail: row 2 of"), "{all}");
    // Every step is readable somewhere in the detail panels.
    for i in [0, 14, 29] {
        assert!(all.contains(&format!("number {i}")), "step {i} missing");
    }
}

#[test]
fn a_deep_tree_is_cut_along_its_width() {
    let mut t = tara();
    // A chain of 14 nested gates: wider than a page at any readable zoom.
    let mut outline = String::new();
    for depth in 0..14 {
        outline.push_str(&"  ".repeat(depth));
        outline.push_str(&format!("AND Nested gate {depth}\n"));
    }
    outline.push_str(&"  ".repeat(14));
    outline.push_str("- The last step\n");
    set_outline(&mut t, outline);
    let ctx = context(false);
    let doc = report_doc(&t.ws, &ctx);
    let f = figures(&doc)[0].clone();
    assert!(f.panels[0].caption.starts_with("Overview"));
    let captions: Vec<&str> = f.panels.iter().map(|p| p.caption.as_str()).collect();
    assert!(
        captions.iter().any(|c| c.contains("column 2 of")),
        "{captions:?}"
    );
    let pdf = report_pdf(&t.ws, &ctx).expect("pdf");
    keep_sample("tara-report-deep-tree.pdf", &pdf);
    let Some(pages) = pdf_pages(&pdf) else { return };
    assert!(words(&pages.join(" ")).contains("The last step"));
}

#[test]
fn a_tree_that_has_no_nodes_left_has_no_figure_and_does_not_break_the_report() {
    let mut t = tara();
    let tree = t.tree;
    apply(&mut t.ws, &Edit::RemoveTree { tree });
    let doc = report_doc(&t.ws, &context(false));
    assert!(figures(&doc).is_empty());
    report_pdf(&t.ws, &context(false)).expect("pdf");
}

#[test]
fn eu_cra_pages_and_exports_use_methodology_names_and_plain_likelihood() {
    let m = Arc::new(Methodology::eu_cra().expect("eu-cra"));
    let vocab = pages::vocabulary(&m);
    assert!(!vocab.has_attack_potential);
    assert_eq!(
        vocab.element_names.damage_scenario.singular.as_ref(),
        "Harm"
    );
    assert_eq!(vocab.element_names.damage_scenario.plural.as_ref(), "Harms");
    assert_eq!(
        vocab.element_names.cybersecurity_goal.singular.as_ref(),
        "Security requirement"
    );
    assert_eq!(
        vocab.element_names.cybersecurity_goal.plural.as_ref(),
        "Security requirements"
    );
    assert_eq!(vocab.element_names.feasibility.as_ref(), "Likelihood");

    let mut ws = Workspace::empty(Arc::clone(&m));
    let ops = aegis_client::sample::sample_ops(&m, "mira", 1_792_956_600_000).expect("sample");
    ws.apply(ops, Origin::Edit).expect("applies");

    let ov = pages::overview(&ws);
    assert_eq!(ov.counts[1].label, "Harms");
    assert_eq!(ov.counts[5].label, "Security requirements");

    let (_, tree_id, tree) = ws.project().attack_trees().next().expect("tree");
    let (_, step_id, _) = tree
        .iter()
        .find(|(_, _, n)| n.gate_kind().is_none())
        .expect("step");
    let ui = UiState {
        selected_tree: Some(tree_id),
        selected_node: Some(step_id),
        ..UiState::default()
    };
    let ins = inspector(&ws, &ui).expect("inspector");
    assert!(
        ins.factors.is_empty(),
        "plain likelihood has no attack potential factors"
    );
    assert!(ins.potential.is_none());

    let ctx = context(false);
    let html = report_html(&ws, &ctx);
    assert!(html.contains("<h2>3. Harms</h2>"), "{html}");
    assert!(
        html.contains("<h2>8. Security requirements and cybersecurity claims</h2>"),
        "{html}"
    );
    let doc = report_doc(&ws, &ctx);
    let from_html: Vec<String> = html
        .split("<h2>")
        .skip(1)
        .map(|s| s.split("</h2>").next().unwrap_or_default().to_owned())
        .collect();
    let from_doc: Vec<String> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading(2, text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(from_doc, from_html);

    let csv = risk_register_csv(&ws, &ctx);
    assert!(
        csv.starts_with(
            "# Confidential. Exported by Mira Chen on 25 Oct 2026 19:30 UTC.\nThreat scenario,Asset,Deciding harm,Impact,Likelihood,"
        ),
        "{csv}"
    );
}
