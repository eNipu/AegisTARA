//! Column-spec-driven GPUI table tab state and UI-14 presentation.

use std::rc::Rc;

use aegis_client::explorer::Owner;
use aegis_client::grid::GridSelection;
use aegis_client::i18n::Catalog;
use aegis_client::pages::vocabulary_for_lang;
use aegis_client::table::{
    Cell, LinkChip, TableRow, Tone, table_rows_for_owner, table_source_json,
};
use aegis_client::table_filter::{ColumnFilter, TableFilters};
use aegis_client::view::{ColumnKind, ColumnSpec, Selection, TableId};
use aegis_client::workspace::Workspace;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClickEvent, Context, Div, Entity, EntityId, FocusHandle, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex,
};

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

fn cell_box(width: u16, border: Hsla) -> Div {
    div()
        .w(px(f32::from(width)))
        .flex_shrink_0()
        .px_2()
        .border_r_1()
        .border_color(border)
        .overflow_hidden()
}

fn pill_box(bg: Hsla, fg: Hsla) -> Div {
    div()
        .px_1p5()
        .py_0p5()
        .rounded_sm()
        .bg(bg)
        .text_xs()
        .text_color(fg)
}

fn with_tip(el: Stateful<Div>, tip: &str) -> Stateful<Div> {
    if tip.is_empty() {
        return el;
    }
    let tip = SharedString::from(tip.to_owned());
    el.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
}

fn tone_pill_colors(tone: Tone, dark_mode: bool, tokens: &Tokens) -> (Hsla, Hsla) {
    let (bg, fg) = match (tone, dark_mode) {
        (Tone::VeryLow, false) => ("#E1F5EE", "#005A40"),
        (Tone::Low, false) => ("#E3F2FD", "#0B4F7A"),
        (Tone::Medium, false) => ("#FEF3D6", "#7A4D00"),
        (Tone::High, false) => ("#FDE6DC", "#8F2D00"),
        (Tone::VeryLow, true) => ("#0E3329", "#72DFBD"),
        (Tone::Low, true) => ("#123047", "#8BD0F7"),
        (Tone::Medium, true) => ("#3D2B08", "#F5C451"),
        (Tone::High, true) => ("#451C0C", "#FF9E75"),
        (Tone::Unrated, _) => (tokens.raised.as_str(), tokens.ink_muted.as_str()),
    };
    (color(bg), color(fg))
}

