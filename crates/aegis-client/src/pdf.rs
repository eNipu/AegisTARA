//! The TARA report as a PDF (job 050, ADR 0044).
//!
//! Three steps, each small and testable:
//!
//! 1. [`report_doc`] turns the analysis into a [`ReportDoc`]: headings,
//!    paragraphs, tables and the attack tree outlines, with the same
//!    sections as the HTML report.
//! 2. `layout` places the blocks on A4 pages: greedy line wrapping with the
//!    real glyph widths of the embedded font, long words broken at the
//!    column edge, tables that continue on the next page with their header
//!    row repeated, and a row that is taller than a page split line by line.
//! 3. `render` writes the pages with `krilla`, adds the footer (the
//!    watermark and "Page n of N") and embeds the subsets of the font.
//!
//! The output is a function of the analysis and the [`ReportContext`] only
//! (the creation date is the export time in the context), so the same input
//! gives the same bytes (rule P4).
//!
//! Text is placed left to right, glyph by glyph, with no kerning, ligatures
//! or right to left support. That fits the report (Latin script, names and
//! numbers) and avoids the text shaper that `krilla` can bring along, which
//! is unmaintained (advisory RUSTSEC-2026-0206). Characters the font does
//! not have are printed as `?`.
//!
//! Time and memory: O(project size). Rule R3 does not apply (no input is
//! parsed); R6: the page count is capped by [`MAX_PAGES`].

use std::collections::HashMap;
use std::sync::Arc;

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size};
use krilla::metadata::{DateTime, Metadata};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, Stroke as PdfStroke, StrokeDash};
use krilla::text::{Font, GlyphId, KrillaGlyph};
use skrifa::instance::{LocationRef, Size as FontSize};
use skrifa::{FontRef, MetadataProvider};

use aegis_canvas::{
    Align, DisplayList, Item as CanvasItem, Paint, RenderOptions, Size as CanvasSize, SvgTheme,
    TileLimits, plan_tiles, render as render_view,
};

use crate::commands::TREATMENTS;
use crate::outline::to_outline;
use crate::pages::{self, treatment_label};
use crate::report::{ReportContext, methodology_line, risk_text};
use crate::workspace::Workspace;

const REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.otf");
const BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.otf");

/// Most pages a report may have. A project that needs more is cut off with a
/// sentence on the last page (R6).
pub const MAX_PAGES: usize = 2000;

// ---- the document ---------------------------------------------------------

/// How a paragraph looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Body text.
    Normal,
    /// Grey, for notes.
    Muted,
    /// Bold, for the draft warning.
    Draft,
}

/// One block of the report.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// A heading of level 1 (title), 2 (section) or 3.
    Heading(u8, String),
    /// A paragraph.
    Paragraph(String, Style),
    /// A table. No rows means "None.".
    Table {
        /// Column titles.
        headers: Vec<String>,
        /// Rows of cells.
        rows: Vec<Vec<String>>,
    },
    /// Lines whose indentation matters (an attack tree outline).
    Preformatted(String),
    /// A drawing: an attack tree as one or more panels.
    Figure(Figure),
}

/// A drawing made of panels, each a canvas display list on its own frame.
///
/// A tree that fits a page readably is one panel. A big tree is an overview
/// panel (the whole tree, boxes only) followed by detail panels in reading
/// order; see `aegis_canvas::tiles` for the rules.
#[derive(Clone, Debug, PartialEq)]
pub struct Figure {
    /// The panels, in reading order.
    pub panels: Vec<Panel>,
    /// Detail panels left out because the figure has a limit.
    pub omitted: usize,
}

/// One framed drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    /// Printed above the frame.
    pub caption: String,
    /// Frame width and height in points.
    pub size: (f32, f32),
    /// What to draw, in frame coordinates (origin at the top-left corner).
    pub list: Arc<DisplayList>,
}

/// Panel area on a page: the full text width and this height in points.
const FIGURE_H: f32 = 540.0;

/// The report before it is laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportDoc {
    /// Document title for the PDF properties.
    pub title: String,
    /// Printed at the foot of every page.
    pub watermark: String,
    /// The content.
    pub blocks: Vec<Block>,
}

/// The panels for one attack tree (the inherent view, as the HTML report
/// shows it). `None` if the tree is gone or has no nodes.
///
/// Cost: the layout is linear in the nodes; the plan is O(n log n) and draws
/// at most [`TileLimits::max_panels`] + 1 panels, whatever the tree's size.
fn tree_figure(ws: &Workspace, tree: aegis_model::TreeId) -> Option<Figure> {
    let scene = crate::view::tree_scene(ws, tree, false)?;
    if scene.is_empty() {
        return None;
    }
    let area = CanvasSize::new(CONTENT_W, FIGURE_H);
    let plan = plan_tiles(&scene, area, &TileLimits::default());
    let options = RenderOptions::default();
    let panel = |caption: String, view: &aegis_canvas::Viewport| Panel {
        caption,
        size: (view.size.w, view.size.h),
        list: Arc::new(render_view(&scene, view, &options)),
    };
    let mut panels = Vec::new();
    let caption = if plan.needs_detail {
        "Overview of the whole tree. Detail panels follow."
    } else {
        "Attack tree"
    };
    panels.push(panel(caption.to_owned(), &plan.overview));
    for tile in &plan.detail {
        panels.push(panel(
            format!(
                "Detail: row {} of {}, column {} of {}",
                tile.row + 1,
                plan.rows,
                tile.col + 1,
                plan.cols
            ),
            &tile.view,
        ));
    }
    Some(Figure {
        panels,
        omitted: plan.omitted,
    })
}

