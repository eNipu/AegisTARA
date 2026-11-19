//! Column-spec-driven GPUI table tab core state and grid scaffold (UI-14).

use std::rc::Rc;

use aegis_client::explorer::Owner;
use aegis_client::grid::GridSelection;
use aegis_client::i18n::Catalog;
use aegis_client::pages::vocabulary_for_lang;
use aegis_client::table::{TableRow, table_rows_for_owner};
use aegis_client::table_filter::TableFilters;
use aegis_client::view::{ColumnSpec, TableId};
use aegis_client::workspace::Workspace;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    ClickEvent, Context, EntityId, FocusHandle, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, div, px,
};
use gpui_component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};

use super::color;
use super::panels::dock_panel;
use super::workbench::Workbench;
use crate::theme::{Tokens, dark, light};

fn build_rows_and_headings(
    ws: &Workspace,
    table: TableId,
    owner: Owner,
    visible_cols: &[&'static ColumnSpec],
    catalog: &Catalog,
) -> (Vec<TableRow>, Vec<String>, Vec<String>) {
    let vocab = vocabulary_for_lang(ws.project().methodology(), catalog.language().code());
    let rows = table_rows_for_owner(ws, table, owner, &vocab, catalog);
    let heading = |col: &ColumnSpec| col.heading(&vocab.element_names, catalog);
    let all_headings = table.columns().iter().map(heading).collect();
    let headings = visible_cols.iter().copied().map(heading).collect();
    (rows, all_headings, headings)
}

fn cell_box(width: u16, border: Hsla) -> gpui::Div {
    div()
        .w(px(f32::from(width)))
        .flex_shrink_0()
        .px_2()
        .border_r_1()
        .border_color(border)
        .truncate()
}

/// Center dock tab rendering one [`TableId`] table driven by [`ColumnSpec`].
pub struct TableTab {
    pub(super) focus_handle: FocusHandle,
    pub(super) title: SharedString,
    pub(super) table: TableId,
    pub(super) owner: Owner,
    pub(super) catalog: Rc<Catalog>,
    pub(super) workbench: WeakEntity<Workbench>,
    pub(super) rows: Vec<TableRow>,
    pub(super) visible_cols: Vec<&'static ColumnSpec>,
    #[allow(dead_code)] // Read by 189d2 filter chips for hidden columns.
    pub(super) all_headings: Vec<String>,
    pub(super) headings: Vec<String>,
    pub(super) filters: TableFilters,
    pub(super) filtered_indices: Vec<usize>,
    pub(super) selection: Option<GridSelection>,
    pub(super) light: Tokens,
    pub(super) dark: Tokens,
}

dock_panel!(TableTab, "TableTab");

impl TableTab {
    /// Creates a new table tab for `(table, owner)` with the given visible columns.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        title: SharedString,
        table: TableId,
        owner: Owner,
        ws: &Workspace,
        visible_cols: Vec<&'static ColumnSpec>,
        catalog: Rc<Catalog>,
        workbench: WeakEntity<Workbench>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (rows, all_headings, headings) =
            build_rows_and_headings(ws, table, owner, &visible_cols, &catalog);
        let filters = TableFilters::default();
        let filtered_indices = filters.matching_rows(&rows, table.columns());
        Self {
            focus_handle: cx.focus_handle(),
            title,
            table,
            owner,
            catalog,
            workbench,
            rows,
            visible_cols,
            all_headings,
            headings,
            filters,
            filtered_indices,
            selection: None,
            light: light(),
            dark: dark(),
        }
    }

    /// Rebuilds rows, headings, and filter matches from `ws`, clamping any active selection.
    pub fn refresh(
        &mut self,
        title: SharedString,
        ws: &Workspace,
        visible_cols: Vec<&'static ColumnSpec>,
        _skip: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        self.title = title;
        self.visible_cols = visible_cols;
        let built = build_rows_and_headings(
            ws,
            self.table,
            self.owner,
            &self.visible_cols,
            &self.catalog,
        );
        (self.rows, self.all_headings, self.headings) = built;
        self.filtered_indices = self.filters.matching_rows(&self.rows, self.table.columns());
        self.clamp_selection();
        cx.notify();
    }

    pub(super) fn clamp_selection(&mut self) {
        let (rows, cols) = (self.filtered_indices.len(), self.visible_cols.len());
        if rows == 0 || cols == 0 {
            self.selection = None;
        } else if let Some(sel) = &mut self.selection {
            sel.clamp(rows, cols);
        }
    }

    /// The table rendered by this tab.
    #[must_use]
    pub fn table(&self) -> TableId {
        self.table
    }

    /// The explorer owner scope of this tab.
    #[must_use]
    pub fn owner(&self) -> Owner {
        self.owner
    }

    /// The currently visible column specifications, in table column order.
    #[must_use]
    pub fn visible_columns(&self) -> &[&'static ColumnSpec] {
        &self.visible_cols
    }

    /// Number of rows currently visible after applying [`Self::filters`].
    #[must_use]
    pub fn visible_row_count(&self) -> usize {
        self.filtered_indices.len()
    }

    /// The row displayed at visible row index `vis_row`, if in bounds.
    #[must_use]
    pub fn displayed_row(&self, vis_row: usize) -> Option<&TableRow> {
        self.filtered_indices
            .get(vis_row)
            .and_then(|&i| self.rows.get(i))
    }

    /// Active per-column filters for this table tab.
    #[must_use]
    pub fn filters(&self) -> &TableFilters {
        &self.filters
    }

    /// Current rectangular cell selection, if any.
    #[must_use]
    pub fn selection(&self) -> Option<GridSelection> {
        self.selection
    }

    /// Sets the rectangular cell selection, clamping to current visible bounds.
    pub fn set_selection(&mut self, sel: Option<GridSelection>, cx: &mut Context<Self>) {
        self.selection = sel;
        self.clamp_selection();
        cx.notify();
    }

    pub(super) fn grid_keys_active(&self) -> bool {
        true
    }

    fn select_cell(
        &mut self,
        vis_row: usize,
        vis_col: usize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if extend && let Some(sel) = &mut self.selection {
            sel.extend_to(vis_row, vis_col);
        } else {
            self.selection = Some(GridSelection::single(vis_row, vis_col));
        }
        self.clamp_selection();
        window.focus(&self.focus_handle, cx);
        let target = self.displayed_row(vis_row).and_then(|r| r.target);
        let my_id = cx.entity_id();
        if let Some(wb) = self.workbench.upgrade() {
            wb.update(cx, |w, cx| {
                w.select_element_skipping(target, Some(my_id), cx);
            });
        }
        cx.notify();
    }

    fn render_row(
        &self,
        vis_row: usize,
        row: &TableRow,
        col_indices: &[usize],
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let (line, accent, wash) = (
            color(&tokens.line),
            color(&tokens.accent),
            color(&tokens.accent_wash),
        );
        let stride = self.visible_cols.len().max(1);
        let sel = self.selection;
        let cells = self.visible_cols.iter().zip(col_indices).enumerate().map(
            |(vis_col, (col, &all_ix))| {
                let text = row.cells.get(all_ix).map_or("", |c| c.text.as_str());
                let in_sel = sel.is_some_and(|s| s.contains(vis_row, vis_col));
                let active =
                    sel.is_some_and(|s| s.active_row == vis_row && s.active_col == vis_col);
                cell_box(col.width, if active { accent } else { line })
                    .id(("table-cell", vis_row * stride + vis_col))
                    .py_1()
                    .when(in_sel, |d| d.bg(wash))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        this.select_cell(vis_row, vis_col, ev.modifiers().shift, window, cx);
                    }))
                    .child(text.to_owned())
            },
        );
        let row_sel = sel.is_some_and(|s| s.row_range().contains(&vis_row));
        h_flex()
            .w_full()
            .border_b_1()
            .border_color(line)
            .when(row_sel, |d| d.bg(wash.opacity(0.4)))
            .children(cells)
    }
}

