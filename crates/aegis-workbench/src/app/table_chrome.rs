//! Toolbar, persisted column chooser, per-column filter popover, and opt-in
//! read-only developer source view for [`TableTab`].

use aegis_client::command_table::command;
use aegis_client::explorer::{ExplorerKey, Folder};
use aegis_client::table::table_source_json;
use aegis_client::table_filter::{ColumnFilter, TableFilters};
use aegis_client::view::{ColumnKind, TableId};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, ElementId, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, StyledExt as _, Theme,
    h_flex, v_flex,
};

use super::icons::icon;
use super::table_tab::TableTab;

fn folder_for_table(table: TableId) -> Folder {
    match table {
        TableId::Assets => Folder::Assets,
        TableId::Damage => Folder::Damage,
        TableId::Threats => Folder::Threats,
        TableId::Risks => Folder::Risks,
        TableId::Controls => Folder::Controls,
        TableId::Goals => Folder::Goals,
        TableId::Claims => Folder::Claims,
        TableId::Problems => Folder::Problems,
        TableId::SharedSteps => Folder::SharedSteps,
        TableId::Fragments => Folder::Fragments,
    }
}

fn chip_label(heading: &str, filter: &ColumnFilter) -> String {
    match (!filter.query().is_empty(), filter.allowed().len()) {
        (true, n) if n > 0 => format!("{heading}: {}, {n}", filter.query()),
        (true, _) => format!("{heading}: {}", filter.query()),
        (false, n) if n > 0 => format!("{heading}: {n}"),
        _ => heading.to_owned(),
    }
}

fn strip(el: Div, th: &Theme) -> Div {
    el.w_full()
        .px_2()
        .py_1()
        .bg(th.title_bar)
        .border_b_1()
        .border_color(th.border)
}

fn bar_box(th: &Theme) -> Div {
    strip(h_flex().flex_wrap().items_center().gap_1(), th)
}

fn chip_box(bg: Hsla, border: Hsla) -> Div {
    h_flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_0p5()
        .rounded_sm()
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_xs()
}

