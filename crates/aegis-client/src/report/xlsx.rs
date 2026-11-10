//! XLSX export (RPT-05): a risk-register sheet and a full workbook with one
//! sheet per element type. Written by this crate's own minimal XLSX writer
//! (a ZIP of stored XML parts) so no spreadsheet library joins the shipped
//! graph; `scripts/deny.sh` already covers the tree, and the writer adds no
//! dependency.
//!
//! Every cell is written as an inline string and text that would be read as
//! a formula by a spreadsheet (`=`, `+`, `-`, `@`, and leading tab/CR) gets a
//! leading `'`, the same guard the CSV exporter uses (OWASP CSV/formula
//! injection). Cell text is XML-escaped and control characters are dropped,
//! so a hostile project name cannot break the file.

use std::fmt::Write as _;

use crate::pages;
use crate::workspace::Workspace;

use super::ReportContext;

/// A sheet: its name and rows; the first row is the header (bold).
struct Sheet {
    name: String,
    rows: Vec<Vec<String>>,
}

/// Prefixes a spreadsheet formula marker so it stays text (formula
/// injection). O(len).
fn guard(text: &str) -> String {
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_owned()
    }
}

/// Escapes XML text and drops control characters XML 1.0 forbids. O(len).
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() && c != '\n' && c != '\r' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn cell_text(text: &str) -> String {
    xml_escape(&guard(text))
}

/// The Excel column name for a 0-based index: 0 -> `A`, 25 -> `Z`, 26 -> `AA`.
fn col_name(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.push(char::from(b'A' + u8::try_from(n % 26).unwrap_or(0)));
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.chars().rev().collect()
}

/// A sheet name is at most 31 characters and cannot hold `[]:*?/\`.
fn sheet_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '[' | ']' | ':' | '*' | '?' | '/' | '\\' => ' ',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim();
    let mut out: String = trimmed.chars().take(31).collect();
    if out.is_empty() {
        out = "Sheet".to_owned();
    }
    out
}

/// Makes sheet names unique by appending ` (2)`, ` (3)`, ... when a name
/// repeats.
fn unique_names(sheets: &mut [Sheet]) {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    for sheet in sheets {
        let base = sheet.name.clone();
        let mut candidate = base.clone();
        let mut i = 2usize;
        while !seen.insert(candidate.clone()) {
            candidate = format!("{base} ({i})");
            i += 1;
        }
        sheet.name = candidate;
    }
}

fn worksheet_xml(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>\n",
    );
    for (r, row) in rows.iter().enumerate() {
        let _ = write!(out, "<row r=\"{}\">", r + 1);
        for (c, cell) in row.iter().enumerate() {
            let style = if r == 0 { 1 } else { 0 };
            let _ = write!(
                out,
                "<c r=\"{}{}\" s=\"{style}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                col_name(c),
                r + 1,
                cell_text(cell)
            );
        }
        out.push_str("</row>\n");
    }
    out.push_str("</sheetData></worksheet>\n");
    out
}

