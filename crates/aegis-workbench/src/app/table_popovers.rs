//! Typed rating editor popovers (`SingleChoice`, `MultiChoice`, and atomic
//! `ImpactRatings`) and local staging helpers for [`TableTab`].

use aegis_client::grid::CellInput;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex,
};

use super::table_edit::ActiveEditor;
use super::table_tab::TableTab;

fn check_slot(checked: bool, fg: Hsla) -> Div {
    div()
        .w(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .text_color(fg)
        .when(checked, |d| d.child(Icon::new(IconName::Check).xsmall()))
}

fn checkbox_box(checked: bool, primary: Hsla, border: Hsla, accent: Hsla) -> Div {
    div()
        .w(px(14.))
        .h(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(if checked { primary } else { border })
        .text_color(primary)
        .when(checked, |d| {
            d.bg(accent).child(Icon::new(IconName::Check).xsmall())
        })
}

fn popover_footer(
    cancel_id: &'static str,
    apply_id: &'static str,
    apply_label: String,
    border: Hsla,
    apply: fn(&mut TableTab, &mut Window, &mut Context<TableTab>) -> bool,
    cx: &mut Context<TableTab>,
) -> Div {
    h_flex()
        .justify_end()
        .gap_1()
        .pt_1p5()
        .border_t_1()
        .border_color(border)
        .child(
            Button::new(cancel_id)
                .ghost()
                .xsmall()
                .icon(IconName::Close)
                .on_click(cx.listener(|this, _, w, cx| this.cancel_edit(w, cx))),
        )
        .child(
            Button::new(apply_id)
                .primary()
                .xsmall()
                .label(apply_label)
                .on_click(cx.listener(move |this, _, w, cx| {
                    apply(this, w, cx);
                })),
        )
}

impl TableTab {
    /// Toggles option `index` inside [`ActiveEditor::MultiChoice`] without
    /// mutating the project until [`Self::apply_multi_choice`].
    pub fn toggle_multi_choice_option(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(ActiveEditor::MultiChoice { options, .. }) = &mut self.editor
            && let Some((_, checked)) = options.get_mut(index)
        {
            *checked = !*checked;
            cx.notify();
        }
    }

    /// Commits the staged [`ActiveEditor::MultiChoice`] flags via
    /// [`Self::commit_input`], preserving the editor on validation failure.
    pub fn apply_multi_choice(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(ActiveEditor::MultiChoice { options, .. }) = &self.editor else {
            return false;
        };
        let flags = options.iter().map(|(_, checked)| *checked).collect();
        self.commit_input(CellInput::MultiChoice(flags), window, cx)
    }

    /// Updates `category`'s staged level inside [`ActiveEditor::ImpactRatings`]
    /// locally without mutating the project until [`Self::apply_impact_ratings`].
    pub fn set_impact_editor_level(
        &mut self,
        category: usize,
        level: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        if let Some(ActiveEditor::ImpactRatings { categories, .. }) = &mut self.editor
            && let Some((_, _, selected)) = categories.get_mut(category)
        {
            *selected = level;
            cx.notify();
        }
    }

    /// Commits all staged [`ActiveEditor::ImpactRatings`] levels atomically via
    /// [`CellInput::Impacts`] in a single undo step.
    pub fn apply_impact_ratings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(ActiveEditor::ImpactRatings { categories, .. }) = &self.editor else {
            return false;
        };
        let levels = categories.iter().map(|(_, _, level)| *level).collect();
        self.commit_input(CellInput::Impacts(levels), window, cx)
    }

    pub(super) fn render_single_choice(
        &self,
        options: &[String],
        selected: Option<usize>,
        allow_clear: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let th = cx.theme();
        let (active_bg, primary, border, muted) =
            (th.list_active, th.primary, th.border, th.muted_foreground);
        let rows = options.iter().enumerate().map(|(ix, opt)| {
            let is_sel = selected == Some(ix);
            h_flex()
                .id(("table-choice-opt", ix))
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .text_sm()
                .when(is_sel, |d| d.bg(active_bg))
                .child(check_slot(is_sel, primary))
                .child(div().flex_1().truncate().child(opt.clone()))
                .on_click(cx.listener(move |this, _, w, cx| {
                    this.commit_input(CellInput::SingleChoice(Some(ix)), w, cx);
                }))
        });
        v_flex()
            .gap_0p5()
            .children(rows)
            .when(allow_clear, |col| {
                col.child(
                    div().border_t_1().border_color(border).mt_1().pt_1().child(
                        h_flex()
                            .id("table-choice-clear")
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(muted)
                            .child(
                                div()
                                    .w(px(14.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(Icon::new(IconName::Close).xsmall()),
                            )
                            .child(self.catalog.tr("grid-clear"))
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.commit_input(CellInput::SingleChoice(None), w, cx);
                            })),
                    ),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_multi_choice(
        &self,
        options: &[(String, bool)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let th = cx.theme();
        let (primary, border, accent) = (th.primary, th.border, th.accent);
        let rows = options.iter().enumerate().map(|(ix, (label, checked))| {
            h_flex()
                .id(("table-multi-opt", ix))
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .text_sm()
                .child(checkbox_box(*checked, primary, border, accent))
                .child(div().flex_1().truncate().child(label.clone()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_multi_choice_option(ix, cx);
                }))
        });
        let list = v_flex().gap_0p5().children(rows);
        let footer = popover_footer(
            "table-multi-cancel",
            "table-multi-apply",
            self.catalog.tr("grid-apply"),
            border,
            Self::apply_multi_choice,
            cx,
        );
        v_flex()
            .gap_2()
            .child(list)
            .child(footer)
            .into_any_element()
    }

    pub(super) fn render_impact_ratings(
        &self,
        categories: &[(String, Vec<String>, Option<usize>)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let th = cx.theme();
        let (primary, border, accent, bg, fg) = (
            th.primary,
            th.border,
            th.accent,
            th.background,
            th.foreground,
        );
        let clear_label = self.catalog.tr("grid-clear");
        let stride = categories
            .iter()
            .map(|(_, levels, _)| levels.len())
            .max()
            .unwrap_or(1)
            .max(1);
        let rows = categories
            .iter()
            .enumerate()
            .map(|(cat_ix, (cat_label, levels, selected))| {
                let chips = levels.iter().enumerate().map(|(lvl_ix, lvl_label)| {
                    let is_sel = *selected == Some(lvl_ix);
                    div()
                        .id(("table-impact-lvl", cat_ix * stride + lvl_ix))
                        .px_2()
                        .py_0p5()
                        .rounded_sm()
                        .border_1()
                        .border_color(if is_sel { primary } else { border })
                        .bg(if is_sel { accent } else { bg })
                        .text_xs()
                        .text_color(if is_sel { primary } else { fg })
                        .when(is_sel, |d| d.font_semibold())
                        .cursor_pointer()
                        .child(lvl_label.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_impact_editor_level(cat_ix, Some(lvl_ix), cx);
                        }))
                });
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(div().text_xs().font_semibold().child(cat_label.clone()))
                            .child(
                                Button::new(("table-impact-clear", cat_ix))
                                    .ghost()
                                    .xsmall()
                                    .label(clear_label.clone())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_impact_editor_level(cat_ix, None, cx);
                                    })),
                            ),
                    )
                    .child(h_flex().flex_wrap().gap_1().children(chips))
            });
        let list = v_flex().gap_2().children(rows);
        let footer = popover_footer(
            "table-impact-cancel",
            "table-impact-apply",
            self.catalog.tr("grid-apply"),
            border,
            Self::apply_impact_ratings,
            cx,
        );
        v_flex()
            .gap_2()
            .child(list)
            .child(footer)
            .into_any_element()
    }
}
