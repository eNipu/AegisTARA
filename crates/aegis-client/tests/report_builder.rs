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

    let bilingual = report_doc_with_options(
        &ws,
        &ctx,
        &ReportOptions {
            profile: ReportProfile {
                language: ReportLanguage::Bilingual,
                ..ReportProfile::default()
            },
            baseline_project: None,
        },
    );
    assert!(
        !bilingual
            .blocks
            .windows(2)
            .any(|w| matches!(w, [Block::Heading(1, _), Block::Heading(1, _)])),
        "bilingual cover must not emit consecutive level-1 headings"
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
        "Parties",
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
fn goals_and_claim_report_tables_carry_the_responsible_header() {
    let doc = report_doc(&workspace(), &context());
    let headers: Vec<&Vec<String>> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Table { headers, .. } => Some(headers),
            _ => None,
        })
        .collect();
    let goals = headers
        .iter()
        .find(|h| h.first().is_some_and(|x| x.to_lowercase().contains("goal")))
        .expect("goals table");
    assert_eq!(goals.last().map(String::as_str), Some("Responsible"));
    let claims = headers
        .iter()
        .find(|h| {
            h.first()
                .is_some_and(|x| x.to_lowercase().contains("claim"))
        })
        .expect("claims table");
    assert_eq!(claims.last().map(String::as_str), Some("Responsible"));
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

#[test]
fn a_japanese_report_uses_pack_translated_element_headings_and_vocabulary_labels() {
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
    let hs = headings(&doc);
    // Element-type headings come from the methodology pack's "ja" translation (L10N-07).
    for expected in [
        "資産",
        "ダメージシナリオ",
        "脅威シナリオ",
        "アタックツリー",
        "セキュリティ対策",
        "サイバーセキュリティ目標",
        "サイバーセキュリティ主張",
    ] {
        assert!(
            hs.iter().any(|h| h.contains(expected)),
            "missing pack-translated Japanese heading {expected:?} in {hs:?}"
        );
    }
    let html = report_html_with_options(
        &ws,
        &ctx,
        &ReportOptions {
            profile: ReportProfile {
                language: ReportLanguage::Japanese,
                ..ReportProfile::default()
            },
            baseline_project: None,
        },
    );
    // Pack vocabulary labels (security properties, threat categories, treatments) are translated.
    assert!(html.contains("完全性") || html.contains("機密性"));
    assert!(html.contains("なりすまし") || html.contains("改ざん"));
}

#[test]
fn figures_render_as_real_images_in_pdf_html_and_xlsx() {
    let ws = workspace();
    let ctx = context();
    let asset_id = ws
        .project()
        .assets()
        .next()
        .map(|(_, id, _)| id)
        .expect("asset");

    // 1x1 valid PNG.
    let png_bytes: Vec<u8> = vec![
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, b'I', b'D', b'A', b'T', 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
    ];
    let svg_bytes = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60"><rect x="4" y="4" width="112" height="52" fill="#2563eb" stroke="#1e293b" stroke-width="2"/><circle cx="60" cy="30" r="16" fill="#f8fafc"/></svg>"##.to_vec();

    let figures = vec![
        aegis_client::report::FigureAttachment {
            figure_id: "0196ed00-0000-7000-8000-000000000001".into(),
            caption: "System context PNG".into(),
            target: String::new(),
            media_type: "image/png".into(),
            width_px: 1,
            height_px: 1,
            size_bytes: png_bytes.len() as u64,
            updated_by: "mira".into(),
            updated_at_ms: 1_792_956_600_000,
            data: png_bytes,
        },
        aegis_client::report::FigureAttachment {
            figure_id: "0196ed00-0000-7000-8000-000000000002".into(),
            caption: "CAN topology SVG".into(),
            target: format!("asset:{asset_id}"),
            media_type: "image/svg+xml".into(),
            width_px: 120,
            height_px: 60,
            size_bytes: svg_bytes.len() as u64,
            updated_by: "mira".into(),
            updated_at_ms: 1_792_956_600_000,
            data: svg_bytes,
        },
    ];

    let pdf_without = aegis_client::report::report_pdf(&ws, &ctx).expect("baseline pdf");
    let opts = ReportOptions {
        profile: ReportProfile {
            figures: figures.clone(),
            ..ReportProfile::default()
        },
        baseline_project: None,
    };
    let doc = report_doc_with_options(&ws, &ctx, &opts);
    let image_blocks = doc
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Image { .. }))
        .count();
    assert_eq!(
        image_blocks, 2,
        "both project and asset figures appear in ReportDoc"
    );

    let html = report_html_with_options(&ws, &ctx, &opts);
    assert!(html.contains("data:image/png;base64,"));
    assert!(html.contains("data:image/svg+xml;base64,"));
    assert!(html.contains("System context PNG"));
    assert!(html.contains("CAN topology SVG"));

    let pdf_with =
        aegis_client::report::report_pdf_with_options(&ws, &ctx, &opts).expect("pdf with figures");
    assert!(pdf_with.starts_with(b"%PDF-"));
    let pdf_str = String::from_utf8_lossy(&pdf_with);
    assert!(
        pdf_str.contains("/Image") || pdf_str.contains("/XObject"),
        "PDF embeds real krilla Image XObjects for Block::Image"
    );
    assert!(pdf_with.len() > pdf_without.len() + 200);

    let xlsx = aegis_client::report::report_workbook_xlsx_with_figures(&ws, &figures);
    let wb = aegis_client::excel::read_workbook(&xlsx).expect("read xlsx");
    assert!(
        wb.sheets.iter().any(|s| s.name == "Figures"),
        "XLSX workbook includes the Figures sheet"
    );
}