/// Builds the XLSX parts and zips them (stored, no compression: the cells
/// are small and this keeps the writer dependency-free and deterministic).
fn workbook_bytes(sheets: &[Sheet]) -> Vec<u8> {
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();

    let mut content_types = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\n\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\n\
         <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\n",
    );
    let mut workbook_rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n",
    );
    let mut sheets_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>\n",
    );

    for (i, sheet) in sheets.iter().enumerate() {
        let n = i + 1;
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\n"
        ));
        let _ = writeln!(
            sheets_xml,
            "<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>",
            xml_escape(&sheet.name)
        );
        let _ = writeln!(
            workbook_rels,
            "<Relationship Id=\"rId{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>"
        );
        parts.push((
            format!("xl/worksheets/sheet{n}.xml"),
            worksheet_xml(&sheet.rows).into_bytes(),
        ));
    }
    content_types.push_str("</Types>\n");
    sheets_xml.push_str("</sheets></workbook>\n");
    let _ = writeln!(
        workbook_rels,
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>",
        sheets.len() + 1
    );

    parts.push(("_rels/.rels".to_owned(), b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>\n".to_vec()));
    parts.push(("xl/workbook.xml".to_owned(), sheets_xml.into_bytes()));
    parts.push((
        "xl/_rels/workbook.xml.rels".to_owned(),
        workbook_rels.into_bytes(),
    ));
    parts.push(("xl/styles.xml".to_owned(), b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"2\"><font/><font><b/></font></fonts><fills count=\"1\"><fill/></fills><borders count=\"1\"><border/></borders><cellStyleXfs count=\"1\"><xf/></cellStyleXfs><cellXfs count=\"2\"><xf/><xf fontId=\"1\"/></cellXfs></styleSheet>\n".to_vec()));
    parts.push(("[Content_Types].xml".to_owned(), content_types.into_bytes()));

    // Deterministic order: the XML parts, then the relationship parts, then
    // content types, matching how readers expect the archive laid out.
    zip_stored(&parts)
}

// ---- the ZIP writer (stored entries only) ---------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, entry) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *entry = c;
    }
    let mut c = u32::MAX;
    for byte in data {
        c = table[((c ^ u32::from(*byte)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ u32::MAX
}

/// A minimal ZIP with stored (uncompressed) entries, enough for XLSX. The
/// names are ASCII, the sizes are small, and the output is deterministic.
fn zip_stored(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let mut offset = 0u32;
    for (name, data) in files {
        let name_bytes = name.as_bytes();
        let crc = crc32(data);
        let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
        let name_len = u16::try_from(name_bytes.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]); // local file header
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        out.extend_from_slice(&0u32.to_le_bytes()); // mod time/date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra length
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]); // central directory
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // method
        central.extend_from_slice(&0u32.to_le_bytes()); // mod time/date
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&len.to_le_bytes());
        central.extend_from_slice(&len.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra length
        central.extend_from_slice(&0u16.to_le_bytes()); // comment length
        central.extend_from_slice(&0u16.to_le_bytes()); // disk start
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes()); // local header offset
        central.extend_from_slice(name_bytes);

        offset = offset
            .saturating_add(30)
            .saturating_add(u32::from(name_len))
            .saturating_add(len);
    }
    let central_len = u32::try_from(central.len()).unwrap_or(u32::MAX);
    let count = u16::try_from(files.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&central);
    out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]); // end of central directory
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&central_len.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out
}

// ---- the exports ----------------------------------------------------------

fn risk_register_sheet(ws: &Workspace) -> Sheet {
    let names = ws.project().methodology().element_names();
    let header = vec![
        "ID".to_owned(),
        names.threat_scenario.singular.to_string(),
        names.asset.singular.to_string(),
        format!("Deciding {}", names.damage_scenario.singular.to_lowercase()),
        "Impact".to_owned(),
        names.feasibility.to_string(),
        "Risk before controls".to_owned(),
        "Risk after controls".to_owned(),
        "Treatment".to_owned(),
        "Goal or claim".to_owned(),
    ];
    let mut rows = vec![header];
    for r in pages::risk_rows(ws) {
        let risk = |v: Option<&crate::view::RiskView>| {
            v.map_or_else(|| "not rated".to_owned(), |x| x.label.clone())
        };
        rows.push(vec![
            r.visual_id,
            r.threat,
            r.asset,
            r.damage,
            r.impact,
            r.feasibility,
            risk(r.inherent.as_ref()),
            risk(r.residual.as_ref()),
            r.treatment,
            r.backing,
        ]);
    }
    Sheet {
        name: "Risk register".to_owned(),
        rows,
    }
}

/// A workbook with a risk-register sheet and one sheet per element type
/// (RPT-05). Time O(project size).
#[must_use]
pub fn report_workbook_xlsx(ws: &Workspace) -> Vec<u8> {
    report_workbook_xlsx_with_figures(ws, &[])
}