/// Description text as paragraphs, one per line, so line breaks survive.
fn desc_blocks(b: &mut Vec<Block>, text: &str) {
    for line in text.lines() {
        b.push(Block::Paragraph(line.to_owned(), Style::Normal));
    }
}

/// The descriptions of a section, if any element has one.
fn desc_list(b: &mut Vec<Block>, items: &[(String, String)]) {
    if items.is_empty() {
        return;
    }
    b.push(Block::Heading(3, "Descriptions".to_owned()));
    for (name, text) in items {
        b.push(Block::Paragraph(name.clone(), Style::Muted));
        desc_blocks(b, text);
    }
}

fn table(headers: &[&str], rows: Vec<Vec<String>>) -> Block {
    Block::Table {
        headers: headers.iter().map(|h| (*h).to_owned()).collect(),
        rows,
    }
}

fn risk_rows(rows: &[pages::RiskRow]) -> Block {
    table(
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
        rows.iter()
            .map(|r| {
                vec![
                    r.threat.clone(),
                    r.asset.clone(),
                    r.damage.clone(),
                    r.impact.clone(),
                    r.feasibility.clone(),
                    risk_text(r.inherent.as_ref()),
                    risk_text(r.residual.as_ref()),
                    r.treatment.clone(),
                    r.backing.clone(),
                ]
            })
            .collect(),
    )
}

