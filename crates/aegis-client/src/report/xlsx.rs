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

use aegis_model::{Applicability, ElementRef, Project, SystemSet, SystemUse, ThreatId};

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

/// What reduces a risk, or why nothing does. O(steps).
fn reduced_by_cell(r: &pages::RiskRow) -> String {
    if let Some(text) = pages::reduced_by_text(r) {
        text
    } else if let Some(step) = &r.deciding_step {
        format!("Not reduced; deciding step: {}", step.label)
    } else {
        String::new()
    }
}

/// Which rows the workbook keeps and their last "Systems" cell (121b).
struct Rows<'a> {
    p: &'a aegis_model::Project,
    has_systems: bool,
    usage: SystemUse,
    scope: crate::systems::Scope,
}

impl<'a> Rows<'a> {
    /// Time O(project) when the project has systems, O(1) otherwise.
    fn new(p: &'a aegis_model::Project, view: &crate::systems::SystemView) -> Self {
        let has_systems = p
            .elements(aegis_model::ElementKind::System)
            .next()
            .is_some();
        let usage = if has_systems {
            p.system_use()
        } else {
            SystemUse::default()
        };
        let scope = crate::systems::Scope::new(p, view);
        Self {
            p,
            has_systems,
            usage,
            scope,
        }
    }

    /// Puts "Systems" last in the header row when the project has systems. Time O(1).
    fn head(&self, rows: &mut [Vec<String>]) {
        match rows.first_mut() {
            Some(h) if self.has_systems => h.push("Systems".to_owned()),
            _ => {}
        }
    }

    /// Whether an element shows in the scope. Time O(1) plus `set`.
    fn shows(&self, set: impl Fn(&SystemUse) -> Option<SystemSet>) -> bool {
        self.scope.shows(set)
    }

    /// `None` when the element does not show; otherwise its Systems cell,
    /// or no cell when the project has no systems. Time O(s log n) plus `set`.
    fn cell(&self, set: impl Fn(&SystemUse) -> Option<SystemSet>) -> Option<Vec<String>> {
        if !self.scope.shows(&set) {
            return None;
        }
        Some(if self.has_systems {
            let s = set(&self.usage);
            vec![s.map_or_else(String::new, |s| {
                crate::systems::systems_text(self.p, &self.usage, s)
            })]
        } else {
            Vec::new()
        })
    }
}

/// Appends `cell` to the last row. Time O(1).
fn end_row(rows: &mut [Vec<String>], cell: Vec<String>) {
    if let Some(r) = rows.last_mut() {
        r.extend(cell);
    }
}

fn excluded_cell(p: &Project, app: Option<&Applicability>, eref: ElementRef) -> Option<String> {
    let app = app?;
    Some(if app.is_excluded(eref) {
        crate::exclusion::reasons_text(p, app.reasons(eref))
    } else {
        String::new()
    })
}

fn risk_cell(
    ws: &Workspace,
    has_conditions: bool,
    threat: ThreatId,
    residual: bool,
    view: Option<&crate::view::RiskView>,
) -> String {
    if has_conditions && crate::exclusion::threat_risk_not_applicable(ws, threat, residual) {
        "not applicable".to_owned()
    } else {
        view.map_or_else(|| "not rated".to_owned(), |x| x.label.clone())
    }
}

