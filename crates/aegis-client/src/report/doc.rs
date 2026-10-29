//! The one report document model (RPT-01, ADR 0056): [`ReportDoc`] and its
//! [`Block`]s are built once by [`report_doc`] and rendered by the HTML, PDF
//! and XLSX writers in this module, so the three cannot drift apart.
//!
//! The block builder is the superset of the old `report_html` (job 041) and
//! the old `pdf::report_doc` (job 050); a test keeps the section list of the
//! HTML and PDF outputs equal. A [`ReportOptions`](super::ReportOptions)
//! selects sections, filters (risk level, asset, baseline), language and
//! branding (RPT-02 to RPT-04).

use std::collections::BTreeSet;
use std::sync::Arc;

use aegis_canvas::{
    RenderOptions, Size as CanvasSize, TileLimits, plan_tiles, render as render_view,
};
use aegis_model::{ElementRef, Project, ThreatId, TreeId};

use crate::commands::TREATMENTS;
use crate::i18n::{Catalog, Language};
use crate::outline::to_outline;
use crate::pages::{self, RiskRow, treatment_label};
use crate::time::format_ms_lang;
use crate::view::tree_scene;
use crate::workspace::Workspace;

use super::ReportContext;
use super::profile::{Classification, FigureAttachment, ReportFilters, ReportOptions};

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
    /// A logo image (PNG, JPEG or sanitized SVG). The HTML renderer embeds
    /// it; the PDF renderer prints a caption with the same alternative text.
    Image {
        /// MIME type, for example `image/png`.
        media_type: String,
        /// The bytes, capped by the profile.
        bytes: Vec<u8>,
        /// Alternative text.
        alt: String,
    },
}

/// A drawing made of panels, each a canvas display list on its own frame.
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
    pub list: Arc<aegis_canvas::DisplayList>,
}

/// Panel area on a page: the full text width and this height in points.
pub(crate) const FIGURE_H: f32 = 540.0;

/// The report before it is laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportDoc {
    /// Document title for the PDF properties.
    pub title: String,
    /// Printed at the foot of every page (the classification mark and the
    /// exporter, RPT-04).
    pub watermark: String,
    /// The report language, as a BCP 47 tag for the PDF metadata.
    pub language: String,
    /// The content.
    pub blocks: Vec<Block>,
}

// ---- content helpers ------------------------------------------------------