impl Render for TableTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = if cx.theme().is_dark() {
            &self.dark
        } else {
            &self.light
        };
        let (line, muted) = (color(&t.line), color(&t.ink_muted));
        let all_cols = self.table.columns();
        let col_indices: Vec<usize> = self
            .visible_cols
            .iter()
            .filter_map(|vc| all_cols.iter().position(|c| c.key == vc.key))
            .collect();

        let header = h_flex()
            .w_full()
            .bg(color(&t.raised))
            .border_b_1()
            .border_color(line)
            .text_xs()
            .font_semibold()
            .text_color(muted)
            .children(
                self.visible_cols
                    .iter()
                    .zip(&self.headings)
                    .map(|(c, h)| cell_box(c.width, line).py_1p5().child(h.clone())),
            );

        let body: gpui::AnyElement = if self.filtered_indices.is_empty() {
            div()
                .p_4()
                .text_color(muted)
                .child(self.catalog.tr("grid-no-matches"))
                .into_any_element()
        } else {
            let rows = self
                .filtered_indices
                .iter()
                .enumerate()
                .filter_map(|(r, &i)| {
                    let row = self.rows.get(i)?;
                    Some(self.render_row(r, row, &col_indices, t, cx))
                });
            v_flex().children(rows).into_any_element()
        };

        v_flex()
            .id("table-tab")
            .when(self.grid_keys_active(), |d| d.key_context("Grid"))
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(color(&t.surface))
            .text_color(color(&t.ink))
            .text_sm()
            .child(
                v_flex()
                    .id("table-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_scroll()
                    .child(header)
                    .child(body),
            )
    }
}
