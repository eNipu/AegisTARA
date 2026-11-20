//! Relation-backed GPUI inspector dock panel with explain-why and trace-spine navigation (WBX-06, UI-15).

use std::rc::Rc;

use aegis_client::i18n::Catalog;
use aegis_client::inspector::inspector_view;
pub use aegis_client::inspector::{
    CauseTarget, InspectorExplain, InspectorField, InspectorRelationRow, InspectorView,
};
use aegis_client::picker::MultiLinkPicker;
use aegis_client::relations::{self, ElementKind, RelationKey};
use aegis_client::table::Tone;
use aegis_client::view::Selection;
use aegis_client::workspace::Workspace;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, Div, Entity, EntityId, FocusHandle, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::tooltip::Tooltip;
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, StyledExt as _, Theme, h_flex,
    v_flex,
};

use super::color;
use super::panels::dock_panel;
use super::picker_view::MultiLinkPickerView;
use super::workbench::Workbench;
use crate::theme::{Tokens, dark, light};

fn chip_box(theme: &Theme) -> Div {
    h_flex()
        .gap_1()
        .px_2()
        .py_0p5()
        .rounded_sm()
        .bg(theme.title_bar)
        .border_1()
        .border_color(theme.border)
        .text_xs()
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

fn section_heading(text: String, muted: Hsla) -> Div {
    div()
        .text_xs()
        .font_semibold()
        .text_color(muted)
        .child(text)
}

/// Relation-backed inspector dock panel driven by [`inspector_view`] (WBX-06, UI-15).
pub struct InspectorPanel {
    focus_handle: FocusHandle,
    title: SharedString,
    catalog: Rc<Catalog>,
    workbench: WeakEntity<Workbench>,
    selection: Option<Selection>,
    view: Option<InspectorView>,
    active_picker: Option<(RelationKey, Entity<MultiLinkPickerView>)>,
    light: Tokens,
    dark: Tokens,
}

dock_panel!(InspectorPanel, "InspectorPanel");

impl InspectorPanel {
    /// Creates an empty inspector dock panel.
    pub fn new(
        title: SharedString,
        catalog: Rc<Catalog>,
        workbench: WeakEntity<Workbench>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            title,
            catalog,
            workbench,
            selection: None,
            view: None,
            active_picker: None,
            light: light(),
            dark: dark(),
        }
    }

    fn apply_built(
        &mut self,
        sel: Option<Selection>,
        built: Option<InspectorView>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(sel), Some(view)) = (sel, built) else {
            self.selection = None;
            self.view = None;
            self.active_picker = None;
            cx.notify();
            return false;
        };
        self.view = Some(view);
        let same = self.selection == Some(sel);
        if !same {
            self.selection = Some(sel);
            self.active_picker = None;
        }
        cx.notify();
        same
    }

    /// Updates the displayed element and inspector view from `ws`, clearing
    /// stale/deleted selections and refreshing an open picker unless `skip` matches it.
    pub fn set_selection(
        &mut self,
        sel: Option<Selection>,
        ws: &Workspace,
        skip: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        let built = sel.and_then(|s| inspector_view(ws, s, &self.catalog));
        if self.apply_built(sel, built, cx)
            && let Some((_, picker)) = &self.active_picker
            && Some(picker.entity_id()) != skip
        {
            picker.update(cx, |p, cx| p.refresh(ws.project(), cx));
        }
    }

    /// Opens the inline [`MultiLinkPickerView`] for `key`, refusing in read-only mode.
    pub fn open_picker(&mut self, key: RelationKey, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(from), Some(wb)) = (self.selection, self.workbench.upgrade()) else {
            return;
        };
        if !relations::relation(key)
            .is_some_and(|r| r.link.is_some() && Some(r.from) == ElementKind::kind_of(from))
        {
            return;
        }
        if wb.read(cx).read_only().is_some() {
            wb.update(cx, |w, cx| {
                w.refuse_read_only(cx);
            });
            return;
        }
        let picker = MultiLinkPicker::open(wb.read(cx).workspace().project(), key, from);
        let cat = Rc::clone(&self.catalog);
        let wb_weak = self.workbench.clone();
        let me = cx.weak_entity();
        let view = cx.new(|cx| {
            let mut v = MultiLinkPickerView::from_picker(picker, cat, wb_weak, window, cx);
            v.set_on_close(move |_, cx| {
                let _ = me.update(cx, |this, cx| this.close_picker(cx));
            });
            v
        });
        self.active_picker = Some((key, view));
        cx.notify();
    }

    /// Closes the inline multi-link picker if one is open.
    pub fn close_picker(&mut self, cx: &mut Context<Self>) {
        if self.active_picker.take().is_some() {
            cx.notify();
        }
    }

    /// The currently inspected element, if any.
    #[must_use]
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// The currently open inline multi-link picker view, if any.
    #[must_use]
    pub fn active_picker(&self) -> Option<&Entity<MultiLinkPickerView>> {
        self.active_picker.as_ref().map(|(_, picker)| picker)
    }

    /// The complete toolkit-independent inspector view model for [`Self::selection`].
    #[must_use]
    pub fn view(&self) -> Option<&InspectorView> {
        self.view.as_ref()
    }

    /// The forward relation rows currently displayed for [`Self::selection`].
    #[must_use]
    pub fn relation_rows(&self) -> &[InspectorRelationRow] {
        self.view.as_ref().map_or(&[], |v| v.relations.as_slice())
    }

    /// The reverse trace-spine neighbour rows currently displayed for [`Self::selection`].
    #[must_use]
    pub fn reverse_relation_rows(&self) -> &[InspectorRelationRow] {
        self.view
            .as_ref()
            .map_or(&[], |v| v.reverse_relations.as_slice())
    }

    /// The explain-why derivation trace and cause targets for [`Self::selection`].
    #[must_use]
    pub fn explain(&self) -> Option<&InspectorExplain> {
        self.view.as_ref().and_then(|v| v.explain.as_ref())
    }

    /// The primary deciding or excluding cause target for [`Self::selection`].
    #[must_use]
    pub fn primary_cause(&self) -> Option<Selection> {
        self.view.as_ref().and_then(|v| v.primary_cause)
    }

    /// Jumps to the primary deciding or excluding cause of [`Self::selection`].
    pub fn go_to_cause(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Selection> {
        let target = self.primary_cause()?;
        self.open_chip(target, window, cx);
        Some(target)
    }

    /// Navigates the workbench and inspector to `target` without double-leasing.
    pub fn navigate_to(&mut self, target: Selection, window: &mut Window, cx: &mut Context<Self>) {
        self.open_chip(target, window, cx);
    }

    fn open_chip(&mut self, target: Selection, window: &mut Window, cx: &mut Context<Self>) {
        let Some(wb) = self.workbench.upgrade() else {
            return;
        };
        let my_id = cx.entity_id();
        wb.update(cx, |w, cx| {
            w.open_target_skipping(target, Some(my_id), window, cx);
        });
        let built = inspector_view(wb.read(cx).workspace(), target, &self.catalog);
        self.apply_built(Some(target), built, cx);
    }

    fn render_cause_chip(
        &self,
        prefix: &str,
        ix: usize,
        cause: &CauseTarget,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let target = cause.target;
        let (muted, fg) = (cx.theme().muted_foreground, cx.theme().foreground);
        chip_box(cx.theme())
            .id(SharedString::from(format!("{prefix}-{ix}")))
            .text_color(fg)
            .cursor_pointer()
            .when(!cause.role.is_empty(), |d| {
                d.child(div().text_color(muted).child(format!("{}:", cause.role)))
            })
            .when(!cause.visual_id.is_empty(), |d| {
                d.child(div().text_color(muted).child(cause.visual_id.clone()))
            })
            .child(div().truncate().child(cause.label.clone()))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_chip(target, window, cx);
            }))
    }

    fn render_header(&self, view: &InspectorView, cx: &mut Context<Self>) -> Div {
        let (border, muted, fg) = (
            cx.theme().border,
            cx.theme().muted_foreground,
            cx.theme().foreground,
        );
        let badge = view.excluded_reason.as_ref().map(|reason| {
            let tip = SharedString::from(reason.clone());
            chip_box(cx.theme())
                .id("inspector-excluded-badge")
                .text_color(muted)
                .child(self.catalog.tr("grid-excluded-badge"))
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        });
        let meta = h_flex()
            .flex_wrap()
            .gap_1p5()
            .text_xs()
            .text_color(muted)
            .when(!view.visual_id.is_empty(), |d| {
                d.child(view.visual_id.clone())
            })
            .when(!view.kind.is_empty(), |d| d.child(view.kind.clone()))
            .children(badge);
        let title = div()
            .text_base()
            .font_semibold()
            .text_color(fg)
            .child(view.name.clone());
        let excl_causes: Vec<_> = view
            .exclusion_causes
            .iter()
            .enumerate()
            .map(|(ix, c)| self.render_cause_chip("inspector-excl-cause", ix, c, cx))
            .collect();
        v_flex()
            .gap_1()
            .pb_2()
            .border_b_1()
            .border_color(border)
            .child(meta)
            .child(title)
            .when(!view.systems.is_empty(), |d| {
                let text = format!(
                    "{}: {}",
                    self.catalog.tr("inspector-used-in"),
                    view.systems.join(", ")
                );
                d.child(div().text_xs().text_color(muted).child(text))
            })
            .when(!view.changed.is_empty(), |d| {
                d.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(view.changed.clone()),
                )
            })
            .when(!view.description.is_empty(), |d| {
                d.child(
                    div()
                        .text_xs()
                        .text_color(fg)
                        .child(view.description.clone()),
                )
            })
            .when(!excl_causes.is_empty(), |d| {
                d.child(h_flex().flex_wrap().gap_1().children(excl_causes))
            })
    }

    fn render_fields(&self, fields: &[InspectorField], cx: &Context<Self>) -> Div {
        let is_dark = cx.theme().mode.is_dark();
        let tokens = if is_dark { &self.dark } else { &self.light };
        let (muted, fg) = (cx.theme().muted_foreground, cx.theme().foreground);
        let rows = fields.iter().map(|f| {
            let val = match f.tone {
                Some(tone) => {
                    let (bg, pill_fg) = tone_pill_colors(tone, is_dark, tokens);
                    pill_box(bg, pill_fg).child(f.value.clone())
                }
                None => div().text_xs().text_color(fg).child(f.value.clone()),
            };
            h_flex()
                .justify_between()
                .items_center()
                .gap_2()
                .child(div().text_xs().text_color(muted).child(f.label.clone()))
                .child(val)
        });
        v_flex()
            .gap_1p5()
            .child(section_heading(self.catalog.tr("inspector-details"), muted))
            .children(rows)
    }

    fn render_explain(
        &self,
        explain: &InspectorExplain,
        has_primary: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let (muted, fg) = (cx.theme().muted_foreground, cx.theme().foreground);
        let go_btn = has_primary.then(|| {
            Button::new("inspector-go-to-cause")
                .ghost()
                .xsmall()
                .icon(IconName::ExternalLink)
                .label(self.catalog.tr("cmd-go-to-cause"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.go_to_cause(window, cx);
                }))
        });
        let head = h_flex()
            .justify_between()
            .items_center()
            .gap_1()
            .child(section_heading(
                self.catalog.tr("inspector-explain-why"),
                muted,
            ))
            .children(go_btn);
        let lines = explain
            .lines
            .iter()
            .map(|l| div().text_xs().text_color(fg).child(l.clone()));
        let causes: Vec<_> = explain
            .causes
            .iter()
            .enumerate()
            .map(|(ix, c)| self.render_cause_chip("inspector-cause", ix, c, cx))
            .collect();
        v_flex()
            .gap_1p5()
            .child(head)
            .children(lines)
            .when(!causes.is_empty(), |d| {
                d.child(h_flex().flex_wrap().gap_1().children(causes))
            })
    }

    fn render_row(
        &self,
        prefix: &str,
        row_ix: usize,
        row: &InspectorRelationRow,
        read_only: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let key = row.key;
        let active_view = row
            .editable
            .then(|| {
                self.active_picker
                    .as_ref()
                    .filter(|(k, _)| *k == key)
                    .map(|(_, view)| view.clone())
            })
            .flatten();
        let is_active = active_view.is_some();
        let edit_btn = row.editable.then(|| {
            Button::new(("inspector-edit", row_ix))
                .ghost()
                .xsmall()
                .icon(IconName::ExternalLink)
                .tooltip(self.catalog.tr("picker-edit-links"))
                .disabled(read_only)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if is_active {
                        this.close_picker(cx);
                    } else {
                        this.open_picker(key, window, cx);
                    }
                }))
        });
        let (muted, fg) = (cx.theme().muted_foreground, cx.theme().foreground);
        let chips = row.chips.iter().enumerate().map(|(ix, chip)| {
            let target = chip.target;
            let id = SharedString::from(format!("{prefix}-chip-{row_ix}-{ix}"));
            chip_box(cx.theme())
                .id(id)
                .text_color(fg)
                .cursor_pointer()
                .when(!chip.visual_id.is_empty(), |d| {
                    d.child(div().text_color(muted).child(chip.visual_id.clone()))
                })
                .child(div().truncate().child(chip.label.clone()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_chip(target, window, cx);
                }))
        });
        let label = div()
            .flex_1()
            .text_xs()
            .text_color(muted)
            .child(row.label.clone());
        let head = h_flex()
            .justify_between()
            .gap_1()
            .child(label)
            .children(edit_btn);
        v_flex()
            .gap_1()
            .child(head)
            .child(h_flex().flex_wrap().gap_1().children(chips))
            .children(active_view)
    }
}

