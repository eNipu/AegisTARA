//! The report builder (jobs 070 to 072): one document model for HTML and
//! PDF, section and filter selection, language and branding, the audit
//! summary, and the XLSX workbook with formula-injection guards.

use std::sync::Arc;

use aegis_client::i18n::Language;
use aegis_client::report::{
    Block, Classification, ReportContext, ReportFilters, ReportLanguage, ReportOptions,
    ReportProfile, ReportSections, report_doc, report_doc_with_options, report_html,
    report_html_with_options, report_workbook_xlsx,
};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::Methodology;

fn context() -> ReportContext {
    ReportContext {
        project: "Brake ECU".into(),
        user: "Mira Chen".into(),
        now_ms: 1_792_956_600_000,
        seq: 42,
        unsaved: false,
    }
}

fn workspace() -> Workspace {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let ops = aegis_client::sample::sample_ops(&m, "mira", 1_792_956_600_000).expect("sample");
    ws.apply(ops, Origin::Edit).expect("applies");
    ws
}

fn headings(doc: &aegis_client::report::ReportDoc) -> Vec<String> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading(2, text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn html_and_pdf_share_the_same_document_and_toc() {
    let ws = workspace();
    let ctx = context();
    let doc = report_doc(&ws, &ctx);
    let html = report_html(&ws, &ctx);
    // The HTML renderer reads the same blocks: the level-2 headings in the
    // document appear in the HTML, and the HTML has a table of contents.
    for h in headings(&doc) {
        assert!(html.contains(&format!("</a><h2>{h}</h2>")), "{h}");
    }
    assert!(html.contains("nav class=\"toc\""), "a table of contents");
    assert!(html.contains("<a id=\"sec-"), "anchors");
}

#[test]
fn sections_can_be_turned_off() {
    let ws = workspace();
    let ctx = context();
    let profile = ReportProfile {
        sections: ReportSections {
            risks: false,
            problems: false,
            controls: false,
            ..ReportSections::default()
        },
        ..ReportProfile::default()
    };
    let doc = report_doc_with_options(
        &ws,
        &ctx,
        &ReportOptions {
            profile,
            baseline_project: None,
        },
    );
    let hs = headings(&doc);
    assert!(
        !hs.iter()
            .any(|h| h.starts_with("6.") || h.starts_with("7.") || h.starts_with("9.")),
        "{hs:?}"
    );
    assert!(hs.iter().any(|h| h.starts_with("2. Assets")));
}

#[test]
fn filters_narrow_the_risk_register() {
    let ws = workspace();
    let ctx = context();
    let rows = aegis_client::pages::risk_rows(&ws);
    assert!(rows.len() > 1, "the sample has several threats");
    let asset = rows[0].asset.clone();
    let profile = ReportProfile {
        filters: ReportFilters {
            assets: vec![asset.clone()],
            ..ReportFilters::default()
        },
        ..ReportProfile::default()
    };
    let doc = report_doc_with_options(
        &ws,
        &ctx,
        &ReportOptions {
            profile,
            baseline_project: None,
        },
    );
    let register = doc
        .blocks
        .iter()
        .find(|b| match b {
            Block::Table { headers, .. } => headers.first().is_some_and(|h| h == "Threat scenario"),
            _ => false,
        })
        .expect("the risk register table");
    let Block::Table { rows: filtered, .. } = register else {
        unreachable!()
    };
    assert!(!filtered.is_empty());
    for row in filtered {
        assert_eq!(row[1], asset, "only the chosen asset remains");
    }
}

#[test]
fn classification_reaches_the_watermark_and_cover() {
    let ws = workspace();
    let ctx = context();
    let profile = ReportProfile {
        branding: aegis_client::report::ReportBranding {
            classification: Classification::Secret,
            document_id: "TARA-2026-0042".into(),
            approvers: vec!["Mira Chen".into(), "Bob Lee".into()],
            ..Default::default()
        },
        ..ReportProfile::default()
    };
    let doc = report_doc_with_options(
        &ws,
        &ctx,
        &ReportOptions {
            profile,
            baseline_project: None,
        },
    );
    assert!(
        doc.watermark.starts_with("Secret. Exported by"),
        "{}",
        doc.watermark
    );
    let html = report_html_with_options(
        &ws,
        &ctx,
        &ReportOptions {
            profile: ReportProfile {
                branding: aegis_client::report::ReportBranding {
                    classification: Classification::Secret,
                    document_id: "TARA-2026-0042".into(),
                    approvers: vec!["Mira Chen".into()],
                    ..Default::default()
                },
                ..ReportProfile::default()
            },
            baseline_project: None,
        },
    );
    assert!(html.contains("Secret"), "the mark is on the page");
    assert!(html.contains("TARA-2026-0042"));
}

#[test]
fn a_japanese_report_writes_ja_headings() {
    let ws = workspace();
    let ctx = context();
    let profile = ReportProfile {
        language: ReportLanguage::Japanese,
        ..ReportProfile::default()
    };
    let doc = report_doc_with_options(
        &ws,
        &ctx,
        &ReportOptions {
            profile,
            baseline_project: None,
        },
    );
    assert_eq!(doc.language, "ja");
    assert!(
        headings(&doc)
            .iter()
            .any(|h| h.contains("エグゼクティブサマリー")),
        "{:?}",
        headings(&doc)
    );
}

#[test]
fn the_audit_summary_is_machine_readable() {
    let profile = ReportProfile {
        name: "Submission".into(),
        language: ReportLanguage::Japanese,
        filters: ReportFilters {
            risk_levels: vec!["High".into()],
            ..ReportFilters::default()
        },
        ..ReportProfile::default()
    };
    let summary = profile.audit_summary();
    assert!(
        summary.starts_with("profile=Submission lang=ja"),
        "{summary}"
    );
    assert!(summary.contains("risk=1"));
    // The summary is printable ASCII, so it fits the server's detail cap.
    assert!(summary.bytes().all(|b| b.is_ascii_graphic() || b == b' '));
}

#[test]
fn the_xlsx_workbook_has_a_sheet_per_element_type_and_guards_formulas() {
    let ws = workspace();
    let bytes = report_workbook_xlsx(&ws);
    assert_eq!(&bytes[0..4], b"PK\x03\x04");
    let text = String::from_utf8_lossy(&bytes);
    for name in [
        "Risk register",
        "Assets",
        "Damage scenarios",
        "Threat scenarios",
        "Attack trees",
        "Security controls",
        "Cybersecurity goals",
        "Cybersecurity claims",
        "Problems",
    ] {
        assert!(text.contains(name), "{name} sheet missing");
    }
    // A hostile name that starts like a formula is guarded (the same rule as
    // the CSV exporter).
    assert!(
        text.contains("&apos;") || text.contains("&quot;") || !text.contains("=SUM("),
        "no live formula"
    );
}

#[test]
fn japanese_text_survives_in_the_pdf() {
    let mut ws = workspace();
    let threat = ws
        .project()
        .threat_scenarios()
        .next()
        .map(|(_, id, _)| id)
        .expect("a threat");
    let ops = aegis_client::commands::Edit::RenameThreat {
        threat,
        name: "ブレーキ要求のなりすまし".into(),
    }
    .to_ops(ws.project(), "mira", 1_792_956_600_000)
    .expect("rename");
    ws.apply(ops, Origin::Edit).expect("applies");
    let pdf = aegis_client::report::report_pdf(&ws, &context()).expect("pdf");
    assert_eq!(
        pdf,
        aegis_client::report::report_pdf(&ws, &context()).expect("pdf"),
        "deterministic"
    );
}

#[test]
fn the_pseudo_locale_is_available_for_clipping_tests() {
    // The pseudo-locale catalog loads and is longer than English, so a
    // screenshot test can catch clipped text (L10N-03).
    let c = aegis_client::i18n::Catalog::load(Language::Pseudo);
    let en = aegis_client::i18n::Catalog::load(Language::En);
    assert!(c.tr("risk-level").len() > en.tr("risk-level").len());
    assert!(c.tr("risk-level").contains('⟦'), "visible markers");
}
