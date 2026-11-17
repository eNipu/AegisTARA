//! Job 146: the damage-link rationale reaches the risk register, the threat
//! inspector, the DOC report and the XLSX Threats sheet.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::pages;
use aegis_client::report::{Block, ReportContext, report_doc, report_workbook_xlsx};
use aegis_client::view::TableId;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{AssetId, DamageId, Methodology, ThreatId};

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), "mira", 1_792_956_600_000)
        .expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

struct Fx {
    ws: Workspace,
    threat: ThreatId,
    damage: DamageId,
}

/// One asset, one rated damage scenario and one threat linked to it with a
/// direct feasibility rating, so the risk engine has a deciding damage link.
fn fixture() -> Fx {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let vocab = pages::vocabulary(&m);
    let mut ws = Workspace::empty(Arc::clone(&m));
    let (asset, damage, threat) = (AssetId::new_v7(), DamageId::new_v7(), ThreatId::new_v7());
    apply(
        &mut ws,
        &Edit::AddAsset {
            id: asset,
            name: "Firmware".into(),
            properties: (0..vocab.properties.len()).collect(),
        },
    );
    apply(
        &mut ws,
        &Edit::AddDamage {
            id: damage,
            name: "Vehicle misbehaves".into(),
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
            damage,
            category: safety,
            level: Some(vocab.impact_categories[safety].levels.len() - 1),
        },
    );
    apply(
        &mut ws,
        &Edit::AddThreat {
            id: threat,
            name: "Firmware replaced".into(),
            asset,
            category: 0,
        },
    );
    apply(
        &mut ws,
        &Edit::LinkDamage {
            threat,
            damage,
            linked: true,
        },
    );
    apply(
        &mut ws,
        &Edit::SetThreatFeasibility {
            threat,
            level: Some(1),
        },
    );
    Fx { ws, threat, damage }
}

fn set_rationale(f: &mut Fx, text: &str) {
    apply(
        &mut f.ws,
        &Edit::SetDamageLinkRationale {
            threat: f.threat,
            damage: f.damage,
            text: text.into(),
        },
    );
}

fn ctx() -> ReportContext {
    ReportContext {
        project: "Brake ECU".into(),
        user: "Mira Chen".into(),
        now_ms: 1_792_956_600_000,
        seq: 42,
        unsaved: false,
    }
}

// Reused from tests/reduced_by_report.rs: find a report table by header and
// read a named cell of a row identified by a substring.
fn table_with<'a>(blocks: &'a [Block], header: &str) -> (&'a [String], &'a [Vec<String>]) {
    blocks
        .iter()
        .find_map(|b| match b {
            Block::Table { headers, rows } if headers.iter().any(|h| h == header) => {
                Some((headers.as_slice(), rows.as_slice()))
            }
            _ => None,
        })
        .expect("table")
}

fn cell_of<'a>(headers: &[String], rows: &'a [Vec<String>], key: &str, col: &str) -> &'a str {
    let c = headers.iter().position(|h| h == col).expect("column");
    rows.iter()
        .find(|r| r[0].contains(key))
        .map(|r| r[c].as_str())
        .expect("row")
}

/// Read the "Pair rationale" cell of the XLSX Threats sheet row for `key`.
fn xlsx_rationale(ws: &Workspace, key: &str) -> String {
    let wb = aegis_client::excel::read_workbook(&report_workbook_xlsx(ws)).expect("reads");
    let s = wb
        .sheets
        .iter()
        .find(|s| s.name == "Threat scenarios")
        .expect("Threat scenarios sheet");
    let c = s
        .headers
        .iter()
        .position(|h| h == "Pair rationale")
        .expect("Pair rationale header");
    s.rows
        .iter()
        .find(|(_, r)| r.iter().any(|v| v.contains(key)))
        .map(|(_, r)| r[c].clone())
        .expect("row")
}

#[test]
fn pair_rationale_reaches_the_client_views() {
    let mut f = fixture();
    set_rationale(&mut f, "Trusted boot chain");
    assert!(
        TableId::Risks
            .columns()
            .iter()
            .any(|c| c.key == "rationale"),
        "Risks table is missing the rationale column"
    );
    assert_eq!(pages::risk_rows(&f.ws)[0].rationale, "Trusted boot chain");
    let detail = pages::threat_detail(&f.ws, f.threat).expect("detail");
    assert_eq!(detail.rationale, "Trusted boot chain");
}

#[test]
fn pair_rationale_is_in_the_doc_risk_table() {
    let mut f = fixture();
    set_rationale(&mut f, "Trusted boot chain");
    let doc = report_doc(&f.ws, &ctx());
    let (headers, rows) = table_with(&doc.blocks, "Pair rationale");
    assert_eq!(
        cell_of(headers, rows, "Firmware replaced", "Pair rationale"),
        "Trusted boot chain"
    );
}

#[test]
fn pair_rationale_is_in_the_xlsx_threats_sheet() {
    let mut f = fixture();
    set_rationale(&mut f, "Trusted boot chain");
    assert_eq!(
        xlsx_rationale(&f.ws, "Firmware replaced"),
        "Trusted boot chain"
    );
}

#[test]
fn an_empty_rationale_gives_empty_cells() {
    let f = fixture();
    assert_eq!(pages::risk_rows(&f.ws)[0].rationale, "");
    let detail = pages::threat_detail(&f.ws, f.threat).expect("detail");
    assert_eq!(detail.rationale, "");
    let doc = report_doc(&f.ws, &ctx());
    let (headers, rows) = table_with(&doc.blocks, "Pair rationale");
    assert_eq!(
        cell_of(headers, rows, "Firmware replaced", "Pair rationale"),
        ""
    );
    assert_eq!(xlsx_rationale(&f.ws, "Firmware replaced"), "");
}

/// Unlinking a damage, removing the damage, or removing the threat each undo
/// back to a project that still has the link rationale.
#[test]
fn unlink_and_removals_undo_with_the_rationale() {
    for removal in 0..3 {
        let mut fx = fixture();
        apply(
            &mut fx.ws,
            &Edit::SetDamageLinkRationale {
                threat: fx.threat,
                damage: fx.damage,
                text: "Firmware swap reaches the brake ECU".into(),
            },
        );
        if removal == 2 {
            // A threat with a rating cannot go in one step; clear it first.
            apply(
                &mut fx.ws,
                &Edit::SetThreatFeasibility {
                    threat: fx.threat,
                    level: None,
                },
            );
        }
        let before = fx.ws.project().clone();
        let edit = match removal {
            0 => Edit::LinkDamage {
                threat: fx.threat,
                damage: fx.damage,
                linked: false,
            },
            1 => Edit::RemoveDamage { damage: fx.damage },
            _ => Edit::RemoveThreat { threat: fx.threat },
        };
        apply(&mut fx.ws, &edit);
        fx.ws.undo().expect("undo");
        assert!(
            *fx.ws.project() == before,
            "removal {removal} lost the rationale"
        );
    }
}