fn check_mark(checked: bool, fg: Hsla) -> Div {
    div()
        .w(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(fg)
        .when(checked, |d| d.child(Icon::new(IconName::Check).xsmall()))
}

fn ghost_btn(id: impl Into<ElementId>) -> Button {
    Button::new(id).ghost().xsmall()
}

impl TableTab {
    fn recompute_filtered(&mut self, cx: &mut Context<Self>) {
        self.filtered_indices = self.filters.matching_rows(&self.rows, self.table.columns());
        self.clamp_selection();
        cx.notify();
    }

    fn sync_filter_input(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.syncing_filter = true;
        self.filter_input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.syncing_filter = false;
    }

    fn clear_column_and_input(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.filters.clear_column(key);
        if self.filter_col == Some(key) {
            self.sync_filter_input(String::new(), window, cx);
        }
        self.recompute_filtered(cx);
    }

    /// Toggles visibility of column `key`, persists it in workbench settings,
    /// and refreshes sibling tabs and `self` without double-leasing `self`.
    pub fn toggle_column(&mut self, key: &str, cx: &mut Context<Self>) {
        let visible = self.visible_cols.iter().any(|c| c.key == key);
        let Some(wb) = self.workbench.upgrade() else {
            return;
        };
        let my_id = cx.entity_id();
        wb.update(cx, |w, cx| {
            w.set_table_column_visible_skipping(self.table, key, !visible, Some(my_id), cx);
        });
        {
            let w = wb.read(cx);
            let tab_key = ExplorerKey::Folder(self.owner, folder_for_table(self.table));
            let title = w.table_title(tab_key, self.table);
            let cols = w.visible_columns_for(self.table);
            self.apply_refresh(title, w.workspace(), cols, w.developer_source_view());
        }
        cx.notify();
    }

    /// Sets the substring filter query for column `key` and recomputes matches.
    pub fn set_column_filter_query(
        &mut self,
        key: &str,
        query: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.filters.set_query(key, query);
        self.recompute_filtered(cx);
    }

    /// Toggles `value` in column `key`'s allowed value set and recomputes matches.
    pub fn toggle_column_filter_value(&mut self, key: &str, value: String, cx: &mut Context<Self>) {
        self.filters.toggle_value(key, value);
        self.recompute_filtered(cx);
    }

    /// Clears all per-column filters and restores the full row list.
    pub fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.filters.clear_all();
        self.recompute_filtered(cx);
    }

    /// Opens the filter popover for column `key` (or closes it if already open
    /// for `key`), syncing the shared text input without clobbering stored queries.
    pub fn open_filter_for(
        &mut self,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.filter_col == Some(key) {
            self.filter_col = None;
            cx.notify();
            return;
        }
        self.filter_col = Some(key);
        let existing = self.filters.column(key).map_or("", ColumnFilter::query);
        self.sync_filter_input(existing.to_owned(), window, cx);
        self.filter_input.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    /// Column key whose filter popover is currently open, if any.
    #[must_use]
    pub fn open_filter_column(&self) -> Option<&'static str> {
        self.filter_col
    }

    /// Whether this tab is currently showing the read-only developer source view.
    #[must_use]
    pub fn is_source_mode(&self) -> bool {
        self.developer_source_enabled && self.source_mode
    }

    /// Enables or disables read-only developer source mode (D40). Ignored when
    /// `developer_source_view` is disabled in settings.
    pub fn set_source_mode(
        &mut self,
        on: bool,
        ws_source: impl FnOnce() -> String,
        cx: &mut Context<Self>,
    ) {
        if !self.developer_source_enabled || !on {
            self.source_mode = false;
            cx.notify();
            return;
        }
        self.source_mode = true;
        self.filter_col = None;
        self.chooser_open = false;
        self.source_text = SharedString::from(ws_source());
        cx.notify();
    }

    /// Current read-only developer source JSON text.
    #[must_use]
    pub fn source_text(&self) -> &str {
        self.source_text.as_ref()
    }

    fn toggle_source(&mut self, cx: &mut Context<Self>) {
        let next = !self.source_mode;
        let json = (next && self.developer_source_enabled)
            .then(|| self.workbench.upgrade())
            .flatten()
            .map(|wb| table_source_json(wb.read(cx).workspace().project(), self.table))
            .unwrap_or_default();
        self.set_source_mode(next, || json, cx);
    }

    fn grid_btn(&self, id: &'static str, disabled: bool) -> Option<Button> {
        let spec = command(id)?;
        Some(
            ghost_btn(id)
                .icon(icon(spec.icon)?)
                .tooltip(self.catalog.tr(&spec.tooltip_id()))
                .disabled(disabled),
        )
    }

    pub(super) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let th = cx.theme().clone();
        let source_key = if self.source_mode {
            "grid-grid"
        } else {
            "grid-source"
        };
        let ro = self
            .workbench
            .upgrade()
            .is_none_or(|wb| wb.read(cx).read_only().is_some());
        let off = self.table == TableId::Problems || !self.can_run_grid_command() || ro;
        let rows_sel = self.selection.map_or(0, |s| s.row_count());
        let add_btn = self
            .grid_btn("grid-add-row", off)
            .map(|b| b.on_click(cx.listener(|t, _, w, cx| _ = t.add_row(w, cx))));
        let del_btn = self
            .grid_btn("grid-delete-rows", off || rows_sel == 0)
            .map(|b| b.on_click(cx.listener(|t, _, _, cx| _ = t.delete_selected(cx))));
        let fill_btn = self
            .grid_btn("grid-fill-down", off || rows_sel < 2)
            .map(|b| b.on_click(cx.listener(|t, _, _, cx| _ = t.fill_down(cx))));
        let chips = self
            .table
            .columns()
            .iter()
            .zip(&self.all_headings)
            .enumerate()
            .filter_map(|(ix, (col, heading))| {
                let filter = self.filters.column(col.key)?;
                let key = col.key;
                let rm = ghost_btn(("table-chip-rm", ix))
                    .icon(IconName::Close)
                    .on_click(cx.listener(move |t, _, w, cx| t.clear_column_and_input(key, w, cx)));
                Some(
                    chip_box(th.background, th.border)
                        .pr_1()
                        .child(chip_label(heading, filter))
                        .child(rm),
                )
            });
        bar_box(&th)
            .text_color(th.muted_foreground)
            .children(add_btn)
            .children(del_btn)
            .children(fill_btn)
            .children(chips)
            .when(self.filters.is_active(), |bar| {
                bar.child(
                    ghost_btn("table-clear-filters")
                        .label(self.catalog.tr("grid-clear-filters"))
                        .on_click(cx.listener(|t, _, w, cx| {
                            if t.filter_col.is_some() {
                                t.sync_filter_input(String::new(), w, cx);
                            }
                            t.clear_filters(cx);
                        })),
                )
            })
            .child(div().flex_1())
            .child(
                ghost_btn("table-columns")
                    .icon(IconName::Settings2)
                    .label(self.catalog.tr("grid-columns"))
                    .on_click(cx.listener(|t, _, _, cx| {
                        t.chooser_open = !t.chooser_open;
                        cx.notify();
                    })),
            )
            .when(self.developer_source_enabled, |bar| {
                bar.child(
                    ghost_btn("table-source-toggle")
                        .icon(IconName::SquareTerminal)
                        .label(self.catalog.tr(source_key))
                        .on_click(cx.listener(|t, _, _, cx| t.toggle_source(cx))),
                )
            })
    }

    pub(super) fn render_chooser(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.chooser_open {
            return None;
        }
        let th = cx.theme().clone();
        let (bg_on, bg_off, fg_on, fg_off) =
            (th.background, th.title_bar, th.primary, th.muted_foreground);
        let items = self
            .table
            .columns()
            .iter()
            .zip(&self.all_headings)
            .enumerate()
            .map(|(ix, (col, heading))| {
                let visible = self.visible_cols.iter().any(|c| c.key == col.key);
                let locked = col.kind == ColumnKind::Id;
                let key = col.key;
                chip_box(if visible { bg_on } else { bg_off }, th.border)
                    .id(("table-col-opt", ix))
                    .when(locked, |d| d.text_color(fg_off))
                    .when(!locked, |d| {
                        d.cursor_pointer()
                            .on_click(cx.listener(move |t, _, _, cx| t.toggle_column(key, cx)))
                    })
                    .child(check_mark(visible, if locked { fg_off } else { fg_on }))
                    .child(heading.clone())
            });
        let close = ghost_btn("table-chooser-close")
            .icon(IconName::Close)
            .on_click(cx.listener(|t, _, _, cx| {
                t.chooser_open = false;
                cx.notify();
            }));
        let bar = bar_box(&th)
            .children(items)
            .child(div().flex_1())
            .child(close);
        Some(bar.into_any_element())
    }

    pub(super) fn render_filter_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let key = self.filter_col?;
        let col_ix = self.table.columns().iter().position(|c| c.key == key)?;
        let heading = self.all_headings.get(col_ix).map_or(key, String::as_str);
        let allowed = self.filters.column(key).map(ColumnFilter::allowed);
        let th = cx.theme().clone();
        let values = TableFilters::distinct_values(&self.rows, col_ix)
            .into_iter()
            .enumerate()
            .map(|(ix, val)| {
                let checked = allowed.is_some_and(|set| set.contains(&val));
                let display = if val.is_empty() { "—" } else { val.as_str() }.to_owned();
                let border = if checked { th.primary } else { th.border };
                chip_box(th.background, border)
                    .id(("table-filter-val", ix))
                    .cursor_pointer()
                    .on_click(cx.listener(move |t, _, _, cx| {
                        t.toggle_column_filter_value(key, val.clone(), cx);
                    }))
                    .child(check_mark(checked, th.primary))
                    .child(display)
            });
        let input = div()
            .flex_1()
            .max_w(px(280.))
            .child(Input::new(&self.filter_input).small());
        let clear = ghost_btn("table-filter-clear")
            .label(self.catalog.tr("grid-clear"))
            .on_click(cx.listener(move |t, _, w, cx| t.clear_column_and_input(key, w, cx)));
        let close = ghost_btn("table-filter-close")
            .icon(IconName::Close)
            .on_click(cx.listener(|t, _, _, cx| {
                t.filter_col = None;
                cx.notify();
            }));
        let top = h_flex()
            .items_center()
            .gap_1p5()
            .child(div().text_xs().font_semibold().child(heading.to_owned()))
            .child(input)
            .child(clear)
            .child(close);
        let list = h_flex()
            .id("table-filter-values")
            .flex_wrap()
            .gap_1()
            .max_h(px(140.))
            .overflow_y_scroll()
            .children(values);
        Some(
            strip(v_flex().gap_1p5(), &th)
                .child(top)
                .child(list)
                .into_any_element(),
        )
    }

    pub(super) fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let th = cx.theme();
        div()
            .id("table-source-scroll")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_scroll()
            .p_3()
            .bg(th.background)
            .text_color(th.foreground)
            .font_family(th.mono_font_family.clone())
            .text_xs()
            .child(div().whitespace_nowrap().child(self.source_text.clone()))
    }
}