/// The report as blocks. The sections and their order are those of
/// [`report_html`](crate::report::report_html); a test keeps them equal.
/// Time O(project size).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn report_doc(ws: &Workspace, ctx: &ReportContext) -> ReportDoc {
    use Block::{Heading, Paragraph};
    let project = ws.project();
    let m = project.methodology();
    let vocab = pages::vocabulary(m);
    let mut b: Vec<Block> = Vec::new();

    b.push(Heading(1, ctx.project.clone()));
    b.push(Paragraph(
        format!(
            "Threat analysis and risk assessment. Methodology: {}. State of the server at change {}. Engine {}.",
            methodology_line(project),
            ctx.seq,
            aegis_model::ENGINE_VERSION
        ),
        Style::Muted,
    ));
    if ctx.unsaved {
        b.push(Paragraph(
            "Draft: this report includes changes that are not saved on the server yet.".to_owned(),
            Style::Draft,
        ));
    }

    if let Some(notes) = crate::report::description_of(ws, aegis_model::ElementRef::Project) {
        b.push(Heading(2, "Project notes".to_owned()));
        desc_blocks(&mut b, &notes);
    }

    // ---- executive summary ----
    let ov = pages::overview(ws);
    b.push(Heading(2, "1. Executive summary".to_owned()));
    let above: usize = ov.residual.iter().take(2).map(|c| c.count).sum();
    b.push(Paragraph(
        format!(
            "The project has {} threat scenarios on {} assets. After the planned and verified controls, {} threat scenario(s) remain in the two highest risk levels. {} blocking problem(s) and {} warning(s) are open.",
            project.threat_scenario_count(),
            project.asset_count(),
            above,
            ov.errors,
            ov.warnings
        ),
        Style::Normal,
    ));
    b.push(Heading(3, "Risks by level".to_owned()));
    b.push(table(
        &["Risk level", "Before controls", "After controls"],
        ov.inherent
            .iter()
            .zip(&ov.residual)
            .map(|(a, c)| vec![a.label.clone(), a.count.to_string(), c.count.to_string()])
            .collect(),
    ));
    b.push(Heading(3, "Highest risks".to_owned()));
    b.push(risk_rows(&ov.top));

    // ---- assets ----
    b.push(Heading(2, "2. Assets".to_owned()));
    let mut headers: Vec<&str> = vec!["Asset"];
    headers.extend(vocab.properties.iter().map(String::as_str));
    headers.push("Threats");
    b.push(table(
        &headers,
        pages::asset_rows(ws)
            .into_iter()
            .map(|a| {
                let mut row = vec![a.name];
                row.extend(
                    a.properties
                        .iter()
                        .map(|p| if *p { "yes" } else { "" }.to_owned()),
                );
                row.push(a.threats.to_string());
                row
            })
            .collect(),
    ));
    desc_list(
        &mut b,
        &crate::report::section_descriptions(ws, crate::report::Described::Assets),
    );

    // ---- damage ----
    b.push(Heading(2, "3. Damage scenarios".to_owned()));
    let mut headers: Vec<&str> = vec!["Damage scenario"];
    headers.extend(vocab.impact_categories.iter().map(|c| c.label.as_str()));
    headers.push("Threats");
    b.push(table(
        &headers,
        pages::damage_rows(ws)
            .into_iter()
            .map(|d| {
                let mut row = vec![d.name];
                for (i, level) in d.impacts.iter().enumerate() {
                    row.push(
                        level
                            .and_then(|l| {
                                vocab.impact_categories.get(i).and_then(|c| c.levels.get(l))
                            })
                            .cloned()
                            .unwrap_or_default(),
                    );
                }
                row.push(d.threats.to_string());
                row
            })
            .collect(),
    ));
    desc_list(
        &mut b,
        &crate::report::section_descriptions(ws, crate::report::Described::Damage),
    );

    // ---- threats ----
    b.push(Heading(2, "4. Threat scenarios".to_owned()));
    for (_, id, _) in project.threat_scenarios() {
        let Some(t) = pages::threat_detail(ws, id) else {
            continue;
        };
        b.push(Heading(3, t.name.clone()));
        if let Some(text) = crate::report::description_of(ws, aegis_model::ElementRef::Threat(id)) {
            desc_blocks(&mut b, &text);
        }
        let damages: Vec<&str> = t
            .damages
            .iter()
            .filter(|d| d.linked)
            .map(|d| d.name.as_str())
            .collect();
        b.push(table(
            &["Field", "Value"],
            vec![
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
                        .and_then(|i| TREATMENTS.get(i))
                        .map(|x| treatment_label(*x).to_owned())
                        .unwrap_or_default(),
                ],
            ],
        ));
        for line in &t.why {
            b.push(Paragraph(line.clone(), Style::Muted));
        }
    }

    // ---- attack trees ----
    b.push(Heading(2, "5. Attack trees".to_owned()));
    let trees = pages::tree_rows(ws);
    if trees.is_empty() {
        b.push(Paragraph("None.".to_owned(), Style::Muted));
    }
    for row in trees {
        b.push(Heading(3, row.title.clone()));
        if let Some(text) = crate::report::description_of(ws, aegis_model::ElementRef::Tree(row.id))
        {
            desc_blocks(&mut b, &text);
        }
        b.push(Paragraph(
            format!(
                "{} nodes. Feasibility at the root: {}. Threat scenarios: {}.",
                row.nodes,
                row.feasibility,
                if row.threats.is_empty() {
                    "none"
                } else {
                    &row.threats
                }
            ),
            Style::Muted,
        ));
        if let Some(figure) = tree_figure(ws, row.id) {
            b.push(Block::Figure(figure));
        }
        if let Some(tree) = project
            .attack_tree_idx(&row.id)
            .and_then(|i| project.attack_tree(i))
        {
            b.push(Block::Preformatted(to_outline(tree)));
        }
    }

    // ---- risks ----
    b.push(Heading(2, "6. Risk register and treatment".to_owned()));
    b.push(risk_rows(&pages::risk_rows(ws)));

    // ---- controls ----
    b.push(Heading(2, "7. Security controls".to_owned()));
    b.push(table(
        &["Control", "Attack steps", "Goals"],
        pages::control_rows(ws)
            .into_iter()
            .map(|c| vec![c.name, c.steps.to_string(), c.goals.to_string()])
            .collect(),
    ));
    desc_list(
        &mut b,
        &crate::report::section_descriptions(ws, crate::report::Described::Controls),
    );

    // ---- goals and claims ----
    b.push(Heading(2, "8. Cybersecurity goals and claims".to_owned()));
    b.push(Heading(3, "Goals".to_owned()));
    b.push(table(
        &["Goal", "Controls", "Threat scenarios"],
        pages::goal_rows(ws)
            .into_iter()
            .map(|g| vec![g.name, g.controls.to_string(), g.threats.to_string()])
            .collect(),
    ));
    desc_list(
        &mut b,
        &crate::report::section_descriptions(ws, crate::report::Described::Goals),
    );
    b.push(Heading(3, "Claims".to_owned()));
    b.push(table(
        &["Claim", "Threat scenarios"],
        pages::claim_rows(ws)
            .into_iter()
            .map(|c| vec![c.name, c.threats.to_string()])
            .collect(),
    ));
    desc_list(
        &mut b,
        &crate::report::section_descriptions(ws, crate::report::Described::Claims),
    );

    // ---- problems ----
    b.push(Heading(2, "9. Open problems".to_owned()));
    b.push(table(
        &["Severity", "Problem"],
        pages::problems(ws)
            .into_iter()
            .map(|p| {
                vec![
                    if p.error { "Blocking" } else { "Warning" }.to_owned(),
                    p.text,
                ]
            })
            .collect(),
    ));

    ReportDoc {
        title: format!("{} - TARA report", ctx.project),
        watermark: ctx.watermark(),
        blocks: b,
    }
}

