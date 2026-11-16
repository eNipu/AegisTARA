//! Reports and exports (jobs 041, 050, 070 to 072; ADRs 0037, 0044, 0056).
//!
//! One document model, [`ReportDoc`], feeds every export so HTML, PDF and
//! XLSX cannot drift apart (RPT-01). [`doc::report_doc`] turns the analysis
//! into blocks once; the renderers in this module turn the blocks into a
//! file. The risk register CSV and the attack-tree SVG stay beside them.
//!
//! A [`ReportProfile`](profile::ReportProfile) selects sections, filters
//! (risk level, asset, baseline), language and branding (logo, document ID,
//! revision history, approvers, classification), and is recorded in the
//! export audit event (RPT-02 to RPT-04).
//!
//! Output is deterministic (P4): the same project, profile, user and time
//! give the same bytes. Building a report is O(project size); it runs on the
//! blocking pool (R5).

mod csv;
pub mod doc;
mod html;
pub mod pdf;
pub mod profile;
pub mod xlsx;

pub use csv::{risk_register_csv, tree_svg};
pub use doc::{Block, Figure, Panel, ReportDoc, Style, report_doc, report_doc_with_options};
pub use html::{report_html, report_html_with_options};
pub use pdf::{MAX_PAGES, PdfError, report_pdf, report_pdf_with_options};
pub use profile::{
    Classification, FigureAttachment, ReportBranding, ReportFilters, ReportLanguage, ReportOptions,
    ReportProfile, ReportSections, RevisionEntry,
};
pub use xlsx::{
    report_workbook_xlsx, report_workbook_xlsx_in, report_workbook_xlsx_with_figures,
    risk_register_xlsx,
};

use crate::time::format_ms;

/// What to export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportKind {
    /// The HTML TARA report.
    Report,
    /// The TARA report as a PDF file.
    ReportPdf,
    /// The TARA report as an XLSX workbook.
    ReportXlsx,
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
            ExportKind::ReportXlsx => "xlsx-report",
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
            ExportKind::ReportXlsx => "xlsx",
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
    /// The watermark for exports without branding (CSV, SVG): the user and
    /// the time, marked confidential as the reports always were (job 050).
    #[must_use]
    pub fn watermark(&self) -> String {
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
/// run it as a formula (CSV injection, OWASP). The same guard is applied to
/// XLSX cell values (RPT-05).
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