fn risk_register_sheet(ws: &Workspace, sys: &Rows<'_>) -> Sheet {
    let p = ws.project();
    let has_conditions = p.conditions().next().is_some();
    let app = has_conditions.then(|| p.applicability());
    let names = p.methodology().element_names();
    let mut header = vec![
        "ID".to_owned(),
        names.threat_scenario.singular.to_string(),
        names.asset.singular.to_string(),
        format!("Deciding {}", names.damage_scenario.singular.to_lowercase()),
        "Impact".to_owned(),
        names.feasibility.to_string(),
        "Risk before controls".to_owned(),
        "Risk after controls".to_owned(),
        "Reduced by".to_owned(),
        "Treatment".to_owned(),
        "Goal or claim".to_owned(),
    ];
    if has_conditions {
        header.push("Excluded".to_owned());
    }
    let mut rows = vec![header];
    sys.head(&mut rows);
    for r in pages::risk_rows(ws) {
        let Some(cell) = sys.cell(|u| Some(u.threat(p.threat_scenario_idx(&r.id)?))) else {
            continue;
        };
        let reduced = reduced_by_cell(&r);
        let mut row = vec![
            r.visual_id,
            r.threat,
            r.asset,
            r.damage,
            r.impact,
            r.feasibility,
            risk_cell(ws, has_conditions, r.id, false, r.inherent.as_ref()),
            risk_cell(ws, has_conditions, r.id, true, r.residual.as_ref()),
            reduced,
            r.treatment,
            r.backing,
        ];
        if let Some(ex) = excluded_cell(p, app.as_ref(), ElementRef::Threat(r.id)) {
            row.push(ex);
        }
        rows.push(row);
        end_row(&mut rows, cell);
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
pub fn report_workbook_xlsx_with_figures(
    ws: &Workspace,
    figures: &[super::profile::FigureAttachment],
) -> Vec<u8> {
    report_workbook_xlsx_in(ws, figures, &crate::systems::SystemView::all())
}

/// The workbook of what `view` shows (121b). When the project has systems,
/// every element sheet ends with a "Systems" column; with a view of some
/// systems, rows of other systems are left out and the Overview counts the
/// rows of each sheet. A project with no systems gives the same bytes as
/// `report_workbook_xlsx_with_figures`. Time O(project size).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn report_workbook_xlsx_in(
    ws: &Workspace,
    figures: &[super::profile::FigureAttachment],
    view: &crate::systems::SystemView,
) -> Vec<u8> {
    let p = ws.project();
    let has_conditions = p.conditions().next().is_some();
    let app = has_conditions.then(|| p.applicability());
    let sys = Rows::new(p, view);
    let names = p.methodology().element_names();
    let vocab = pages::vocabulary(p.methodology());
    let mut sheets = Vec::new();
    // Data rows of each element sheet, in Overview order (121b).
    let mut counts: Vec<usize> = Vec::new();
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

    sheets.push(risk_register_sheet(ws, &sys));

    if has_item {
        let mut comps = vec![vec![
            "ID".to_owned(),
            names.component.singular.to_string(),
            "Parent".to_owned(),
            names.interface.plural.to_string(),
            names.asset.plural.to_string(),
        ]];
        if has_conditions {
            comps[0].push("Excluded".to_owned());
        }
        sys.head(&mut comps);
        for c in pages::component_rows(ws) {
            let Some(cell) = sys.cell(|u| Some(u.component(p.component_idx(&c.id)?))) else {
                continue;
            };
            let mut row = vec![
                c.visual_id,
                c.name,
                c.parent,
                c.interfaces.to_string(),
                c.assets.to_string(),
            ];
            if let Some(ex) = excluded_cell(p, app.as_ref(), ElementRef::Component(c.id)) {
                row.push(ex);
            }
            comps.push(row);
            end_row(&mut comps, cell);
        }
        counts.push(comps.len().saturating_sub(1));
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
        if has_conditions {
            ifaces[0].push("Excluded".to_owned());
        }
        sys.head(&mut ifaces);
        for i in pages::interface_rows(ws) {
            let Some(cell) = sys.cell(|u| Some(u.interface(p.interface_idx(&i.id)?))) else {
                continue;
            };
            let mut row = vec![
                i.visual_id,
                i.name,
                i.component,
                i.kind,
                i.flows.to_string(),
            ];
            if let Some(ex) = excluded_cell(p, app.as_ref(), ElementRef::Interface(i.id)) {
                row.push(ex);
            }
            ifaces.push(row);
            end_row(&mut ifaces, cell);
        }
        counts.push(ifaces.len().saturating_sub(1));
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
        sys.head(&mut flows);
        for df in pages::data_flow_rows(ws) {
            let Some(cell) = sys.cell(|u| Some(u.data_flow(p.data_flow_idx(&df.id)?))) else {
                continue;
            };
            flows.push(vec![df.visual_id, df.name, df.from, df.to, df.interface]);
            end_row(&mut flows, cell);
        }
        counts.push(flows.len().saturating_sub(1));
        sheets.push(Sheet {
            name: sheet_name(&names.data_flow.plural),
            rows: flows,
        });

        let mut bounds = vec![vec![
            "ID".to_owned(),
            names.trust_boundary.singular.to_string(),
            "Members".to_owned(),
        ]];
        sys.head(&mut bounds);
        for tb in pages::trust_boundary_rows(ws) {
            let Some(cell) = sys.cell(|u| Some(u.trust_boundary(p.trust_boundary_idx(&tb.id)?)))
            else {
                continue;
            };
            bounds.push(vec![tb.visual_id, tb.name, tb.members.to_string()]);
            end_row(&mut bounds, cell);
        }
        counts.push(bounds.len().saturating_sub(1));
        sheets.push(Sheet {
            name: sheet_name(&names.trust_boundary.plural),
            rows: bounds,
        });

        let mut settings = vec![vec![
            "ID".to_owned(),
            "Applied to".to_owned(),
            "Setting ID".to_owned(),
            "Setting".to_owned(),
            "Rules".to_owned(),
        ]];
        for a in pages::applied_settings(ws, "en") {
            if !sys.shows(|u| match &a.target {
                aegis_model::ItemRef::Component(id) => Some(u.component(p.component_idx(id)?)),
                aegis_model::ItemRef::Interface(id) => Some(u.interface(p.interface_idx(id)?)),
            }) {
                continue;
            }
            settings.push(vec![
                a.target_visual_id,
                a.target_label,
                a.control_visual_id,
                a.name,
                a.rules,
            ]);
        }
        sheets.push(Sheet {
            name: "Security settings".to_owned(),
            rows: settings,
        });
    }

    // Assets.
    let mut assets = vec![vec!["ID".to_owned(), names.asset.singular.to_string()]];
    assets[0].extend(vocab.properties.iter().cloned());
    assets[0].push("Threats".to_owned());
    if has_conditions {
        assets[0].push("Excluded".to_owned());
    }
    sys.head(&mut assets);
    for a in pages::asset_rows(ws) {
        let Some(cell) = sys.cell(|u| Some(u.asset(p.asset_idx(&a.id)?))) else {
            continue;
        };
        let mut row = vec![a.visual_id, a.name];
        row.extend(
            a.properties
                .iter()
                .map(|p| if *p { "yes" } else { "" }.to_owned()),
        );
        row.push(a.threats.to_string());
        if let Some(ex) = excluded_cell(p, app.as_ref(), ElementRef::Asset(a.id)) {
            row.push(ex);
        }
        assets.push(row);
        end_row(&mut assets, cell);
    }
    counts.push(assets.len().saturating_sub(1));
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
    sys.head(&mut damage);
    for d in pages::damage_rows(ws) {
        let Some(cell) = sys.cell(|u| Some(u.damage(p.damage_scenario_idx(&d.id)?))) else {
            continue;
        };
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
        end_row(&mut damage, cell);
    }
    counts.push(damage.len().saturating_sub(1));
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
        "Pair rationale".to_owned(),
        "Scales".to_owned(),
        "Scaling reason".to_owned(),
        "Risk before controls".to_owned(),
        "Risk after controls".to_owned(),
        "Reduced by".to_owned(),
        "Treatment".to_owned(),
    ]];
    if has_conditions {
        threats[0].push("Excluded".to_owned());
    }
    sys.head(&mut threats);
    for r in pages::risk_rows(ws) {
        let Some(cell) = sys.cell(|u| Some(u.threat(p.threat_scenario_idx(&r.id)?))) else {
            continue;
        };
        let t = pages::threat_detail(ws, r.id);
        let category = t
            .as_ref()
            .and_then(|t| vocab.threat_categories.get(t.category))
            .cloned()
            .unwrap_or_default();
        let refs = t
            .as_ref()
            .map_or(String::new(), |t| t.catalog_refs.join(", "));
        let reduced = reduced_by_cell(&r);
        let mut row = vec![
            r.visual_id,
            r.threat,
            r.asset,
            category,
            refs,
            r.damage,
            r.feasibility,
            t.as_ref()
                .map_or_else(String::new, |t| t.feasibility_rationale.clone()),
            r.rationale,
            match t.as_ref().and_then(|t| t.scales) {
                Some(true) => "yes",
                Some(false) => "no",
                None => "",
            }
            .to_owned(),
            t.as_ref()
                .map_or_else(String::new, |t| t.scaling_reason.clone()),
            risk_cell(ws, has_conditions, r.id, false, r.inherent.as_ref()),
            risk_cell(ws, has_conditions, r.id, true, r.residual.as_ref()),
            reduced,
            r.treatment,
        ];
        if let Some(ex) = excluded_cell(p, app.as_ref(), ElementRef::Threat(r.id)) {
            row.push(ex);
        }
        threats.push(row);
        end_row(&mut threats, cell);
    }
    counts.push(threats.len().saturating_sub(1));
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
    sys.head(&mut trees);
    for t in pages::tree_rows(ws) {
        let Some(cell) = sys.cell(|u| Some(u.tree(p.attack_tree_idx(&t.id)?))) else {
            continue;
        };
        trees.push(vec![
            t.visual_id,
            t.title,
            t.nodes.to_string(),
            t.threats,
            t.feasibility,
        ]);
        end_row(&mut trees, cell);
    }
    counts.push(trees.len().saturating_sub(1));
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
        if !sys.shows(|u| Some(u.tree(idx))) {
            continue;
        }
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
        "Risks lowered".to_owned(),
    ]];
    sys.head(&mut controls);
    for c in pages::control_rows(ws) {
        let Some(cell) = sys.cell(|u| Some(u.control(p.security_control_idx(&c.id)?))) else {
            continue;
        };
        controls.push(vec![
            c.visual_id,
            c.name,
            c.steps.to_string(),
            c.goals.to_string(),
            c.lowers.to_string(),
        ]);
        end_row(&mut controls, cell);
    }
    counts.push(controls.len().saturating_sub(1));
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
        "Responsible".to_owned(),
    ]];
    sys.head(&mut goals);
    for g in pages::goal_rows(ws) {
        let Some(cell) = sys.cell(|u| Some(u.goal(p.cybersecurity_goal_idx(&g.id)?))) else {
            continue;
        };
        goals.push(vec![
            g.visual_id,
            g.name,
            g.controls.to_string(),
            g.threats.to_string(),
            g.responsible.join(", "),
        ]);
        end_row(&mut goals, cell);
    }
    counts.push(goals.len().saturating_sub(1));
    sheets.push(Sheet {
        name: sheet_name(&names.cybersecurity_goal.plural),
        rows: goals,
    });

    let mut claims = vec![vec![
        "ID".to_owned(),
        names.cybersecurity_claim.singular.to_string(),
        names.threat_scenario.plural.to_string(),
        "Responsible".to_owned(),
    ]];
    sys.head(&mut claims);
    for c in pages::claim_rows(ws) {
        let Some(cell) = sys.cell(|u| Some(u.claim(p.cybersecurity_claim_idx(&c.id)?))) else {
            continue;
        };
        claims.push(vec![
            c.visual_id,
            c.name,
            c.threats.to_string(),
            c.responsible.join(", "),
        ]);
        end_row(&mut claims, cell);
    }
    counts.push(claims.len().saturating_sub(1));
    sheets.push(Sheet {
        name: sheet_name(&names.cybersecurity_claim.plural),
        rows: claims,
    });

    // Parties (FND-08). Built from the whole project, not a system view, so
    // the count equals the rows of this sheet (L22).
    let mut parties = vec![vec!["ID".to_owned(), "Name".to_owned(), "Kind".to_owned()]];
    for (idx, _, party) in p.parties() {
        parties.push(vec![
            p.party_visual_id(idx).unwrap_or_default(),
            party.name().to_owned(),
            party.kind().as_str().to_owned(),
        ]);
    }
    counts.push(parties.len().saturating_sub(1));
    sheets.push(Sheet {
        name: "Parties".to_owned(),
        rows: parties,
    });

    // Problems.
    let mut problems = vec![vec!["Severity".to_owned(), "Problem".to_owned()]];
    for p in pages::problems(ws) {
        if !sys.shows(|u| crate::systems::selection_systems(ws.project(), u, p.target?)) {
            continue;
        }
        problems.push(vec![
            if p.error { "Blocking" } else { "Warning" }.to_owned(),
            p.text,
        ]);
    }
    sheets.push(Sheet {
        name: "Problems".to_owned(),
        rows: problems,
    });

    if has_conditions {
        let catalog = crate::i18n::Catalog::load(crate::i18n::Language::En);
        let mut cond_rows = vec![vec![
            "ID".to_owned(),
            "Name".to_owned(),
            "Present".to_owned(),
            "Needed by".to_owned(),
        ]];
        for row in crate::configuration::configuration_view(ws, &catalog).rows {
            cond_rows.push(vec![
                row.visual_id,
                row.name,
                if row.present { "yes" } else { "no" }.to_owned(),
                (row.needed_by_elements + row.needed_by_steps).to_string(),
            ]);
        }
        sheets.push(Sheet {
            name: "Conditions".to_owned(),
            rows: cond_rows,
        });
    }

    let figures: Vec<&super::profile::FigureAttachment> = figures
        .iter()
        .filter(|f| crate::systems::figure_shows(p, &sys.scope, &f.target))
        .collect();
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

    // Each Overview row (after the header) takes the data-row count of its
    // own sheet, by position, so repeated methodology names cannot mix them.
    if sys.scope.is_on()
        && let Some(overview) = sheets.first_mut()
    {
        for (row, count) in overview.rows.iter_mut().skip(1).zip(&counts) {
            if let Some(cell) = row.get_mut(1) {
                *cell = count.to_string();
            }
        }
    }
    unique_names(&mut sheets);
    workbook_bytes(&sheets)
}

