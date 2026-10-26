//! Reports and exports (job 041, docs/adr/0037-baselines-and-reports.md).
//!
//! Three exports of the open project, all written by this computer from the
//! same engine numbers the screen shows:
//!
//! - **TARA report** (HTML): an executive summary and the full work product
//!   (assets, damage, threats, attack trees as diagrams, risks, treatment,
//!   controls, goals and claims, open problems). One self-contained file
//!   with print styles, so "Print to PDF" in any browser makes the PDF. The
//!   attack tree diagrams come from the canvas display list (direction
//!   section 5), so they look exactly like the screen.
//! - **Risk register** (CSV, RFC 4180): one row per threat scenario, for a
//!   spreadsheet.
//! - **Attack tree** (SVG): the tree on the canvas.
//!
//! Every export carries a watermark with the user and the time (direction
//! section 6), and the session records it in the server's event log.
//! Output is deterministic (P4): the same project, user and time give the
//! same bytes, so reports can be hashed and compared. Building a report is
//! O(project size); it runs on the blocking pool (R5).

use std::fmt::Write as _;

use aegis_canvas::{RenderOptions, SvgOptions, scene_to_svg};
use aegis_model::Project;

use crate::outline::to_outline;
use crate::pages::{self, RiskRow, treatment_label};
use crate::time::format_ms;
use crate::view::tree_scene;
use crate::workspace::Workspace;

/// What to export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportKind {
    /// The HTML TARA report.
    Report,
    /// The TARA report as a PDF file.
    ReportPdf,
    /// The CSV risk register.
    RiskRegister,
    /// The SVG of the tree on the canvas.
    TreeSvg,
}

impl ExportKind {
    /// The name the server's event log records.
    #[must_use]
    pub fn audit_name(self) -> &'static str {
        match self {
            ExportKind::Report => "html-report",
            ExportKind::ReportPdf => "pdf-report",
            ExportKind::RiskRegister => "csv-risk-register",
            ExportKind::TreeSvg => "svg-attack-tree",
        }
    }

    /// The file extension.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            ExportKind::Report => "html",
            ExportKind::ReportPdf => "pdf",
            ExportKind::RiskRegister => "csv",
            ExportKind::TreeSvg => "svg",
        }
    }
}

/// Who exports what, for the watermark and the title page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportContext {
    /// Project name.
    pub project: String,
    /// The user.
    pub user: String,
    /// Export time, milliseconds since the Unix epoch.
    pub now_ms: u64,
    /// The server sequence number the content reflects.
    pub seq: u64,
    /// Whether unsaved edits are included (marked "Draft").
    pub unsaved: bool,
}

impl ReportContext {
    pub(crate) fn watermark(&self) -> String {
        format!(
            "Confidential. Exported by {} on {}.",
            self.user,
            format_ms(self.now_ms)
        )
    }
}

/// HTML-escapes text for element content and attribute values.
#[must_use]
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// One CSV field (RFC 4180): quoted when it holds a comma, quote or line
/// break; a leading `=`, `+`, `-` or `@` gets a `'` so spreadsheets do not
/// run it as a formula (CSV injection, OWASP).
#[must_use]
pub fn csv_field(text: &str) -> String {
    let guarded = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_owned()
    };
    if guarded.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

pub(crate) fn risk_text(r: Option<&crate::view::RiskView>) -> String {
    r.map_or_else(|| "not rated".to_owned(), |v| v.label.clone())
}

/// The risk register as CSV. Time O(threats log threats).
#[must_use]
pub fn risk_register_csv(ws: &Workspace, ctx: &ReportContext) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}", csv_field(&ctx.watermark()));
    out.push_str(
        "Threat scenario,Asset,Deciding damage scenario,Impact,Attack feasibility,Risk before controls,Risk after controls,Treatment,Goal or claim\r\n",
    );
    for r in pages::risk_rows(ws) {
        let fields = [
            r.threat.as_str(),
            &r.asset,
            &r.damage,
            &r.impact,
            &r.feasibility,
            &risk_text(r.inherent.as_ref()),
            &risk_text(r.residual.as_ref()),
            &r.treatment,
            &r.backing,
        ];
        let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
        out.push_str(&line.join(","));
        out.push_str("\r\n");
    }
    out
}