// ---- fonts and measuring --------------------------------------------------

/// One face: its bytes for krilla, and its metrics for measuring.
struct Face {
    font: Font,
    parsed: FontRef<'static>,
    units_per_em: f32,
    /// char -> (glyph, advance in em). Lookup only, never iterated (P4).
    cache: HashMap<char, (u32, f32)>,
}

impl Face {
    fn new(bytes: &'static [u8]) -> Result<Face, PdfError> {
        let font = Font::new(Arc::new(bytes.to_vec()).into(), 0).ok_or(PdfError::Font)?;
        let parsed = FontRef::new(bytes).map_err(|_| PdfError::Font)?;
        let units_per_em = f32::from(
            parsed
                .metrics(FontSize::unscaled(), LocationRef::default())
                .units_per_em,
        );
        Ok(Face {
            font,
            parsed,
            units_per_em,
            cache: HashMap::new(),
        })
    }

    /// The glyph and advance (in em) of `c`; `?` if the font lacks it.
    fn glyph(&mut self, c: char) -> (u32, f32) {
        if let Some(g) = self.cache.get(&c) {
            return *g;
        }
        let found = self
            .parsed
            .charmap()
            .map(c)
            .filter(|g| g.to_u32() != 0)
            .or_else(|| self.parsed.charmap().map('?'));
        let entry = found.map_or((0, 0.5), |gid| {
            let advance = self
                .parsed
                .glyph_metrics(FontSize::unscaled(), LocationRef::default())
                .advance_width(gid)
                .unwrap_or(0.0);
            (gid.to_u32(), advance / self.units_per_em)
        });
        self.cache.insert(c, entry);
        entry
    }

    /// Width of `text` at `size` points.
    fn width(&mut self, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.glyph(c).1).sum::<f32>() * size
    }
}

struct Fonts {
    regular: Face,
    bold: Face,
}

impl Fonts {
    fn new() -> Result<Fonts, PdfError> {
        Ok(Fonts {
            regular: Face::new(REGULAR)?,
            bold: Face::new(BOLD)?,
        })
    }

    fn face(&mut self, bold: bool) -> &mut Face {
        if bold {
            &mut self.bold
        } else {
            &mut self.regular
        }
    }
}

// ---- layout ---------------------------------------------------------------

const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;
const MARGIN: f32 = 48.0;
const FOOTER_H: f32 = 30.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;
const BOTTOM: f32 = PAGE_H - MARGIN - FOOTER_H;
const BODY: f32 = 9.5;
const TABLE: f32 = 7.5;
const CELL_PAD: f32 = 3.0;

#[derive(Clone, Copy)]
enum Ink {
    Black,
    Grey,
    Red,
}

/// How a run of text looks.
#[derive(Clone, Copy)]
struct Look {
    size: f32,
    bold: bool,
    ink: Ink,
}

enum Item {
    Text {
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        ink: Ink,
        text: String,
    },
    /// A filled rectangle (header rows).
    Shade { x: f32, y: f32, w: f32, h: f32 },
    /// A thin rule.
    Rule { x1: f32, x2: f32, y: f32 },
    /// A framed drawing (a figure panel).
    Diagram {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        list: Arc<DisplayList>,
    },
}

struct Pages {
    pages: Vec<Vec<Item>>,
    y: f32,
    truncated: bool,
}

impl Pages {
    fn new() -> Pages {
        Pages {
            pages: vec![Vec::new()],
            y: MARGIN,
            truncated: false,
        }
    }

    fn new_page(&mut self) {
        if self.pages.len() >= MAX_PAGES {
            self.truncated = true;
            return;
        }
        self.pages.push(Vec::new());
        self.y = MARGIN;
    }

    fn room(&self) -> f32 {
        BOTTOM - self.y
    }

    fn push(&mut self, item: Item) {
        if !self.truncated
            && let Some(page) = self.pages.last_mut()
        {
            page.push(item);
        }
    }
}

/// Splits `text` into lines no wider than `width`. Words are kept whole and
/// a word wider than the line is broken at the edge. O(chars).
fn wrap(face: &mut Face, text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0.0;
        let space = face.width(" ", size);
        for word in paragraph.split_whitespace() {
            let w = face.width(word, size);
            if w > width {
                // Flush the line, then cut the word where it no longer fits.
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let mut piece = String::new();
                let mut piece_w = 0.0;
                for c in word.chars() {
                    let cw = face.glyph(c).1 * size;
                    if piece_w + cw > width && !piece.is_empty() {
                        lines.push(std::mem::take(&mut piece));
                        piece_w = 0.0;
                    }
                    piece.push(c);
                    piece_w += cw;
                }
                line = piece;
                used = piece_w;
                continue;
            }
            let extra = if line.is_empty() { w } else { space + w };
            if used + extra > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
                used = w;
            } else {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
                used += extra;
            }
        }
        lines.push(line);
    }
    lines
}

