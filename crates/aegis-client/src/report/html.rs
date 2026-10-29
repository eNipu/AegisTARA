//! The TARA report as one self-contained HTML file (RPT-01). It renders the
//! shared [`ReportDoc`] so HTML and PDF come from the same blocks, with a
//! table of contents and anchor ids on every heading (ADR 0056).
//!
//! No network and no scripts: the file opens from disk and prints to PDF in
//! any browser. The attack-tree panels are inline SVG drawn by the same
//! display list the PDF embeds.

use std::fmt::Write as _;

use aegis_canvas::{Size as CanvasSize, SvgOptions, to_svg};

use crate::workspace::Workspace;

use super::doc::{Block, Figure, ReportDoc, Style};
use super::profile::ReportOptions;
use super::{ReportContext, esc};

const STYLE: &str = r"
body{font-family:Segoe UI,Helvetica,Arial,sans-serif;color:#1D2733;margin:32px auto;max-width:1100px;line-height:1.45}
h1{font-size:28px;margin:0 0 4px}h2{font-size:20px;border-bottom:2px solid #1D2733;padding-bottom:4px;margin-top:36px}
h3{font-size:16px;margin:20px 0 6px}
table{border-collapse:collapse;width:100%;margin:8px 0 16px;font-size:13px}
th,td{border:1px solid #C3CBD3;padding:4px 8px;text-align:left;vertical-align:top}th{background:#F3F5F7}
.muted{color:#5B6875}.draft{color:#D55E00;font-weight:700}
nav.toc{border:1px solid #C3CBD3;background:#F3F5F7;padding:12px 16px;margin:16px 0}
nav.toc ul{list-style:none;margin:0;padding:0}nav.toc li{margin:2px 0}
.mark{position:fixed;bottom:6px;right:12px;font-size:11px;color:#5B6875}
.svg{border:1px solid #C3CBD3;margin:8px 0;overflow:auto}.svg svg{max-width:100%;height:auto}
pre{background:#F3F5F7;padding:8px;font-size:12px;white-space:pre-wrap}
img.logo{max-height:120px;max-width:100%;margin:8px 0}
@media print{h2{page-break-before:always}.svg{border:none}nav.toc{page-break-after:always}}
";

fn heading_html(level: u8, text: &str, id: Option<&str>) -> String {
    let anchor = id.map_or_else(String::new, |i| format!("<a id=\"{i}\"></a>"));
    format!("{anchor}<h{level}>{}</h{level}>\n", esc(text))
}

fn paragraph_html(text: &str, style: Style) -> String {
    let class = match style {
        Style::Normal => "",
        Style::Muted => " class=\"muted\"",
        Style::Draft => " class=\"draft\"",
    };
    format!("<p{class}>{}</p>\n", esc(text))
}

fn table_html(headers: &[String], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str("<table><tr>");
    for h in headers {
        let _ = write!(out, "<th>{}</th>", esc(h));
    }
    out.push_str("</tr>\n");
    for row in rows {
        out.push_str("<tr>");
        for cell in row {
            let _ = write!(out, "<td>{}</td>", esc(cell));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

fn figure_html(figure: &Figure) -> String {
    let mut out = String::new();
    for panel in &figure.panels {
        let _ = writeln!(out, "<p class=\"muted\">{}</p>", esc(&panel.caption));
        let svg = to_svg(
            &panel.list,
            CanvasSize::new(panel.size.0, panel.size.1),
            &SvgOptions::light(),
        );
        let _ = writeln!(out, "<div class=\"svg\">{svg}</div>");
    }
    if figure.omitted > 0 {
        let _ = writeln!(
            out,
            "<p class=\"muted\">{} more detail panels are not drawn. The outline below lists every node.</p>",
            figure.omitted
        );
    }
    out
}

fn image_html(media_type: &str, bytes: &[u8], alt: &str) -> String {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!(
        "<figure class=\"img\"><img class=\"logo\" src=\"data:{media_type};base64,{b64}\" alt=\"{}\"><figcaption class=\"muted\">{}</figcaption></figure>\n",
        esc(alt),
        esc(alt)
    )
}

/// Renders a document to HTML. O(blocks).
fn render_html(doc: &ReportDoc) -> String {
    // Pass 1: assign anchor ids in block order and collect the level-2
    // headings for the table of contents.
    let mut anchors: Vec<Option<String>> = Vec::with_capacity(doc.blocks.len());
    let mut toc: Vec<(String, String)> = Vec::new();
    let mut n = 0usize;
    for block in &doc.blocks {
        match block {
            Block::Heading(level, text) => {
                n += 1;
                let id = format!("sec-{n}");
                if *level == 2 {
                    toc.push((id.clone(), text.clone()));
                }
                anchors.push(Some(id));
            }
            _ => anchors.push(None),
        }
    }

    let mut out = String::with_capacity(64 * 1024);
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>{}</title><style>{STYLE}</style></head><body>\n",
        esc(&doc.title)
    );
    let _ = writeln!(out, "<div class=\"mark\">{}</div>", esc(&doc.watermark));

    if !toc.is_empty() {
        out.push_str("<nav class=\"toc\"><strong>Contents</strong><ul>\n");
        for (id, text) in &toc {
            let _ = writeln!(out, "<li><a href=\"#{id}\">{}</a></li>", esc(text));
        }
        out.push_str("</ul></nav>\n");
    }

    for (i, block) in doc.blocks.iter().enumerate() {
        match block {
            Block::Heading(level, text) => {
                out.push_str(&heading_html(*level, text, anchors[i].as_deref()));
            }
            Block::Paragraph(text, style) => out.push_str(&paragraph_html(text, *style)),
            Block::Table { headers, rows } => out.push_str(&table_html(headers, rows)),
            Block::Preformatted(text) => {
                let _ = writeln!(out, "<pre>{}</pre>", esc(text));
            }
            Block::Figure(figure) => out.push_str(&figure_html(figure)),
            Block::Image {
                media_type,
                bytes,
                alt,
            } => out.push_str(&image_html(media_type, bytes, alt)),
        }
    }
    let _ = writeln!(
        out,
        "<p class=\"muted\">Generated by AegisTARA.</p></body></html>"
    );
    out
}

/// The full TARA report as one HTML document, every section, no filters.
/// Time O(project size).
#[must_use]
pub fn report_html(ws: &Workspace, ctx: &ReportContext) -> String {
    report_html_with_options(ws, ctx, &ReportOptions::default())
}

/// The report as HTML for a profile (sections, filters, language, branding).
/// Time O(project size).
#[must_use]
pub fn report_html_with_options(
    ws: &Workspace,
    ctx: &ReportContext,
    opts: &ReportOptions,
) -> String {
    render_html(&super::doc::report_doc_with_options(ws, ctx, opts))
}