/// The risk register alone, one sheet. Time O(threats log threats).
#[must_use]
pub fn risk_register_xlsx(ws: &Workspace, _ctx: &ReportContext) -> Vec<u8> {
    let all = Rows::new(ws.project(), &crate::systems::SystemView::all());
    workbook_bytes(&[risk_register_sheet(ws, &all)])
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

    #[test]
    fn parties_sheet_and_responsible_columns_are_exported_and_guarded() {
        use aegis_model::{ClaimId, GoalId, Methodology, Op, PartyId, PartyKind, Provenance};

        use crate::excel::read_workbook;
        use crate::workspace::Origin;

        let m = std::sync::Arc::new(Methodology::iso21434_r155().expect("builtin"));
        let mut ws = Workspace::empty(std::sync::Arc::clone(&m));
        let party = PartyId::new_v7();
        let goal = GoalId::new_v7();
        let claim = ClaimId::new_v7();
        let prov = Provenance::human("mira", 1_792_956_600_000);
        ws.apply(
            vec![
                Op::AddParty {
                    id: party,
                    name: "=Bosch Corp".into(),
                    kind: PartyKind::Supplier,
                    provenance: prov.clone(),
                },
                Op::AddCybersecurityGoal {
                    id: goal,
                    name: "Firmware authenticity".into(),
                    controls: vec![],
                    provenance: prov.clone(),
                },
                Op::AddCybersecurityClaim {
                    id: claim,
                    name: "Secure boot enforced".into(),
                    provenance: prov.clone(),
                },
                Op::LinkGoalParty {
                    goal,
                    party,
                    provenance: prov.clone(),
                },
                Op::LinkClaimParty {
                    claim,
                    party,
                    provenance: prov,
                },
            ],
            Origin::Edit,
        )
        .expect("applies");

        let wb = read_workbook(&report_workbook_xlsx(&ws)).expect("reads workbook");

        let parties = wb
            .sheets
            .iter()
            .find(|s| s.name == "Parties")
            .expect("Parties sheet");
        assert_eq!(parties.headers, vec!["ID", "Name", "Kind"]);
        assert_eq!(parties.rows.len(), 1);
        assert_eq!(parties.rows[0].1[0], "P00");
        assert_eq!(parties.rows[0].1[1], "'=Bosch Corp", "formula guarded");
        assert_eq!(parties.rows[0].1[2], "supplier");

        let goals = wb
            .sheets
            .iter()
            .find(|s| s.name == "Cybersecurity goals")
            .expect("goals sheet");
        assert_eq!(
            goals.headers.last().map(String::as_str),
            Some("Responsible")
        );
        let grow = goals
            .rows
            .iter()
            .find(|(_, r)| r[1] == "Firmware authenticity")
            .expect("goal row");
        assert!(grow.1.last().is_some_and(|c| c.contains("Bosch Corp")));

        let claims = wb
            .sheets
            .iter()
            .find(|s| s.name == "Cybersecurity claims")
            .expect("claims sheet");
        assert_eq!(
            claims.headers.last().map(String::as_str),
            Some("Responsible")
        );
        let crow = claims
            .rows
            .iter()
            .find(|(_, r)| r[1] == "Secure boot enforced")
            .expect("claim row");
        assert!(crow.1.last().is_some_and(|c| c.contains("Bosch Corp")));
    }
}