fn leading(size: f32) -> f32 {
    size * 1.4
}

fn text_block(p: &mut Pages, fonts: &mut Fonts, text: &str, look: Look, x: f32, width: f32) {
    let Look { size, bold, ink } = look;
    for line in wrap(fonts.face(bold), text, size, width) {
        if p.room() < leading(size) {
            p.new_page();
        }
        let y = p.y + size;
        p.push(Item::Text {
            x,
            y,
            size,
            bold,
            ink,
            text: line,
        });
        p.y += leading(size);
    }
}

fn heading(p: &mut Pages, fonts: &mut Fonts, level: u8, text: &str) {
    let (size, before, after) = match level {
        1 => (20.0, 0.0, 6.0),
        2 => (14.0, 16.0, 4.0),
        _ => (10.5, 10.0, 2.0),
    };
    // Keep a heading with at least two lines of what follows.
    if p.room() < before + leading(size) + 5.0 * leading(BODY) && p.y > MARGIN {
        p.new_page();
    } else if p.y > MARGIN {
        p.y += before;
    }
    let look = Look {
        size,
        bold: true,
        ink: Ink::Black,
    };
    text_block(p, fonts, text, look, MARGIN, CONTENT_W);
    p.y += after;
}

/// Column widths in points. Every column is at least as wide as its longest
/// header word (so a heading is never cut in the middle) and as its longest
/// short content word; what is left over is shared in proportion to how much
/// text each column holds. If even the minimums do not fit, they are scaled
/// down together. O(cells).
fn column_widths(fonts: &mut Fonts, headers: &[String], rows: &[Vec<String>]) -> Vec<f32> {
    let n = headers.len().max(1);
    let mut min = Vec::with_capacity(n);
    let mut weight = Vec::with_capacity(n);
    for (i, header) in headers.iter().enumerate() {
        let head_word = header
            .split_whitespace()
            .map(|w| fonts.bold.width(w, TABLE))
            .fold(0.0_f32, f32::max);
        let body_word = rows
            .iter()
            .filter_map(|r| r.get(i))
            .flat_map(|c| c.split_whitespace())
            .map(|w| fonts.regular.width(w, TABLE))
            .fold(0.0_f32, f32::max)
            .min(60.0);
        min.push(head_word.max(body_word) + 2.0 * CELL_PAD + 1.0);
        let longest = rows
            .iter()
            .filter_map(|r| r.get(i))
            .map(|c| c.chars().count())
            .max()
            .unwrap_or(1)
            .max(header.chars().count().min(14));
        #[allow(clippy::cast_precision_loss)]
        weight.push(longest.clamp(5, 48) as f32);
    }
    let min_sum: f32 = min.iter().sum();
    if min_sum >= CONTENT_W {
        return min.iter().map(|m| m * CONTENT_W / min_sum).collect();
    }
    let spare = CONTENT_W - min_sum;
    let weight_sum: f32 = weight.iter().sum();
    min.iter()
        .zip(&weight)
        .map(|(m, w)| m + spare * w / weight_sum)
        .collect()
}

fn table_header(p: &mut Pages, fonts: &mut Fonts, headers: &[String], widths: &[f32]) {
    let lines: Vec<Vec<String>> = headers
        .iter()
        .zip(widths)
        .map(|(h, w)| wrap(&mut fonts.bold, h, TABLE, w - 2.0 * CELL_PAD))
        .collect();
    let rows = lines.iter().map(Vec::len).max().unwrap_or(1);
    #[allow(clippy::cast_precision_loss)]
    let height = rows as f32 * leading(TABLE) + 2.0 * CELL_PAD;
    p.push(Item::Shade {
        x: MARGIN,
        y: p.y,
        w: CONTENT_W,
        h: height,
    });
    let mut x = MARGIN;
    for (cell, w) in lines.iter().zip(widths) {
        for (i, line) in cell.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = p.y + CELL_PAD + TABLE + i as f32 * leading(TABLE);
            p.push(Item::Text {
                x: x + CELL_PAD,
                y,
                size: TABLE,
                bold: true,
                ink: Ink::Black,
                text: line.clone(),
            });
        }
        x += w;
    }
    p.y += height;
}