impl Render for InspectorPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = v_flex()
            .id("inspector-panel")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .text_sm();
        let Some(view) = self.view.as_ref() else {
            return root
                .text_color(cx.theme().muted_foreground)
                .child(self.catalog.tr("workbench-inspector-empty"));
        };
        let read_only = self
            .workbench
            .upgrade()
            .is_some_and(|wb| wb.read(cx).read_only().is_some());
        let header = self.render_header(view, cx);
        let fields = (!view.fields.is_empty()).then(|| self.render_fields(&view.fields, cx));
        let explain = view
            .explain
            .as_ref()
            .map(|ex| self.render_explain(ex, view.primary_cause.is_some(), cx));
        let fwd_rows: Vec<_> = view
            .relations
            .iter()
            .enumerate()
            .map(|(ix, row)| self.render_row("inspector", ix, row, read_only, cx))
            .collect();
        let rev_rows: Vec<_> = view
            .reverse_relations
            .iter()
            .enumerate()
            .map(|(ix, row)| self.render_row("inspector-rev", ix, row, read_only, cx))
            .collect();
        let muted = cx.theme().muted_foreground;
        root.child(header)
            .children(fields)
            .children(explain)
            .when(!fwd_rows.is_empty(), |d| {
                let head = section_heading(self.catalog.tr("inspector-relations"), muted);
                d.child(v_flex().gap_2().child(head).children(fwd_rows))
            })
            .when(!rev_rows.is_empty(), |d| {
                let head = section_heading(self.catalog.tr("inspector-used-by"), muted);
                d.child(v_flex().gap_2().child(head).children(rev_rows))
            })
    }
}