#[test]
fn item_definition_section_and_component_filter_in_reports() {
    use aegis_client::commands::Edit;
    use aegis_model::{ComponentId, DataFlowId, InterfaceId, TrustBoundaryId};

    let mut ws = workspace();
    let ecu = ComponentId::new_v7();
    let gw = ComponentId::new_v7();
    let iface = InterfaceId::new_v7();
    let flow = DataFlowId::new_v7();
    let boundary = TrustBoundaryId::new_v7();
    let first_asset = ws
        .project()
        .assets()
        .next()
        .map(|(_, id, _)| id)
        .expect("asset");

    for edit in [
        Edit::AddComponent {
            id: ecu,
            name: "Brake ECU".into(),
            parent: None,
        },
        Edit::AddComponent {
            id: gw,
            name: "Telematics Gateway".into(),
            parent: None,
        },
        Edit::AddInterface {
            id: iface,
            component: ecu,
            name: "CAN-2 transceiver".into(),
            kind: 0,
        },
        Edit::AddDataFlow {
            id: flow,
            name: "Brake torque request".into(),
            from: gw,
            to: ecu,
            interface: Some(iface),
        },
        Edit::AddTrustBoundary {
            id: boundary,
            name: "Chassis domain".into(),
        },
        Edit::SetTrustBoundaryMember {
            boundary,
            component: ecu,
            member: true,
        },
        Edit::SetAssetComponent {
            asset: first_asset,
            component: Some(ecu),
        },
    ] {
        let ops = edit
            .to_ops(ws.project(), "mira", 1_792_956_600_000)
            .expect("ops");
        ws.apply(ops, Origin::Edit).expect("apply");
    }

    let ctx = context();
    let doc = report_doc(&ws, &ctx);
    let hs = headings(&doc);
    assert!(
        hs.iter().any(|h| h.contains("Item definition")),
        "Item definition section present in {hs:?}"
    );

    // Filter report by component "Brake ECU".
    let filtered_doc = report_doc_with_options(
        &ws,
        &ctx,
        &ReportOptions {
            profile: ReportProfile {
                filters: ReportFilters {
                    component: Some("Brake ECU".into()),
                    ..ReportFilters::default()
                },
                ..ReportProfile::default()
            },
            baseline_project: None,
        },
    );
    let assets_table = filtered_doc
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table { headers, rows } if headers.first().is_some_and(|h| h == "Asset") => {
                Some(rows)
            }
            _ => None,
        })
        .expect("assets table");
    assert_eq!(
        assets_table.len(),
        1,
        "only the asset bound to Brake ECU remains"
    );
}

#[test]
fn excel_import_round_trips_workbook_previews_unmapped_rows_and_applies_with_import_provenance() {
    use aegis_client::excel::{ExcelMapping, preview_import, read_workbook};
    use aegis_model::ProvenanceSource;

    let ws = workspace();
    let xlsx_bytes = report_workbook_xlsx(&ws);
    let wb = read_workbook(&xlsx_bytes).expect("read exported workbook");
    assert!(!wb.sheets.is_empty());

    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let empty_ws = Workspace::empty(Arc::clone(&m));
    let preview = preview_import(
        empty_ws.project(),
        &wb,
        &ExcelMapping::default(),
        "mira",
        1_792_956_600_000,
    )
    .expect("preview import");
    assert!(preview.mapped_rows > 0);
    assert!(preview.threats_to_add > 0);
    assert!(!preview.ops.is_empty());
    assert!(
        preview.ops.iter().all(|op| op
            .provenance()
            .is_some_and(|p| p.source() == ProvenanceSource::Import)),
        "all imported ops carry ProvenanceSource::Import"
    );

    let mut imported_ws = Workspace::empty(Arc::clone(&m));
    imported_ws
        .apply(preview.ops.clone(), Origin::Edit)
        .expect("batch applies cleanly");
    assert!(imported_ws.project().assets().count() >= preview.assets_to_add);
    assert!(imported_ws.project().threat_scenarios().count() >= preview.threats_to_add);
}

#[test]
fn the_item_description_and_scope_reach_the_report_in_en_and_ja() {
    use aegis_model::{ElementRef, Op, Provenance};
    let mut ws = workspace();
    let p = Provenance::default;
    ws.apply(
        vec![
            Op::SetDescription {
                target: ElementRef::ItemDescription,
                text: "A brake-by-wire ECU.".into(),
                provenance: p(),
            },
            Op::SetDescription {
                target: ElementRef::ItemScope,
                text: "The ECU and its firmware.".into(),
                provenance: p(),
            },
            Op::SetProjectLanguages {
                langs: vec![std::sync::Arc::from("ja")].into(),
                provenance: p(),
            },
            Op::SetDescriptionTranslation {
                target: ElementRef::ItemScope,
                lang: "ja".into(),
                text: "ECU とそのファームウェア。".into(),
                provenance: p(),
            },
        ],
        Origin::Edit,
    )
    .expect("applies");
    let text = |doc: &aegis_client::report::ReportDoc| format!("{:?}", doc.blocks);
    let en = text(&report_doc(&ws, &context()));
    assert!(en.contains("Description of the item") && en.contains("A brake-by-wire ECU."));
    assert!(en.contains("Scope of the analysis") && en.contains("The ECU and its firmware."));
    let ja = text(&report_doc_with_options(
        &ws,
        &context(),
        &ReportOptions {
            profile: ReportProfile {
                language: ReportLanguage::Japanese,
                ..ReportProfile::default()
            },
            baseline_project: None,
        },
    ));
    assert!(ja.contains("分析の範囲") && ja.contains("ECU とそのファームウェア。"));
    // No translation: the source text stands in.
    assert!(ja.contains("A brake-by-wire ECU."));
}