fn table_block(p: &mut Pages, fonts: &mut Fonts, headers: &[String], rows: &[Vec<String>]) {
    if rows.is_empty() {
        let look = Look {
            size: BODY,
            bold: false,
            ink: Ink::Grey,
        };
        text_block(p, fonts, "None.", look, MARGIN, CONTENT_W);
        return;
    }
    let widths = column_widths(fonts, headers, rows);
    // The header and the first line of the first row stay together.
    if p.room() < 2.0 * leading(TABLE) + 4.0 * CELL_PAD + leading(TABLE) {
        p.new_page();
    }
    table_header(p, fonts, headers, &widths);
    for row in rows {
        let mut cells: Vec<std::vec::IntoIter<String>> = widths
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let text = row.get(i).map_or("", String::as_str);
                wrap(&mut fonts.regular, text, TABLE, w - 2.0 * CELL_PAD).into_iter()
            })
            .collect();
        loop {
            // How many lines still wait, and how many fit on this page.
            let waiting = cells.iter().map(|c| c.len()).max().unwrap_or(0);
            if waiting == 0 {
                break;
            }
            let lines_fit = ((p.room() - 2.0 * CELL_PAD) / leading(TABLE)).floor();
            if lines_fit < 1.0 {
                p.new_page();
                if p.truncated {
                    return;
                }
                table_header(p, fonts, headers, &widths);
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let take = waiting.min(lines_fit as usize);
            #[allow(clippy::cast_precision_loss)]
            let height = take as f32 * leading(TABLE) + 2.0 * CELL_PAD;
            let mut x = MARGIN;
            for (cell, w) in cells.iter_mut().zip(&widths) {
                for i in 0..take {
                    let Some(line) = cell.next() else { break };
                    #[allow(clippy::cast_precision_loss)]
                    let y = p.y + CELL_PAD + TABLE + i as f32 * leading(TABLE);
                    p.push(Item::Text {
                        x: x + CELL_PAD,
                        y,
                        size: TABLE,
                        bold: false,
                        ink: Ink::Black,
                        text: line,
                    });
                }
                x += w;
            }
            p.y += height;
            p.push(Item::Rule {
                x1: MARGIN,
                x2: MARGIN + CONTENT_W,
                y: p.y,
            });
        }
    }
    p.y += 6.0;
}

fn preformatted(p: &mut Pages, fonts: &mut Fonts, text: &str) {
    const INDENT: f32 = 4.0;
    for line in text.lines() {
        let spaces = line.chars().take_while(|c| *c == ' ').count();
        #[allow(clippy::cast_precision_loss)]
        let x = MARGIN + 6.0 + spaces as f32 * INDENT;
        let width = (CONTENT_W - (x - MARGIN)).max(60.0);
        let look = Look {
            size: TABLE + 1.0,
            bold: false,
            ink: Ink::Black,
        };
        text_block(p, fonts, line.trim_start(), look, x, width);
    }
    p.y += 6.0;
}

/// Places the panels of a figure: a caption, then the frame. A panel never
/// splits across pages; it moves to the next page when it does not fit.
fn figure_block(p: &mut Pages, fonts: &mut Fonts, figure: &Figure) {
    let caption = Look {
        size: TABLE + 1.0,
        bold: false,
        ink: Ink::Grey,
    };
    for panel in &figure.panels {
        let (w, h) = (
            panel.size.0.min(CONTENT_W),
            panel.size.1.min(BOTTOM - MARGIN - 20.0),
        );
        if p.room() < h + leading(caption.size) + 6.0 && p.y > MARGIN {
            p.new_page();
        }
        text_block(p, fonts, &panel.caption, caption, MARGIN, CONTENT_W);
        p.push(Item::Diagram {
            x: MARGIN,
            y: p.y,
            w,
            h,
            list: Arc::clone(&panel.list),
        });
        p.y += h + 8.0;
    }
    if figure.omitted > 0 {
        let note = format!(
            "{} more detail panels are not drawn. The outline below lists every node.",
            figure.omitted
        );
        text_block(p, fonts, &note, caption, MARGIN, CONTENT_W);
        p.y += 4.0;
    }
}

fn layout(doc: &ReportDoc, fonts: &mut Fonts) -> Pages {
    let mut p = Pages::new();
    for block in &doc.blocks {
        if p.truncated {
            break;
        }
        match block {
            Block::Heading(level, text) => heading(&mut p, fonts, *level, text),
            Block::Paragraph(text, style) => {
                let (bold, ink) = match style {
                    Style::Normal => (false, Ink::Black),
                    Style::Muted => (false, Ink::Grey),
                    Style::Draft => (true, Ink::Red),
                };
                let look = Look {
                    size: BODY,
                    bold,
                    ink,
                };
                text_block(&mut p, fonts, text, look, MARGIN, CONTENT_W);
                p.y += 4.0;
            }
            Block::Table { headers, rows } => table_block(&mut p, fonts, headers, rows),
            Block::Preformatted(text) => preformatted(&mut p, fonts, text),
            Block::Figure(figure) => figure_block(&mut p, fonts, figure),
        }
    }
    if p.truncated {
        p.pages.truncate(MAX_PAGES);
        if let Some(last) = p.pages.last_mut() {
            last.push(Item::Text {
                x: MARGIN,
                y: BOTTOM,
                size: BODY,
                bold: true,
                ink: Ink::Red,
                text: "The report was cut off here: it is too long for one PDF.".to_owned(),
            });
        }
    }
    p
}

// ---- writing the file -----------------------------------------------------

/// Why a PDF could not be made.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// The embedded font could not be read. A build fault, not a user error.
    #[error("the report font could not be read")]
    Font,
    /// The PDF library refused the document.
    #[error("the PDF could not be written: {0}")]
    Write(String),
}