/// The SVG of one tree. `None` if the tree does not exist.
#[must_use]
pub fn tree_svg(ws: &Workspace, tree: aegis_model::TreeId, residual: bool) -> Option<String> {
    let scene = tree_scene(ws, tree, residual)?;
    Some(scene_to_svg(
        &scene,
        &SvgOptions::light(),
        &RenderOptions::default(),
    ))
}

const STYLE: &str = r"
body{font-family:Segoe UI,Helvetica,Arial,sans-serif;color:#1D2733;margin:32px auto;max-width:1100px;line-height:1.45}
h1{font-size:28px;margin:0 0 4px}h2{font-size:20px;border-bottom:2px solid #1D2733;padding-bottom:4px;margin-top:36px}
h3{font-size:16px;margin:20px 0 6px}
table{border-collapse:collapse;width:100%;margin:8px 0 16px;font-size:13px}
th,td{border:1px solid #C3CBD3;padding:4px 8px;text-align:left;vertical-align:top}th{background:#F3F5F7}
.muted{color:#5B6875}.draft{color:#D55E00;font-weight:700}
.t1{background:#D9F2EA}.t2{background:#DDF0FB}.t3{background:#FBEBCC}.t4{background:#F7DCCC}
.mark{position:fixed;bottom:6px;right:12px;font-size:11px;color:#5B6875}
.svg{border:1px solid #C3CBD3;margin:8px 0;overflow:auto}.svg svg{max-width:100%;height:auto}
pre{background:#F3F5F7;padding:8px;font-size:12px;white-space:pre-wrap}
@media print{h2{page-break-before:always}.svg{border:none}}
";

fn tone_class(t: aegis_canvas::Tone) -> &'static str {
    match t {
        aegis_canvas::Tone::Unrated => "",
        aegis_canvas::Tone::VeryLow => "t1",
        aegis_canvas::Tone::Low => "t2",
        aegis_canvas::Tone::Medium => "t3",
        aegis_canvas::Tone::High => "t4",
    }
}

fn risk_cell(r: Option<&crate::view::RiskView>) -> String {
    match r {
        Some(v) => format!(
            "<td class=\"{}\">{}</td>",
            tone_class(v.tone),
            esc(&v.label)
        ),
        None => "<td class=\"muted\">not rated</td>".to_owned(),
    }
}

fn table(out: &mut String, headers: &[&str], rows: impl IntoIterator<Item = Vec<String>>) {
    out.push_str("<table><tr>");
    for h in headers {
        let _ = write!(out, "<th>{}</th>", esc(h));
    }
    out.push_str("</tr>");
    let mut any = false;
    for row in rows {
        any = true;
        out.push_str("<tr>");
        for cell in row {
            if cell.starts_with("<td") {
                out.push_str(&cell);
            } else {
                let _ = write!(out, "<td>{}</td>", esc(&cell));
            }
        }
        out.push_str("</tr>");
    }
    if !any {
        let _ = write!(
            out,
            "<tr><td colspan=\"{}\" class=\"muted\">None.</td></tr>",
            headers.len()
        );
    }
    out.push_str("</table>\n");
}

fn risk_table(out: &mut String, rows: &[RiskRow]) {
    table(
        out,
        &[
            "Threat scenario",
            "Asset",
            "Deciding damage",
            "Impact",
            "Feasibility",
            "Risk before",
            "Risk after",
            "Treatment",
            "Goal or claim",
        ],
        rows.iter().map(|r| {
            vec![
                r.threat.clone(),
                r.asset.clone(),
                r.damage.clone(),
                r.impact.clone(),
                r.feasibility.clone(),
                risk_cell(r.inherent.as_ref()),
                risk_cell(r.residual.as_ref()),
                r.treatment.clone(),
                r.backing.clone(),
            ]
        }),
    );
}

pub(crate) fn methodology_line(project: &Project) -> String {
    let m = project.methodology();
    format!("{} (pack {} {})", m.label(), m.key(), m.version())
}

/// The full TARA report as one HTML document. Time O(project size).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn report_html(ws: &Workspace, ctx: &ReportContext) -> String {
    let project = ws.project();
    let m = project.methodology();
    let vocab = pages::vocabulary(m);
    let mut out = String::with_capacity(64 * 1024);
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>{} - TARA report</title><style>{STYLE}</style></head><body>\n",
        esc(&ctx.project)
    );
    let _ = writeln!(out, "<div class=\"mark\">{}</div>", esc(&ctx.watermark()));
    let _ = writeln!(
        out,
        "<h1>{}</h1><p class=\"muted\">Threat analysis and risk assessment. Methodology: {}. State of the server at change {}. Engine {}.</p>",
        esc(&ctx.project),
        esc(&methodology_line(project)),
        ctx.seq,
        esc(aegis_model::ENGINE_VERSION)
    );
    if ctx.unsaved {
        out.push_str("<p class=\"draft\">Draft: this report includes changes that are not saved on the server yet.</p>\n");
    }

    // ---- executive summary ----
    let ov = pages::overview(ws);
    out.push_str("<h2>1. Executive summary</h2>\n");
    let above: usize = ov.residual.iter().take(2).map(|c| c.count).sum();
    let _ = writeln!(
        out,
        "<p>The project has {} threat scenarios on {} assets. After the planned and verified controls, {} threat scenario(s) remain in the two highest risk levels. {} blocking problem(s) and {} warning(s) are open.</p>",
        project.threat_scenario_count(),
        project.asset_count(),
        above,
        ov.errors,
        ov.warnings
    );
    out.push_str("<h3>Risks by level</h3>\n");
    table(
        &mut out,
        &["Risk level", "Before controls", "After controls"],
        ov.inherent.iter().zip(&ov.residual).map(|(a, b)| {
            vec![
                format!(
                    "<td class=\"{}\">{}</td>",
                    tone_class(a.tone),
                    esc(&a.label)
                ),
                a.count.to_string(),
                b.count.to_string(),
            ]
        }),
    );
    out.push_str("<h3>Highest risks</h3>\n");
    risk_table(&mut out, &ov.top);

    // ---- assets ----
    out.push_str("<h2>2. Assets</h2>\n");
    let mut headers: Vec<&str> = vec!["Asset"];
    headers.extend(vocab.properties.iter().map(String::as_str));
    headers.push("Threats");
    table(
        &mut out,
        &headers,
        pages::asset_rows(ws).into_iter().map(|a| {
            let mut row = vec![a.name];
            row.extend(
                a.properties
                    .iter()
                    .map(|p| if *p { "yes" } else { "" }.to_owned()),
            );
            row.push(a.threats.to_string());
            row
        }),
    );

    // ---- damage ----
    out.push_str("<h2>3. Damage scenarios</h2>\n");
    let mut headers: Vec<&str> = vec!["Damage scenario"];
    headers.extend(vocab.impact_categories.iter().map(|c| c.label.as_str()));
    headers.push("Threats");
    table(
        &mut out,
        &headers,
        pages::damage_rows(ws).into_iter().map(|d| {
            let mut row = vec![d.name];
            for (i, level) in d.impacts.iter().enumerate() {
                row.push(
                    level
                        .and_then(|l| vocab.impact_categories.get(i).and_then(|c| c.levels.get(l)))
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            row.push(d.threats.to_string());
            row
        }),
    );

    // ---- threats ----
    out.push_str("<h2>4. Threat scenarios</h2>\n");
    for (_, id, _) in project.threat_scenarios() {
        let Some(t) = pages::threat_detail(ws, id) else {
            continue;
        };
        let _ = writeln!(out, "<h3>{}</h3>", esc(&t.name));
        let damages: Vec<&str> = t
            .damages
            .iter()
            .filter(|d| d.linked)
            .map(|d| d.name.as_str())
            .collect();
        table(
            &mut out,
            &["Field", "Value"],
            [
                vec!["Asset".into(), t.asset_name.clone()],
                vec![
                    "Category".into(),
                    vocab
                        .threat_categories
                        .get(t.category)
                        .cloned()
                        .unwrap_or_default(),
                ],
                vec!["Catalog references".into(), t.catalog_refs.join(", ")],
                vec!["Damage scenarios".into(), damages.join(", ")],
                vec![
                    "Attack feasibility".into(),
                    t.feasibility.clone().unwrap_or_else(|| "not rated".into()),
                ],
                vec![
                    "Risk before controls".into(),
                    risk_text(t.inherent.as_ref()),
                ],
                vec!["Risk after controls".into(), risk_text(t.residual.as_ref())],
                vec![
                    "Treatment".into(),
                    t.treatment
                        .and_then(|i| crate::commands::TREATMENTS.get(i))
                        .map(|x| treatment_label(*x).to_owned())
                        .unwrap_or_default(),
                ],
            ],
        );
        for line in &t.why {
            let _ = writeln!(out, "<p class=\"muted\">{}</p>", esc(line));
        }
    }

    // ---- attack trees ----
    out.push_str("<h2>5. Attack trees</h2>\n");
    let trees = pages::tree_rows(ws);
    if trees.is_empty() {
        out.push_str("<p class=\"muted\">None.</p>\n");
    }
    for row in trees {
        let _ = writeln!(
            out,
            "<h3>{}</h3><p class=\"muted\">{} nodes. Feasibility at the root: {}. Threat scenarios: {}.</p>",
            esc(&row.title),
            row.nodes,
            esc(&row.feasibility),
            esc(if row.threats.is_empty() {
                "none"
            } else {
                &row.threats
            })
        );
        if let Some(svg) = tree_svg(ws, row.id, false) {
            // The SVG adapter escapes its own text (aegis-canvas).
            let _ = writeln!(out, "<div class=\"svg\">{svg}</div>");
        }
        if let Some(tree) = project
            .attack_tree_idx(&row.id)
            .and_then(|i| project.attack_tree(i))
        {
            let _ = writeln!(out, "<pre>{}</pre>", esc(&to_outline(tree)));
        }
    }

    // ---- risks ----
    out.push_str("<h2>6. Risk register and treatment</h2>\n");
    risk_table(&mut out, &pages::risk_rows(ws));

    // ---- controls ----
    out.push_str("<h2>7. Security controls</h2>\n");
    table(
        &mut out,
        &["Control", "Attack steps", "Goals"],
        pages::control_rows(ws)
            .into_iter()
            .map(|c| vec![c.name, c.steps.to_string(), c.goals.to_string()]),
    );

    // ---- goals and claims ----
    out.push_str("<h2>8. Cybersecurity goals and claims</h2>\n<h3>Goals</h3>\n");
    table(
        &mut out,
        &["Goal", "Controls", "Threat scenarios"],
        pages::goal_rows(ws)
            .into_iter()
            .map(|g| vec![g.name, g.controls.to_string(), g.threats.to_string()]),
    );
    out.push_str("<h3>Claims</h3>\n");
    table(
        &mut out,
        &["Claim", "Threat scenarios"],
        pages::claim_rows(ws)
            .into_iter()
            .map(|c| vec![c.name, c.threats.to_string()]),
    );

    // ---- problems ----
    out.push_str("<h2>9. Open problems</h2>\n");
    table(
        &mut out,
        &["Severity", "Problem"],
        pages::problems(ws).into_iter().map(|p| {
            vec![
                if p.error { "Blocking" } else { "Warning" }.to_owned(),
                p.text,
            ]
        }),
    );
    let _ = writeln!(
        out,
        "<p class=\"muted\">{} Generated by AegisTARA.</p></body></html>",
        esc(&ctx.watermark())
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        assert_eq!(
            esc("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("=cmd()"), "'=cmd()");
        assert_eq!(csv_field("-1,2"), "\"'-1,2\"");
    }
}