/// The panels for one attack tree (the inherent view, as the HTML report
/// shows it). `None` if the tree is gone or has no nodes.
fn tree_figure(ws: &Workspace, tree: TreeId, catalog: &Catalog) -> Option<Figure> {
    let scene = tree_scene(ws, tree, false)?;
    if scene.is_empty() {
        return None;
    }
    let area = CanvasSize::new(super::pdf::CONTENT_W, FIGURE_H);
    let plan = plan_tiles(&scene, area, &TileLimits::default());
    let options = RenderOptions::default();
    let panel = |caption: String, view: &aegis_canvas::Viewport| Panel {
        caption,
        size: (view.size.w, view.size.h),
        list: Arc::new(render_view(&scene, view, &options)),
    };
    let mut panels = Vec::new();
    let caption = if plan.needs_detail {
        catalog.tr("tree-overview-caption")
    } else {
        catalog.tr("tree-caption")
    };
    panels.push(panel(caption, &plan.overview));
    for tile in &plan.detail {
        panels.push(panel(
            catalog.tr_with(
                "tree-detail-caption",
                &[
                    ("row", &(tile.row + 1).to_string()),
                    ("rows", &plan.rows.to_string()),
                    ("col", &(tile.col + 1).to_string()),
                    ("cols", &plan.cols.to_string()),
                ],
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
fn desc_list(b: &mut Vec<Block>, items: &[(String, String)], catalog: &Catalog) {
    if items.is_empty() {
        return;
    }
    b.push(Block::Heading(3, catalog.tr("descriptions")));
    for (name, text) in items {
        b.push(Block::Paragraph(name.clone(), Style::Muted));
        desc_blocks(b, text);
    }
}

fn table(headers: &[String], rows: Vec<Vec<String>>) -> Block {
    Block::Table {
        headers: headers.to_vec(),
        rows,
    }
}

fn risk_cell_text(catalog: &Catalog, r: Option<&crate::view::RiskView>) -> String {
    r.map_or_else(|| catalog.tr("not-rated"), |v| v.label.clone())
}

fn risk_rows_block(catalog: &Catalog, rows: &[RiskRow]) -> Block {
    table(
        &[
            catalog.tr("threat-scenario"),
            catalog.tr("asset"),
            catalog.tr("deciding-damage"),
            catalog.tr("impact"),
            catalog.tr("feasibility"),
            catalog.tr("risk-before"),
            catalog.tr("risk-after"),
            catalog.tr("treatment"),
            catalog.tr("goal-or-claim"),
        ],
        rows.iter()
            .map(|r| {
                vec![
                    r.threat.clone(),
                    r.asset.clone(),
                    r.damage.clone(),
                    r.impact.clone(),
                    r.feasibility.clone(),
                    risk_cell_text(catalog, r.inherent.as_ref()),
                    risk_cell_text(catalog, r.residual.as_ref()),
                    r.treatment.clone(),
                    r.backing.clone(),
                ]
            })
            .collect(),
    )
}

/// `"not rated"` (localized by the caller where needed).
#[must_use]
pub fn risk_text(r: Option<&crate::view::RiskView>) -> String {
    r.map_or_else(|| "not rated".to_owned(), |v| v.label.clone())
}

/// A report section whose elements may carry descriptions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Described {
    Item,
    Assets,
    Damage,
    Controls,
    Goals,
    Claims,
}

/// `(name, text)` of the elements of a section that have a description, in
/// project order. Empty when none has. Time O(n log n).
pub(crate) fn section_descriptions(ws: &Workspace, section: Described) -> Vec<(String, String)> {
    let p = ws.project();
    let mut out = Vec::new();
    let mut add = |name: &str, target: ElementRef| {
        if let Some(text) = p.description(target) {
            out.push((name.to_owned(), text.to_owned()));
        }
    };
    match section {
        Described::Item => {
            p.components()
                .for_each(|(_, id, x)| add(x.name(), ElementRef::Component(id)));
            p.interfaces()
                .for_each(|(_, id, x)| add(x.name(), ElementRef::Interface(id)));
            p.data_flows()
                .for_each(|(_, id, x)| add(x.name(), ElementRef::DataFlow(id)));
            p.trust_boundaries()
                .for_each(|(_, id, x)| add(x.name(), ElementRef::TrustBoundary(id)));
        }
        Described::Assets => p
            .assets()
            .for_each(|(_, id, x)| add(x.name(), ElementRef::Asset(id))),
        Described::Damage => p
            .damage_scenarios()
            .for_each(|(_, id, x)| add(x.name(), ElementRef::Damage(id))),
        Described::Controls => p
            .security_controls()
            .for_each(|(_, id, x)| add(x.name(), ElementRef::Control(id))),
        Described::Goals => p
            .cybersecurity_goals()
            .for_each(|(_, id, x)| add(x.name(), ElementRef::Goal(id))),
        Described::Claims => p
            .cybersecurity_claims()
            .for_each(|(_, id, x)| add(x.name(), ElementRef::Claim(id))),
    }
    out
}

/// The description text of one element, if it has any.
pub(crate) fn description_of(ws: &Workspace, target: ElementRef) -> Option<String> {
    ws.project().description(target).map(str::to_owned)
}

pub(crate) fn section_8_heading_lang(
    names: &aegis_model::ElementNamesDef,
    lang: Language,
) -> String {
    if names.cybersecurity_goal.plural.as_ref() == "Cybersecurity goals"
        && names.cybersecurity_claim.plural.as_ref() == "Cybersecurity claims"
    {
        "8. Cybersecurity goals and claims".to_owned()
    } else if lang == Language::Ja {
        format!(
            "8. {}と{}",
            names.cybersecurity_goal.plural, names.cybersecurity_claim.plural
        )
    } else {
        format!(
            "8. {} and {}",
            names.cybersecurity_goal.plural,
            names.cybersecurity_claim.plural.to_lowercase()
        )
    }
}

pub(crate) fn methodology_line_lang(project: &Project, lang: &str) -> String {
    let m = project.methodology();
    format!(
        "{} (pack {} {})",
        m.label_for_lang(lang),
        m.key(),
        m.version()
    )
}

fn push_target_figures(b: &mut Vec<Block>, figures: &[FigureAttachment], target: ElementRef) {
    for f in figures {
        if f.data.is_empty() {
            continue;
        }
        let Some(t) = crate::view::target_to_element_ref(&f.target) else {
            continue;
        };
        if t == target {
            b.push(Block::Image {
                media_type: f.media_type.clone(),
                bytes: f.data.clone(),
                alt: if f.caption.is_empty() {
                    f.figure_id.clone()
                } else {
                    f.caption.clone()
                },
            });
        }
    }
}

/// The per-page footer: the classification mark (if any) and the exporter.
#[must_use]
pub fn report_watermark(
    ctx: &ReportContext,
    classification: Classification,
    catalog: &Catalog,
    lang: Language,
) -> String {
    let when = format_ms_lang(ctx.now_ms, lang);
    if classification == Classification::None {
        format!("Exported by {} on {when}.", ctx.user)
    } else {
        format!(
            "{}. Exported by {} on {when}.",
            classification.label(catalog),
            ctx.user
        )
    }
}

// ---- filtering ------------------------------------------------------------

fn component_assets(ws: &Workspace, filters: &ReportFilters) -> Option<BTreeSet<String>> {
    let comp = filters.component.as_deref()?.trim();
    if comp.is_empty() {
        return None;
    }
    Some(
        pages::asset_rows(ws)
            .into_iter()
            .filter(|a| a.component == comp)
            .map(|a| a.name)
            .collect(),
    )
}

fn row_matches(
    r: &RiskRow,
    filters: &ReportFilters,
    comp_assets: Option<&BTreeSet<String>>,
) -> bool {
    if let Some(allowed) = comp_assets
        && !allowed.contains(&r.asset)
    {
        return false;
    }
    if !filters.assets.is_empty() && !filters.assets.iter().any(|a| a == &r.asset) {
        return false;
    }
    if !filters.risk_levels.is_empty() {
        let label = r
            .residual
            .as_ref()
            .map_or("not rated", |v| v.label.as_str());
        if !filters.risk_levels.iter().any(|l| l == label) {
            return false;
        }
    }
    true
}

/// The threats kept by the filters, or empty when no filter is active (the
/// empty set also means "everything", so callers check [`has_filter`]).
fn matching_threats(
    ws: &Workspace,
    filters: &ReportFilters,
    comp_assets: Option<&BTreeSet<String>>,
) -> BTreeSet<ThreatId> {
    pages::risk_rows(ws)
        .iter()
        .filter(|r| row_matches(r, filters, comp_assets))
        .map(|r| r.id)
        .collect()
}

fn has_filter(filters: &ReportFilters) -> bool {
    !filters.risk_levels.is_empty()
        || !filters.assets.is_empty()
        || filters
            .component
            .as_ref()
            .is_some_and(|c| !c.trim().is_empty())
}

// ---- the builder ----------------------------------------------------------

/// The report as blocks for one language. Sections and filters come from the
/// profile; the numbers of the sections stay fixed (1 to 9) so a disabled
/// section leaves a gap instead of renumbering the ones after it.
#[allow(clippy::too_many_lines)]
fn build(ws: &Workspace, ctx: &ReportContext, opts: &ReportOptions, lang: Language) -> Vec<Block> {
    use Block::{Heading, Paragraph};
    let catalog = Catalog::load(lang);
    let profile = &opts.profile;
    let sections = profile.sections;
    let project = ws.project();
    let m = project.methodology();
    let names = m.element_names_for_lang(lang.code());
    let vocab = pages::vocabulary_for_lang(m, lang.code());
    let meth_line = methodology_line_lang(project, lang.code());
    let mut b: Vec<Block> = Vec::new();

    // ---- cover (RPT-04) ----
    if sections.cover {
        b.push(Heading(
            1,
            catalog.tr_with("report-title", &[("project", &ctx.project)]),
        ));
        b.push(Paragraph(
            catalog.tr_with(
                "report-subtitle",
                &[
                    ("methodology", &meth_line),
                    ("seq", &ctx.seq.to_string()),
                    ("engine", aegis_model::ENGINE_VERSION),
                ],
            ),
            Style::Muted,
        ));
        if ctx.unsaved {
            b.push(Paragraph(catalog.tr("draft-warning"), Style::Draft));
        }
        let branding = &profile.branding;
        b.push(table(
            &[catalog.tr("field"), catalog.tr("value")],
            vec![
                vec![
                    catalog.tr("document-id"),
                    if branding.document_id.is_empty() {
                        "—".to_owned()
                    } else {
                        branding.document_id.clone()
                    },
                ],
                vec![
                    catalog.tr("classification"),
                    branding.classification.label(&catalog),
                ],
                vec![catalog.tr("date"), format_ms_lang(ctx.now_ms, lang)],
                vec![catalog.tr("methodology"), meth_line],
            ],
        ));
        if !branding.approvers.is_empty() {
            b.push(Heading(3, catalog.tr("approvers")));
            b.push(Paragraph(branding.approvers.join(", "), Style::Normal));
        }
        if !branding.revisions.is_empty() {
            b.push(Heading(3, catalog.tr("revision")));
            b.push(table(
                &[
                    catalog.tr("revision"),
                    catalog.tr("date"),
                    catalog.tr("description"),
                ],
                branding
                    .revisions
                    .iter()
                    .map(|r| vec![r.revision.clone(), r.date.clone(), r.description.clone()])
                    .collect(),
            ));
        }
        if let Some(logo) = &branding.logo {
            b.push(Block::Image {
                media_type: logo_media_type(logo),
                bytes: logo.clone(),
                alt: ctx.project.clone(),
            });
        }
    }

    if sections.notes
        && let Some(notes) = description_of(ws, ElementRef::Project)
    {
        b.push(Heading(2, catalog.tr("project-notes")));
        desc_blocks(&mut b, &notes);
    }

    if sections.figures && !profile.figures.is_empty() {
        b.push(Heading(2, catalog.tr("section-figures")));
        b.push(table(
            &[
                catalog.tr("caption"),
                catalog.tr("target"),
                catalog.tr("format"),
                catalog.tr("dimensions"),
            ],
            profile
                .figures
                .iter()
                .map(|f| {
                    vec![
                        if f.caption.is_empty() {
                            f.figure_id.clone()
                        } else {
                            f.caption.clone()
                        },
                        if f.target.is_empty() {
                            "project".to_owned()
                        } else {
                            f.target.clone()
                        },
                        f.media_type.clone(),
                        format!("{}x{}", f.width_px, f.height_px),
                    ]
                })
                .collect(),
        ));
        push_target_figures(&mut b, &profile.figures, ElementRef::Project);
    }

    let comp_assets = component_assets(ws, &profile.filters);
    let filtered = matching_threats(ws, &profile.filters, comp_assets.as_ref());
    let filter_on = has_filter(&profile.filters);

    // ---- executive summary ----
    if sections.executive_summary {
        let ov = pages::overview(ws);
        b.push(Heading(2, catalog.tr("section-executive-summary")));
        let above: usize = ov.residual.iter().take(2).map(|c| c.count).sum();
        let threats = project.threat_scenario_count().to_string();
        let threats_label = names.threat_scenario.plural.to_lowercase();
        let assets = project.asset_count().to_string();
        let assets_label = names.asset.plural.to_lowercase();
        let above = above.to_string();
        let threat_singular = names.threat_scenario.singular.to_lowercase();
        let errors = ov.errors.to_string();
        let warnings = ov.warnings.to_string();
        b.push(Paragraph(
            catalog.tr_with(
                "summary-counts",
                &[
                    ("threats", threats.as_str()),
                    ("threats_label", threats_label.as_str()),
                    ("assets", assets.as_str()),
                    ("assets_label", assets_label.as_str()),
                    ("above", above.as_str()),
                    ("threat_singular", threat_singular.as_str()),
                    ("errors", errors.as_str()),
                    ("warnings", warnings.as_str()),
                ],
            ),
            Style::Normal,
        ));
        b.push(Heading(3, catalog.tr("risks-by-level")));
        b.push(table(
            &[
                catalog.tr("risk-level"),
                catalog.tr("before-controls"),
                catalog.tr("after-controls"),
            ],
            ov.inherent
                .iter()
                .zip(&ov.residual)
                .map(|(a, c)| vec![a.label.clone(), a.count.to_string(), c.count.to_string()])
                .collect(),
        ));
        b.push(Heading(3, catalog.tr("highest-risks")));
        let top: Vec<RiskRow> = ov
            .top
            .into_iter()
            .filter(|r| !filter_on || row_matches(r, &profile.filters, comp_assets.as_ref()))
            .collect();
        b.push(risk_rows_block(&catalog, &top));
    }

    // ---- item definition (ARC-02, job 074) ----
    if sections.item_definition
        && (project.component_count() > 0
            || project.interface_count() > 0
            || project.data_flow_count() > 0
            || project.trust_boundary_count() > 0)
    {
        b.push(Heading(2, catalog.tr("section-item-definition")));
        b.push(Heading(3, names.component.plural.to_string()));
        let comp_rows = pages::component_rows(ws);
        b.push(table(
            &[
                names.component.singular.to_string(),
                catalog.tr("parent"),
                names.interface.plural.to_string(),
                names.asset.plural.to_string(),
            ],
            comp_rows
                .iter()
                .map(|c| {
                    vec![
                        c.name.clone(),
                        c.parent.clone(),
                        c.interfaces.to_string(),
                        c.assets.to_string(),
                    ]
                })
                .collect(),
        ));
        for c in &comp_rows {
            push_target_figures(&mut b, &profile.figures, ElementRef::Component(c.id));
        }

        b.push(Heading(3, names.interface.plural.to_string()));
        let iface_rows = pages::interface_rows(ws);
        b.push(table(
            &[
                names.interface.singular.to_string(),
                names.component.singular.to_string(),
                catalog.tr("kind"),
                names.data_flow.plural.to_string(),
            ],
            iface_rows
                .iter()
                .map(|i| {
                    vec![
                        i.name.clone(),
                        i.component.clone(),
                        i.kind.clone(),
                        i.flows.to_string(),
                    ]
                })
                .collect(),
        ));
        for i in &iface_rows {
            push_target_figures(&mut b, &profile.figures, ElementRef::Interface(i.id));
        }

        b.push(Heading(3, names.data_flow.plural.to_string()));
        let df_rows = pages::data_flow_rows(ws);
        b.push(table(
            &[
                names.data_flow.singular.to_string(),
                catalog.tr("from-interface"),
                catalog.tr("to-interface"),
                names.interface.singular.to_string(),
            ],
            df_rows
                .iter()
                .map(|df| {
                    vec![
                        df.name.clone(),
                        df.from.clone(),
                        df.to.clone(),
                        df.interface.clone(),
                    ]
                })
                .collect(),
        ));
        for df in &df_rows {
            push_target_figures(&mut b, &profile.figures, ElementRef::DataFlow(df.id));
        }

        b.push(Heading(3, names.trust_boundary.plural.to_string()));
        let tb_rows = pages::trust_boundary_rows(ws);
        b.push(table(
            &[
                names.trust_boundary.singular.to_string(),
                catalog.tr("members"),
            ],
            tb_rows
                .iter()
                .map(|tb| vec![tb.name.clone(), tb.members.to_string()])
                .collect(),
        ));
        for tb in &tb_rows {
            push_target_figures(&mut b, &profile.figures, ElementRef::TrustBoundary(tb.id));
        }
        desc_list(&mut b, &section_descriptions(ws, Described::Item), &catalog);
    }

    // ---- assets ----
    if sections.assets {
        b.push(Heading(2, format!("2. {}", names.asset.plural)));
        let mut headers: Vec<String> = vec![names.asset.singular.to_string()];
        headers.extend(vocab.properties.iter().cloned());
        headers.push(catalog.tr("threats"));
        let asset_rows: Vec<_> = pages::asset_rows(ws)
            .into_iter()
            .filter(|a| {
                let comp_ok = comp_assets
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(&a.name));
                let asset_ok = profile.filters.assets.is_empty()
                    || profile.filters.assets.iter().any(|x| x == &a.name);
                comp_ok && asset_ok
            })
            .collect();
        b.push(table(
            &headers,
            asset_rows
                .iter()
                .map(|a| {
                    let mut row = vec![a.name.clone()];
                    row.extend(
                        a.properties
                            .iter()
                            .map(|p| if *p { catalog.tr("yes") } else { String::new() }),
                    );
                    row.push(a.threats.to_string());
                    row
                })
                .collect(),
        ));
        for a in &asset_rows {
            push_target_figures(&mut b, &profile.figures, ElementRef::Asset(a.id));
        }
        desc_list(
            &mut b,
            &section_descriptions(ws, Described::Assets),
            &catalog,
        );
    }

    // ---- damage ----
    if sections.damage {
        b.push(Heading(2, format!("3. {}", names.damage_scenario.plural)));
        let mut headers: Vec<String> = vec![names.damage_scenario.singular.to_string()];
        headers.extend(vocab.impact_categories.iter().map(|c| c.label.clone()));
        headers.push(catalog.tr("threats"));
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
            &section_descriptions(ws, Described::Damage),
            &catalog,
        );
    }

    // ---- threats ----
    if sections.threats {
        b.push(Heading(2, format!("4. {}", names.threat_scenario.plural)));
        for (_, id, _) in project.threat_scenarios() {
            if filter_on && !filtered.contains(&id) {
                continue;
            }
            let Some(t) = pages::threat_detail(ws, id) else {
                continue;
            };
            b.push(Heading(3, t.name.clone()));
            if let Some(text) = description_of(ws, ElementRef::Threat(id)) {
                desc_blocks(&mut b, &text);
            }
            let damages: Vec<&str> = t
                .damages
                .iter()
                .filter(|d| d.linked)
                .map(|d| d.name.as_str())
                .collect();
            b.push(table(
                &[catalog.tr("field"), catalog.tr("value")],
                vec![
                    vec![names.asset.singular.to_string(), t.asset_name.clone()],
                    vec![
                        catalog.tr("category"),
                        vocab
                            .threat_categories
                            .get(t.category)
                            .cloned()
                            .unwrap_or_default(),
                    ],
                    vec![catalog.tr("catalog-references"), t.catalog_refs.join(", ")],
                    vec![names.damage_scenario.plural.to_string(), damages.join(", ")],
                    vec![
                        names.feasibility.to_string(),
                        t.feasibility
                            .clone()
                            .unwrap_or_else(|| catalog.tr("not-rated")),
                    ],
                    vec![
                        catalog.tr("risk-before-controls"),
                        risk_text(t.inherent.as_ref()),
                    ],
                    vec![
                        catalog.tr("risk-after-controls"),
                        risk_text(t.residual.as_ref()),
                    ],
                    vec![
                        catalog.tr("treatment"),
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
            push_target_figures(&mut b, &profile.figures, ElementRef::Threat(id));
        }
    }

    // ---- attack trees ----
    if sections.attack_trees {
        b.push(Heading(2, format!("5. {}", names.attack_tree.plural)));
        let trees = pages::tree_rows(ws);
        if trees.is_empty() {
            b.push(Paragraph(catalog.tr("none"), Style::Muted));
        }
        for row in trees {
            b.push(Heading(3, row.title.clone()));
            if let Some(text) = description_of(ws, ElementRef::Tree(row.id)) {
                desc_blocks(&mut b, &text);
            }
            let nodes = row.nodes.to_string();
            let threats_name = if row.threats.is_empty() {
                catalog.tr("none-lowercase")
            } else {
                row.threats.clone()
            };
            b.push(Paragraph(
                catalog.tr_with(
                    "tree-summary",
                    &[
                        ("nodes", nodes.as_str()),
                        ("feasibility", row.feasibility.as_str()),
                        ("threats_plural", names.threat_scenario.plural.as_ref()),
                        ("threats", threats_name.as_str()),
                    ],
                ),
                Style::Muted,
            ));
            if let Some(figure) = tree_figure(ws, row.id, &catalog) {
                b.push(Block::Figure(figure));
            }
            if let Some(tree) = project
                .attack_tree_idx(&row.id)
                .and_then(|i| project.attack_tree(i))
            {
                b.push(Block::Preformatted(to_outline(tree)));
            }
            push_target_figures(&mut b, &profile.figures, ElementRef::Tree(row.id));
        }
    }

    // ---- risks ----
    if sections.risks {
        b.push(Heading(2, catalog.tr("section-risks")));
        let rows: Vec<RiskRow> = pages::risk_rows(ws)
            .into_iter()
            .filter(|r| !filter_on || row_matches(r, &profile.filters, comp_assets.as_ref()))
            .collect();
        b.push(risk_rows_block(&catalog, &rows));
    }

    // ---- controls ----
    if sections.controls {
        b.push(Heading(2, format!("7. {}", names.security_control.plural)));
        b.push(table(
            &[
                catalog.tr("control"),
                catalog.tr("attack-steps"),
                catalog.tr("goals"),
            ],
            pages::control_rows(ws)
                .into_iter()
                .map(|c| vec![c.name, c.steps.to_string(), c.goals.to_string()])
                .collect(),
        ));
        desc_list(
            &mut b,
            &section_descriptions(ws, Described::Controls),
            &catalog,
        );
    }

    // ---- goals and claims ----
    if sections.goals_claims {
        b.push(Heading(2, section_8_heading_lang(names, lang)));
        b.push(Heading(3, catalog.tr("goals")));
        b.push(table(
            &[
                names.cybersecurity_goal.singular.to_string(),
                catalog.tr("control"),
                names.threat_scenario.plural.to_string(),
            ],
            pages::goal_rows(ws)
                .into_iter()
                .map(|g| vec![g.name, g.controls.to_string(), g.threats.to_string()])
                .collect(),
        ));
        desc_list(
            &mut b,
            &section_descriptions(ws, Described::Goals),
            &catalog,
        );
        b.push(Heading(3, catalog.tr("claims")));
        b.push(table(
            &[
                names.cybersecurity_claim.singular.to_string(),
                names.threat_scenario.plural.to_string(),
            ],
            pages::claim_rows(ws)
                .into_iter()
                .map(|c| vec![c.name, c.threats.to_string()])
                .collect(),
        ));
        desc_list(
            &mut b,
            &section_descriptions(ws, Described::Claims),
            &catalog,
        );
    }

    // ---- problems ----
    if sections.problems {
        b.push(Heading(2, catalog.tr("section-problems")));
        b.push(table(
            &[catalog.tr("severity"), catalog.tr("problem")],
            pages::problems(ws)
                .into_iter()
                .map(|p| {
                    vec![
                        if p.error {
                            catalog.tr("blocking")
                        } else {
                            catalog.tr("warning")
                        },
                        p.text,
                    ]
                })
                .collect(),
        ));
    }

    // ---- comparison with a baseline (RPT-02 filter) ----
    if let Some(baseline) = &opts.baseline_project
        && profile.filters.baseline.is_some()
    {
        b.push(Heading(2, catalog.tr("section-comparison")));
        if let Ok(cmp) = pages::comparison(
            baseline,
            profile.filters.baseline.as_deref().unwrap_or("baseline"),
            project,
        ) {
            b.push(Paragraph(cmp.summary, Style::Normal));
            b.push(table(
                &[catalog.tr("field"), catalog.tr("value")],
                cmp.lines
                    .into_iter()
                    .map(|l| vec![l.kind, l.text])
                    .collect(),
            ));
        }
    }

    b
}

fn logo_media_type(bytes: &[u8]) -> String {
    if bytes.starts_with(b"\x89PNG") {
        "image/png".to_owned()
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg".to_owned()
    } else if bytes.iter().take(64).any(|b| *b == b'<') {
        "image/svg+xml".to_owned()
    } else {
        "application/octet-stream".to_owned()
    }
}

/// The report as blocks for the profile's language. Time O(project size).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn report_doc_with_options(
    ws: &Workspace,
    ctx: &ReportContext,
    opts: &ReportOptions,
) -> ReportDoc {
    let languages = opts.languages();
    let mut blocks = Vec::new();
    for (i, lang) in languages.iter().enumerate() {
        if i > 0 {
            // A bilingual report writes the second language after the first,
            // with its own title block.
            blocks.push(Block::Heading(
                1,
                format!("{} — {}", ctx.project, lang.display_name()),
            ));
        }
        blocks.extend(build(ws, ctx, opts, *lang));
    }
    let first = Catalog::load(languages[0]);
    ReportDoc {
        title: first.tr_with("report-title", &[("project", &ctx.project)]),
        watermark: report_watermark(
            ctx,
            opts.profile.branding.classification,
            &first,
            languages[0],
        ),
        language: languages[0].code().to_owned(),
        blocks,
    }
}

/// The report as blocks, with every section and no filters (the default
/// profile). The sections and their order are those of
/// [`report_html`](super::report_html); a test keeps them equal.
#[must_use]
pub fn report_doc(ws: &Workspace, ctx: &ReportContext) -> ReportDoc {
    report_doc_with_options(ws, ctx, &ReportOptions::default())
}