fn ink(i: Ink) -> rgb::Color {
    match i {
        Ink::Black => rgb::Color::new(20, 24, 31),
        Ink::Grey => rgb::Color::new(95, 105, 117),
        Ink::Red => rgb::Color::new(170, 40, 30),
    }
}

fn fill(color: rgb::Color) -> Fill {
    Fill {
        paint: color.into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::default(),
    }
}

fn draw_text(
    surface: &mut krilla::surface::Surface<'_>,
    face: &mut Face,
    x: f32,
    y: f32,
    size: f32,
    color: rgb::Color,
    text: &str,
) {
    let glyphs: Vec<KrillaGlyph> = text
        .char_indices()
        .map(|(at, c)| {
            let (gid, advance) = face.glyph(c);
            KrillaGlyph::new(
                GlyphId::new(gid),
                advance,
                0.0,
                0.0,
                0.0,
                at..at + c.len_utf8(),
                None,
            )
        })
        .collect();
    surface.set_fill(Some(fill(color)));
    surface.draw_glyphs(
        Point::from_xy(x, y),
        &glyphs,
        face.font.clone(),
        text,
        size,
        false,
    );
}

fn rectangle(
    surface: &mut krilla::surface::Surface<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    c: rgb::Color,
) {
    let Some(rect) = Rect::from_xywh(x, y, w, h) else {
        return;
    };
    let mut b = PathBuilder::new();
    b.push_rect(rect);
    if let Some(path) = b.finish() {
        surface.set_fill(Some(fill(c)));
        surface.set_stroke(None);
        surface.draw_path(&path);
    }
}

/// A canvas color token as a PDF color (the light, print palette).
fn paint_color(paint: Paint) -> rgb::Color {
    let hex = SvgTheme::Light.hex(paint).trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    rgb::Color::new(byte(0), byte(2), byte(4))
}

fn pdf_stroke(s: aegis_canvas::Stroke) -> PdfStroke {
    PdfStroke {
        paint: paint_color(s.paint).into(),
        width: s.width.max(0.3),
        dash: s.dashed.then(|| StrokeDash {
            array: vec![4.0, 3.0],
            offset: 0.0,
        }),
        ..PdfStroke::default()
    }
}

/// A rectangle with rounded corners as a path.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<krilla::geom::Path> {
    const KAPPA: f32 = 0.552_284_7;
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut b = PathBuilder::new();
    if r < 0.25 {
        b.push_rect(Rect::from_xywh(x, y, w, h)?);
        return b.finish();
    }
    let k = r * KAPPA;
    let (right, bottom) = (x + w, y + h);
    b.move_to(x + r, y);
    b.line_to(right - r, y);
    b.cubic_to(right - r + k, y, right, y + r - k, right, y + r);
    b.line_to(right, bottom - r);
    b.cubic_to(
        right,
        bottom - r + k,
        right - r + k,
        bottom,
        right - r,
        bottom,
    );
    b.line_to(x + r, bottom);
    b.cubic_to(x + r - k, bottom, x, bottom - r + k, x, bottom - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish()
}

/// Draws one panel: a frame, then the display list clipped to it.
#[allow(clippy::many_single_char_names)]
fn draw_diagram(
    surface: &mut krilla::surface::Surface<'_>,
    fonts: &mut Fonts,
    (x, y, w, h): (f32, f32, f32, f32),
    list: &DisplayList,
) {
    let Some(frame) = Rect::from_xywh(x, y, w, h) else {
        return;
    };
    let mut b = PathBuilder::new();
    b.push_rect(frame);
    let Some(frame_path) = b.finish() else { return };
    surface.set_fill(Some(fill(paint_color(Paint::Panel))));
    surface.set_stroke(Some(pdf_stroke(aegis_canvas::Stroke::solid(
        Paint::Rule,
        0.5,
    ))));
    surface.draw_path(&frame_path);
    surface.set_stroke(None);

    surface.push_clip_path(&frame_path, &FillRule::default());
    for item in &list.items {
        match item {
            CanvasItem::Rect {
                rect,
                fill: f,
                stroke,
                radius,
                ..
            } => {
                let Some(path) = rounded_rect(
                    x + rect.origin.x,
                    y + rect.origin.y,
                    rect.size.w,
                    rect.size.h,
                    *radius,
                ) else {
                    continue;
                };
                surface.set_fill(f.map(|p| fill(paint_color(p))));
                surface.set_stroke(stroke.map(pdf_stroke));
                surface.draw_path(&path);
            }
            CanvasItem::Curve {
                from,
                c1,
                c2,
                to,
                stroke,
            } => {
                let mut b = PathBuilder::new();
                b.move_to(x + from.x, y + from.y);
                b.cubic_to(x + c1.x, y + c1.y, x + c2.x, y + c2.y, x + to.x, y + to.y);
                if let Some(path) = b.finish() {
                    surface.set_fill(None);
                    surface.set_stroke(Some(pdf_stroke(*stroke)));
                    surface.draw_path(&path);
                }
            }
            CanvasItem::Polyline { points, stroke } => {
                let mut b = PathBuilder::new();
                for (i, pt) in points.iter().enumerate() {
                    if i == 0 {
                        b.move_to(x + pt.x, y + pt.y);
                    } else {
                        b.line_to(x + pt.x, y + pt.y);
                    }
                }
                if let Some(path) = b.finish() {
                    surface.set_fill(None);
                    surface.set_stroke(Some(pdf_stroke(*stroke)));
                    surface.draw_path(&path);
                }
            }
            CanvasItem::Text {
                at,
                text,
                size,
                bold,
                paint,
                align,
            } => {
                surface.set_stroke(None);
                let face = fonts.face(*bold);
                let left = match align {
                    Align::Start => at.x,
                    Align::End => at.x - face.width(text, *size),
                };
                draw_text(
                    surface,
                    face,
                    x + left,
                    y + at.y,
                    *size,
                    paint_color(*paint),
                    text,
                );
            }
        }
    }
    surface.pop();
    surface.set_stroke(None);
}