/// A workbook with a risk-register sheet, one sheet per element type, and an
/// optional `Figures` sheet when figure attachments are present (RPT-05,
/// RPT-06, ARC-01, ARC-02).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn report_workbook_xlsx_with_figures(
    ws: &Workspace,
    figures: &[super::profile::FigureAttachment],
) -> Vec<u8> {
    let p = ws.project();
    let names = p.methodology().element_names();
    let vocab = pages::vocabulary(p.methodology());
    let mut sheets = Vec::new();
    let has_item = p.component_count() > 0
        || p.interface_count() > 0
        || p.data_flow_count() > 0
        || p.trust_boundary_count() > 0;

    // Overview: entity counts.
    let mut overview = vec![vec!["Element".to_owned(), "Count".to_owned()]];
    if has_item {
        for (label, count) in [
            (names.component.plural.to_string(), p.component_count()),
            (names.interface.plural.to_string(), p.interface_count()),
            (names.data_flow.plural.to_string(), p.data_flow_count()),
            (
                names.trust_boundary.plural.to_string(),
                p.trust_boundary_count(),
            ),
        ] {
            overview.push(vec![label, count.to_string()]);
        }
    }
    for (label, count) in [
        (names.asset.plural.to_string(), p.asset_count()),
        (
            names.damage_scenario.plural.to_string(),
            p.damage_scenario_count(),
        ),
        (
            names.threat_scenario.plural.to_string(),
            p.threat_scenario_count(),
        ),
        (names.attack_tree.plural.to_string(), p.attack_tree_count()),
        (
            names.security_control.plural.to_string(),
            p.security_control_count(),
        ),
        (
            names.cybersecurity_goal.plural.to_string(),
            p.cybersecurity_goal_count(),
        ),
        (
            names.cybersecurity_claim.plural.to_string(),
            p.cybersecurity_claim_count(),
        ),
    ] {
        overview.push(vec![label, count.to_string()]);
    }
    sheets.push(Sheet {
        name: "Overview".to_owned(),
        rows: overview,
    });

    sheets.push(risk_register_sheet(ws));

    if has_item {
        let mut comps = vec![vec![
            "ID".to_owned(),
            names.component.singular.to_string(),
            "Parent".to_owned(),
            names.interface.plural.to_string(),
            names.asset.plural.to_string(),
        ]];
        for c in pages::component_rows(ws) {
            comps.push(vec![
                c.visual_id,
                c.name,
                c.parent,
                c.interfaces.to_string(),
                c.assets.to_string(),
            ]);
        }
        sheets.push(Sheet {
            name: sheet_name(&names.component.plural),
            rows: comps,
        });

        let mut ifaces = vec![vec![
            "ID".to_owned(),
            names.interface.singular.to_string(),
            names.component.singular.to_string(),
            "Kind".to_owned(),
            names.data_flow.plural.to_string(),
        ]];
        for i in pages::interface_rows(ws) {
            ifaces.push(vec![
                i.visual_id,
                i.name,
                i.component,
                i.kind,
                i.flows.to_string(),
            ]);
        }
        sheets.push(Sheet {
            name: sheet_name(&names.interface.plural),
            rows: ifaces,
        });

        let mut flows = vec![vec![
            "ID".to_owned(),
            names.data_flow.singular.to_string(),
            "From component".to_owned(),
            "To component".to_owned(),
            names.interface.singular.to_string(),
        ]];
        for df in pages::data_flow_rows(ws) {
            flows.push(vec![df.visual_id, df.name, df.from, df.to, df.interface]);
        }
        sheets.push(Sheet {
            name: sheet_name(&names.data_flow.plural),
            rows: flows,
        });

        let mut bounds = vec![vec![
            "ID".to_owned(),
            names.trust_boundary.singular.to_string(),
            "Members".to_owned(),
        ]];
        for tb in pages::trust_boundary_rows(ws) {
            bounds.push(vec![tb.visual_id, tb.name, tb.members.to_string()]);
        }
        sheets.push(Sheet {
            name: sheet_name(&names.trust_boundary.plural),
            rows: bounds,
        });
    }

    // Assets.
    let mut assets = vec![vec!["ID".to_owned(), names.asset.singular.to_string()]];
    assets[0].extend(vocab.properties.iter().cloned());
    assets[0].push("Threats".to_owned());
    for a in pages::asset_rows(ws) {
        let mut row = vec![a.visual_id, a.name];
        row.extend(
            a.properties
                .iter()
                .map(|p| if *p { "yes" } else { "" }.to_owned()),
        );
        row.push(a.threats.to_string());
        assets.push(row);
    }
    sheets.push(Sheet {
        name: sheet_name(&names.asset.plural),
        rows: assets,
    });

    // Damage scenarios.
    let mut damage = vec![vec![
        "ID".to_owned(),
        names.damage_scenario.singular.to_string(),
    ]];
    damage[0].extend(vocab.impact_categories.iter().map(|c| c.label.clone()));
    damage[0].push("Impact rationale".to_owned());
    damage[0].push("Threats".to_owned());
    for d in pages::damage_rows(ws) {
        let rationale = ws
            .project()
            .damage_scenario_idx(&d.id)
            .and_then(|i| ws.project().damage_scenario(i))
            .map_or_else(String::new, |x| x.impact_rationale().to_owned());
        let mut row = vec![d.visual_id, d.name];
        for (i, level) in d.impacts.iter().enumerate() {
            row.push(
                level
                    .and_then(|l| vocab.impact_categories.get(i).and_then(|c| c.levels.get(l)))
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        row.push(rationale);
        row.push(d.threats.to_string());
        damage.push(row);
    }
    sheets.push(Sheet {
        name: sheet_name(&names.damage_scenario.plural),
        rows: damage,
    });

    // Threats.
    let mut threats = vec![vec![
        "ID".to_owned(),
        names.threat_scenario.singular.to_string(),
        names.asset.singular.to_string(),
        "Category".to_owned(),
        "Catalog references".to_owned(),
        names.damage_scenario.plural.to_string(),
        names.feasibility.to_string(),
        "Feasibility rationale".to_owned(),
        "Scales".to_owned(),
        "Scaling reason".to_owned(),
        "Risk before controls".to_owned(),
        "Risk after controls".to_owned(),
        "Treatment".to_owned(),
    ]];
    for r in pages::risk_rows(ws) {
        let t = pages::threat_detail(ws, r.id);
        let category = t
            .as_ref()
            .and_then(|t| vocab.threat_categories.get(t.category))
            .cloned()
            .unwrap_or_default();
        let refs = t
            .as_ref()
            .map_or(String::new(), |t| t.catalog_refs.join(", "));
        let risk = |v: Option<&crate::view::RiskView>| {
            v.map_or_else(|| "not rated".to_owned(), |x| x.label.clone())
        };
        threats.push(vec![
            r.visual_id,
            r.threat,
            r.asset,
            category,
            refs,
            r.damage,
            r.feasibility,
            t.as_ref()
                .map_or_else(String::new, |t| t.feasibility_rationale.clone()),
            match t.as_ref().and_then(|t| t.scales) {
                Some(true) => "yes",
                Some(false) => "no",
                None => "",
            }
            .to_owned(),
            t.as_ref()
                .map_or_else(String::new, |t| t.scaling_reason.clone()),
            risk(r.inherent.as_ref()),
            risk(r.residual.as_ref()),
            r.treatment,
        ]);
    }
    sheets.push(Sheet {
        name: sheet_name(&names.threat_scenario.plural),
        rows: threats,
    });

    // Attack trees.
    let mut trees = vec![vec![
        "ID".to_owned(),
        "Attack tree".to_owned(),
        "Nodes".to_owned(),
        names.threat_scenario.plural.to_string(),
        names.feasibility.to_string(),
    ]];
    for t in pages::tree_rows(ws) {
        trees.push(vec![
            t.visual_id,
            t.title,
            t.nodes.to_string(),
            t.threats,
            t.feasibility,
        ]);
    }
    sheets.push(Sheet {
        name: sheet_name(&names.attack_tree.plural),
        rows: trees,
    });

    // Attack steps, with what each goes through and its attack type (122c).
    let mut steps = vec![vec![
        "Tree ID".to_owned(),
        "Attack tree".to_owned(),
        "Attack step".to_owned(),
        names.feasibility.to_string(),
        "Through".to_owned(),
        "Attack type".to_owned(),
    ]];
    let project = ws.project();
    for t in pages::tree_rows(ws) {
        let Some(idx) = project.attack_tree_idx(&t.id) else {
            continue;
        };
        for s in crate::view::tree_steps(project, idx) {
            steps.push(vec![
                t.visual_id.clone(),
                t.title.clone(),
                s.step,
                s.feasibility.unwrap_or_default(),
                s.through.unwrap_or_default(),
                s.attack_type.unwrap_or_default(),
            ]);
        }
    }
    sheets.push(Sheet {
        name: "Attack steps".to_owned(),
        rows: steps,
    });

    // Controls.
    let mut controls = vec![vec![
        "ID".to_owned(),
        names.security_control.singular.to_string(),
        "Attack steps".to_owned(),
        "Goals".to_owned(),
    ]];
    for c in pages::control_rows(ws) {
        controls.push(vec![
            c.visual_id,
            c.name,
            c.steps.to_string(),
            c.goals.to_string(),
        ]);
    }
    sheets.push(Sheet {
        name: sheet_name(&names.security_control.plural),
        rows: controls,
    });

    // Goals and claims.
    let mut goals = vec![vec![
        "ID".to_owned(),
        names.cybersecurity_goal.singular.to_string(),
        "Controls".to_owned(),
        names.threat_scenario.plural.to_string(),
    ]];
    for g in pages::goal_rows(ws) {
        goals.push(vec![
            g.visual_id,
            g.name,
            g.controls.to_string(),
            g.threats.to_string(),
        ]);
    }
    sheets.push(Sheet {
        name: sheet_name(&names.cybersecurity_goal.plural),
        rows: goals,
    });

    let mut claims = vec![vec![
        "ID".to_owned(),
        names.cybersecurity_claim.singular.to_string(),
        names.threat_scenario.plural.to_string(),
    ]];
    for c in pages::claim_rows(ws) {
        claims.push(vec![c.visual_id, c.name, c.threats.to_string()]);
    }
    sheets.push(Sheet {
        name: sheet_name(&names.cybersecurity_claim.plural),
        rows: claims,
    });

    // Problems.
    let mut problems = vec![vec!["Severity".to_owned(), "Problem".to_owned()]];
    for p in pages::problems(ws) {
        problems.push(vec![
            if p.error { "Blocking" } else { "Warning" }.to_owned(),
            p.text,
        ]);
    }
    sheets.push(Sheet {
        name: "Problems".to_owned(),
        rows: problems,
    });

    if !figures.is_empty() {
        let mut fig_rows = vec![vec![
            "Figure ID".to_owned(),
            "Caption".to_owned(),
            "Target".to_owned(),
            "Format".to_owned(),
            "Dimensions".to_owned(),
            "Size (bytes)".to_owned(),
            "Updated by".to_owned(),
        ]];
        for f in figures {
            fig_rows.push(vec![
                f.figure_id.clone(),
                f.caption.clone(),
                if f.target.is_empty() {
                    "project".to_owned()
                } else {
                    f.target.clone()
                },
                f.media_type.clone(),
                format!("{}x{}", f.width_px, f.height_px),
                f.size_bytes.to_string(),
                f.updated_by.clone(),
            ]);
        }
        sheets.push(Sheet {
            name: "Figures".to_owned(),
            rows: fig_rows,
        });
    }

    unique_names(&mut sheets);
    workbook_bytes(&sheets)
}

/// The risk register alone, one sheet. Time O(threats log threats).
#[must_use]
pub fn risk_register_xlsx(ws: &Workspace, _ctx: &ReportContext) -> Vec<u8> {
    workbook_bytes(&[risk_register_sheet(ws)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_standard_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
        assert_ne!(crc32(b"a"), crc32(b"b"));
    }

    #[test]
    fn column_names_cover_a_to_zz() {
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(25), "Z");
        assert_eq!(col_name(26), "AA");
        assert_eq!(col_name(27), "AB");
        assert_eq!(col_name(701), "ZZ");
    }

    #[test]
    fn the_zip_has_the_expected_frame() {
        let bytes = zip_stored(&[
            ("a.txt".to_owned(), b"hello".to_vec()),
            ("b.txt".to_owned(), b"world".to_vec()),
        ]);
        assert_eq!(&bytes[0..4], b"PK\x03\x04");
        assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
        // The end-of-central-directory record counts two entries.
        let count = u16::from_le_bytes([bytes[bytes.len() - 12], bytes[bytes.len() - 11]]);
        assert_eq!(count, 2);
    }

    #[test]
    fn formula_guards_and_xml_escaping_are_applied() {
        assert_eq!(guard("=SUM(A1)"), "'=SUM(A1)");
        assert_eq!(guard("+1"), "'+1");
        assert_eq!(guard("-2"), "'-2");
        assert_eq!(guard("@x"), "'@x");
        assert_eq!(guard("plain"), "plain");
        assert_eq!(xml_escape("<a & b>"), "&lt;a &amp; b&gt;");
        assert_eq!(xml_escape("a\u{0000}b"), "ab");
        assert_eq!(cell_text("=cmd()"), "'=cmd()");
        assert_eq!(cell_text("<x>"), "&lt;x&gt;");
    }
}