fn excluded_stripe(excluded: bool, tokens: &Tokens) -> Div {
    let slot = div().relative().w(px(4.)).flex_shrink_0();
    if !excluded {
        return slot;
    }
    let bar = color(&tokens.line_strong);
    let stripes = v_flex()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .bg(color(&tokens.raised))
        .children((0..16).map(move |i| {
            div()
                .w_full()
                .h(px(3.))
                .flex_shrink_0()
                .when(i % 2 == 0, |d| d.bg(bar))
        }));
    slot.child(stripes)
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
    pub(super) all_headings: Vec<String>,
    pub(super) headings: Vec<String>,
    pub(super) filters: TableFilters,
    pub(super) filtered_indices: Vec<usize>,
    pub(super) selection: Option<GridSelection>,
    pub(super) chooser_open: bool,
    pub(super) filter_col: Option<&'static str>,
    pub(super) filter_input: Entity<InputState>,
    pub(super) syncing_filter: bool,
    _filter_sub: Subscription,
    pub(super) developer_source_enabled: bool,
    pub(super) source_mode: bool,
    pub(super) source_text: SharedString,
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
        developer_source_enabled: bool,
        catalog: Rc<Catalog>,
        workbench: WeakEntity<Workbench>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (rows, all_headings, headings) =
            build_rows_and_headings(ws, table, owner, &visible_cols, &catalog);
        let filters = TableFilters::default();
        let filtered_indices = filters.matching_rows(&rows, table.columns());
        let placeholder = SharedString::from(catalog.tr("grid-filters"));
        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let _filter_sub = cx.subscribe(&filter_input, |this, input, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev
                && !this.syncing_filter
                && let Some(col_key) = this.filter_col
            {
                let val = input.read(cx).value().to_string();
                let cur = this.filters.column(col_key).map_or("", ColumnFilter::query);
                if val != cur {
                    this.set_column_filter_query(col_key, val, cx);
                }
            }
        });
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
            chooser_open: false,
            filter_col: None,
            filter_input,
            syncing_filter: false,
            _filter_sub,
            developer_source_enabled,
            source_mode: false,
            source_text: SharedString::default(),
            light: light(),
            dark: dark(),
        }
    }

    pub(super) fn apply_refresh(
        &mut self,
        title: SharedString,
        ws: &Workspace,
        visible_cols: Vec<&'static ColumnSpec>,
        developer_source_enabled: bool,
    ) {
        self.title = title;
        self.visible_cols = visible_cols;
        self.developer_source_enabled = developer_source_enabled;
        if !developer_source_enabled {
            self.source_mode = false;
        }
        if let Some(fcol) = self.filter_col
            && !self.visible_cols.iter().any(|c| c.key == fcol)
        {
            self.filter_col = None;
        }
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
        if developer_source_enabled && self.source_mode {
            self.source_text = SharedString::from(table_source_json(ws.project(), self.table));
        }
    }

    /// Rebuilds rows, headings, and filter matches from `ws`, clamping any active selection.
    pub fn refresh(
        &mut self,
        title: SharedString,
        ws: &Workspace,
        visible_cols: Vec<&'static ColumnSpec>,
        developer_source_enabled: bool,
        _skip: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        self.apply_refresh(title, ws, visible_cols, developer_source_enabled);
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

    /// The shared column-filter text input entity.
    #[must_use]
    pub fn filter_input(&self) -> &Entity<InputState> {
        &self.filter_input
    }

    /// Whether the `"Grid"` key context is active for this tab.
    #[must_use]
    pub fn grid_keys_active(&self) -> bool {
        !self.source_mode && self.filter_col.is_none()
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

    fn open_link_chip(&mut self, target: Selection, window: &mut Window, cx: &mut Context<Self>) {
        let my_id = cx.entity_id();
        if let Some(wb) = self.workbench.upgrade() {
            wb.update(cx, |w, cx| {
                w.open_target_skipping(target, Some(my_id), window, cx);
            });
        }
    }

    fn render_link_chip(
        &self,
        id: SharedString,
        chip: &LinkChip,
        excluded: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let target = chip.target;
        let text = if chip.visual_id.is_empty() {
            chip.label.clone()
        } else {
            chip.visual_id.clone()
        };
        let fg = color(if excluded {
            &tokens.ink_muted
        } else {
            &tokens.accent
        });
        let el = pill_box(color(&tokens.raised), fg)
            .id(id)
            .border_1()
            .border_color(color(&tokens.line))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_link_chip(target, window, cx);
            }))
            .child(text);
        with_tip(el, &chip.label)
    }

    fn render_cell_content(
        &self,
        pos: (usize, usize),
        col: &'static ColumnSpec,
        cell: Option<&Cell>,
        excluded: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if matches!(col.kind, ColumnKind::Links(_))
            && let Some(c) = cell.filter(|c| !c.chips.is_empty())
        {
            let (vis_row, vis_col) = pos;
            let chips = c.chips.iter().enumerate().map(|(ix, chip)| {
                let id = SharedString::from(format!("table-chip-{vis_row}-{vis_col}-{ix}"));
                self.render_link_chip(id, chip, excluded, tokens, cx)
            });
            return h_flex()
                .flex_wrap()
                .gap_1()
                .min_w_0()
                .children(chips)
                .into_any_element();
        }
        if let Some(c) = cell
            && let Some(tone) = c.tone
            && !c.text.is_empty()
        {
            let (bg, fg) = tone_pill_colors(tone, cx.theme().is_dark(), tokens);
            return pill_box(bg, fg)
                .font_semibold()
                .truncate()
                .child(c.text.clone())
                .into_any_element();
        }
        let text = cell.map_or("", |c| c.text.as_str()).to_owned();
        div()
            .min_w_0()
            .flex_1()
            .truncate()
            .child(text)
            .into_any_element()
    }

    pub(super) fn render_cell(
        &self,
        vis_row: usize,
        vis_col: usize,
        col: &'static ColumnSpec,
        row: &TableRow,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let cell = self
            .table
            .columns()
            .iter()
            .position(|c| c.key == col.key)
            .and_then(|ix| row.cells.get(ix));
        let sel = self.selection;
        let in_sel = sel.is_some_and(|s| s.contains(vis_row, vis_col));
        let active = sel.is_some_and(|s| s.active_row == vis_row && s.active_col == vis_col);
        let computed = col.kind == ColumnKind::Computed;
        let (line, accent, wash, raised, muted) = (
            color(&tokens.line),
            color(&tokens.accent),
            color(&tokens.accent_wash),
            color(&tokens.raised),
            color(&tokens.ink_muted),
        );
        let content =
            self.render_cell_content((vis_row, vis_col), col, cell, row.excluded, tokens, cx);
        let badge_col = self
            .visible_cols
            .iter()
            .position(|c| c.kind == ColumnKind::Name)
            .unwrap_or(0);
        let body: AnyElement = if row.excluded && vis_col == badge_col {
            let badge = pill_box(raised, muted)
                .id(("table-excluded-badge", vis_row))
                .flex_shrink_0()
                .border_1()
                .border_color(color(&tokens.line_strong))
                .child(self.catalog.tr("grid-excluded-badge"));
            let badge = with_tip(badge, row.excluded_reason.as_deref().unwrap_or(""));
            h_flex()
                .gap_1p5()
                .min_w_0()
                .flex_1()
                .child(content)
                .child(badge)
                .into_any_element()
        } else {
            content
        };
        let stride = self.visible_cols.len().max(1);
        cell_box(col.width, line)
            .id(("table-cell", vis_row * stride + vis_col))
            .py_1()
            .flex()
            .items_center()
            .min_w_0()
            .cursor_pointer()
            .when(in_sel, |d| d.bg(wash))
            .when(!in_sel && computed, |d| d.bg(raised))
            .when(computed || row.excluded, |d| d.text_color(muted))
            .when(active, |d| d.border_1().border_color(accent))
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                this.select_cell(vis_row, vis_col, ev.modifiers().shift, window, cx);
            }))
            .child(body)
    }

    fn render_row(
        &self,
        vis_row: usize,
        row: &TableRow,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let (line, wash, muted) = (
            color(&tokens.line),
            color(&tokens.accent_wash),
            color(&tokens.ink_muted),
        );
        let cells = self
            .visible_cols
            .iter()
            .enumerate()
            .map(|(vis_col, &col)| self.render_cell(vis_row, vis_col, col, row, tokens, cx));
        let row_sel = self
            .selection
            .is_some_and(|s| s.row_range().contains(&vis_row));
        h_flex()
            .min_w_full()
            .flex_shrink_0()
            .items_stretch()
            .border_b_1()
            .border_color(line)
            .when(row_sel, |d| d.bg(wash.opacity(0.4)))
            .when(row.excluded, |d| d.text_color(muted))
            .child(excluded_stripe(row.excluded, tokens))
            .children(cells)
    }

    fn render_header(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        let (line, muted, accent) = (
            color(&tokens.line),
            color(&tokens.ink_muted),
            color(&tokens.accent),
        );
        let computed_tip = self.catalog.tr("grid-computed-tip");
        let cells = self
            .visible_cols
            .iter()
            .zip(&self.headings)
            .enumerate()
            .map(|(vis_col, (col, heading))| {
                let key = col.key;
                let active = self.filters.column(key).is_some() || self.filter_col == Some(key);
                cell_box(col.width, line)
                    .py_1p5()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().flex_1().min_w_0().truncate().child(heading.clone()))
                    .when(col.kind == ColumnKind::Computed, |d| {
                        let mark = div()
                            .id(("table-hdr-computed", vis_col))
                            .flex_shrink_0()
                            .child(Icon::new(IconName::Asterisk).xsmall());
                        d.child(with_tip(mark, &computed_tip))
                    })
                    .child(
                        div()
                            .id(("table-hdr-filter", vis_col))
                            .flex_shrink_0()
                            .p_0p5()
                            .rounded_sm()
                            .cursor_pointer()
                            .when(active, |d| d.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_filter_for(key, window, cx);
                            }))
                            .child(Icon::new(IconName::Search).xsmall()),
                    )
            });
        h_flex()
            .min_w_full()
            .flex_shrink_0()
            .items_stretch()
            .bg(color(&tokens.raised))
            .border_b_1()
            .border_color(line)
            .text_xs()
            .font_semibold()
            .text_color(muted)
            .child(div().w(px(4.)).flex_shrink_0())
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
        let muted = color(&t.ink_muted);
        if self.is_source_mode() {
            return v_flex()
                .id("table-tab")
                .track_focus(&self.focus_handle)
                .size_full()
                .bg(color(&t.surface))
                .text_color(color(&t.ink))
                .text_sm()
                .child(self.render_toolbar(cx))
                .children(self.render_chooser(cx))
                .children(self.render_filter_panel(cx))
                .child(self.render_source(cx));
        }

        let header = self.render_header(t, cx);

        let body: AnyElement = if self.filtered_indices.is_empty() {
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
                    Some(self.render_row(r, row, t, cx))
                });
            v_flex()
                .min_w_full()
                .flex_shrink_0()
                .children(rows)
                .into_any_element()
        };

        let table_width = px(4.0
            + f32::from(
                self.visible_cols
                    .iter()
                    .fold(0u16, |acc, c| acc.saturating_add(c.width)),
            ));

        v_flex()
            .id("table-tab")
            .when(self.grid_keys_active(), |d| d.key_context("Grid"))
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(color(&t.surface))
            .text_color(color(&t.ink))
            .text_sm()
            .child(self.render_toolbar(cx))
            .children(self.render_chooser(cx))
            .children(self.render_filter_panel(cx))
            .child(
                v_flex()
                    .id("table-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_scroll()
                    .child(
                        v_flex()
                            .min_w_full()
                            .w(table_width)
                            .flex_shrink_0()
                            .child(header)
                            .child(body),
                    ),
            )
    }
}