fn metadata(doc: &ReportDoc, ctx: &ReportContext) -> Metadata {
    let secs = ctx.now_ms / 1000;
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (y, mo, d) = crate::time::civil_from_days(days);
    let in_day = secs % 86_400;
    let date = DateTime::new(u16::try_from(y).unwrap_or(1970))
        .month(u8::try_from(mo).unwrap_or(1))
        .day(u8::try_from(d).unwrap_or(1))
        .hour(u8::try_from(in_day / 3600).unwrap_or(0))
        .minute(u8::try_from(in_day % 3600 / 60).unwrap_or(0))
        .second(u8::try_from(in_day % 60).unwrap_or(0));
    Metadata::new()
        .title(doc.title.clone())
        .creator("AegisTARA".to_owned())
        .producer("AegisTARA".to_owned())
        .authors(vec![ctx.user.clone()])
        .language("en".to_owned())
        .creation_date(date)
}

fn render(
    doc: &ReportDoc,
    ctx: &ReportContext,
    pages: &Pages,
    fonts: &mut Fonts,
) -> Result<Vec<u8>, PdfError> {
    let mut document = Document::new();
    document.set_metadata(metadata(doc, ctx));
    let total = pages.pages.len();
    for (n, items) in pages.pages.iter().enumerate() {
        let size = Size::from_wh(PAGE_W, PAGE_H).ok_or(PdfError::Font)?;
        let mut page = document.start_page_with(PageSettings::new(size));
        let mut surface = page.surface();
        for item in items {
            match item {
                Item::Shade { x, y, w, h } => {
                    rectangle(&mut surface, *x, *y, *w, *h, rgb::Color::new(233, 237, 240));
                }
                Item::Rule { x1, x2, y } => {
                    rectangle(
                        &mut surface,
                        *x1,
                        *y,
                        x2 - x1,
                        0.4,
                        rgb::Color::new(195, 203, 211),
                    );
                }
                Item::Diagram { x, y, w, h, list } => {
                    draw_diagram(&mut surface, fonts, (*x, *y, *w, *h), list);
                }
                Item::Text {
                    x,
                    y,
                    size,
                    bold,
                    ink: i,
                    text,
                } => {
                    draw_text(
                        &mut surface,
                        fonts.face(*bold),
                        *x,
                        *y,
                        *size,
                        ink(*i),
                        text,
                    );
                }
            }
        }
        // Footer: the watermark on the left, "Page n of N" on the right.
        let foot_y = PAGE_H - MARGIN + 6.0;
        rectangle(
            &mut surface,
            MARGIN,
            foot_y - 12.0,
            CONTENT_W,
            0.4,
            rgb::Color::new(195, 203, 211),
        );
        draw_text(
            &mut surface,
            &mut fonts.regular,
            MARGIN,
            foot_y,
            7.5,
            ink(Ink::Grey),
            &doc.watermark,
        );
        let label = format!("Page {} of {total}", n + 1);
        let w = fonts.regular.width(&label, 7.5);
        draw_text(
            &mut surface,
            &mut fonts.regular,
            PAGE_W - MARGIN - w,
            foot_y,
            7.5,
            ink(Ink::Grey),
            &label,
        );
        surface.finish();
        page.finish();
    }
    document
        .finish()
        .map_err(|e| PdfError::Write(format!("{e:?}")))
}

/// The report as PDF bytes. Time and memory O(project size).
///
/// # Errors
///
/// [`PdfError`] if the font or the PDF library fails (neither depends on the
/// project).
pub fn report_pdf(ws: &Workspace, ctx: &ReportContext) -> Result<Vec<u8>, PdfError> {
    let doc = report_doc(ws, ctx);
    let mut fonts = Fonts::new()?;
    let pages = layout(&doc, &mut fonts);
    render(&doc, ctx, &pages, &mut fonts)
}
