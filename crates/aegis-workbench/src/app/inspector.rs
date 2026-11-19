//! Relation-backed GPUI inspector dock panel driven by [`RELATIONS`] (UI-15).

use std::rc::Rc;

use aegis_client::i18n::Catalog;
use aegis_client::pages::selection_visual_id;
use aegis_client::picker::MultiLinkPicker;
use aegis_client::relations::{self, ElementKind, RELATIONS, RelationKey};
use aegis_client::table::{LinkChip, selection_exclusion, selection_label};
use aegis_client::view::Selection;
use aegis_model::{ElementNamesDef, Project};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, Entity, EntityId, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::tooltip::Tooltip;
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, StyledExt as _, Theme, h_flex,
    v_flex,
};

use super::panels::dock_panel;
use super::picker_view::MultiLinkPickerView;
use super::workbench::{Workbench, kind};

type InspectorHeader = (String, String, String, Option<String>);

/// One relation row displayed in [`InspectorPanel`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorRelationRow {
    /// Stable relation identifier from [`RELATIONS`].
    pub key: RelationKey,
    /// Localized display label for this relation.
    pub label: String,
    /// Whether this relation can be edited with [`MultiLinkPickerView`].
    pub editable: bool,
    /// Currently linked elements shown as clickable chips.
    pub chips: Vec<LinkChip>,
}

fn selection_exists(project: &Project, sel: Selection) -> bool {
    use Selection as S;
    match sel {
        S::Component(id) => project.component_idx(&id).is_some(),
        S::Interface(id) => project.interface_idx(&id).is_some(),
        S::DataFlow(id) => project.data_flow_idx(&id).is_some(),
        S::TrustBoundary(id) => project.trust_boundary_idx(&id).is_some(),
        S::Asset(id) => project.asset_idx(&id).is_some(),
        S::Damage(id) => project.damage_scenario_idx(&id).is_some(),
        S::Threat(id) => project.threat_scenario_idx(&id).is_some(),
        S::Control(id) => project.security_control_idx(&id).is_some(),
        S::Goal(id) => project.cybersecurity_goal_idx(&id).is_some(),
        S::Claim(id) => project.cybersecurity_claim_idx(&id).is_some(),
        S::Party(id) => project.party_idx(&id).is_some(),
        S::Condition(id) => project.condition_idx(&id).is_some(),
        S::Tree(id) => project.attack_tree_idx(&id).is_some(),
        S::TreeNode(tree_id, node_id) => project
            .attack_tree_idx(&tree_id)
            .and_then(|i| project.attack_tree(i))
            .is_some_and(|tree| tree.idx_of(&node_id).is_some()),
    }
}

fn chip_box(theme: &Theme) -> gpui::Div {
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

/// Relation-backed inspector dock panel driven by [`RELATIONS`] (UI-15).
pub struct InspectorPanel {
    focus_handle: FocusHandle,
    title: SharedString,
    catalog: Rc<Catalog>,
    workbench: WeakEntity<Workbench>,
    selection: Option<Selection>,
    header: Option<InspectorHeader>,
    rows: Vec<InspectorRelationRow>,
    active_picker: Option<(RelationKey, Entity<MultiLinkPickerView>)>,
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
            header: None,
            rows: Vec::new(),
            active_picker: None,
        }
    }

    fn build_state(
        &self,
        sel: Selection,
        project: &Project,
        names: &ElementNamesDef,
    ) -> Option<(InspectorHeader, Vec<InspectorRelationRow>)> {
        if !selection_exists(project, sel) {
            return None;
        }
        let header = (
            selection_visual_id(project, sel),
            selection_label(project, sel),
            kind(names, sel).unwrap_or_default(),
            selection_exclusion(project, sel).1,
        );
        let elem_kind = ElementKind::kind_of(sel);
        let rows = RELATIONS
            .iter()
            .filter(|rel| Some(rel.from) == elem_kind)
            .map(|rel| InspectorRelationRow {
                key: rel.key,
                label: self.catalog.tr(rel.label),
                editable: rel.link.is_some(),
                chips: relations::related(project, rel.key, sel)
                    .into_iter()
                    .map(|target| LinkChip {
                        target,
                        visual_id: selection_visual_id(project, target),
                        label: selection_label(project, target),
                    })
                    .collect(),
            })
            .collect();
        Some((header, rows))
    }

    fn apply_built(
        &mut self,
        sel: Option<Selection>,
        built: Option<(InspectorHeader, Vec<InspectorRelationRow>)>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(sel), Some((header, rows))) = (sel, built) else {
            self.selection = None;
            self.header = None;
            self.rows.clear();
            self.active_picker = None;
            cx.notify();
            return false;
        };
        self.header = Some(header);
        self.rows = rows;
        let same = self.selection == Some(sel);
        if !same {
            self.selection = Some(sel);
            self.active_picker = None;
        }
        cx.notify();
        same
    }

    /// Updates the displayed element and relation rows from `project`, clearing
    /// stale/deleted selections and refreshing an open picker unless `skip` matches it.
    pub fn set_selection(
        &mut self,
        sel: Option<Selection>,
        project: &Project,
        names: &ElementNamesDef,
        skip: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        let built = sel.and_then(|s| self.build_state(s, project, names));
        if self.apply_built(sel, built, cx)
            && let Some((_, picker)) = &self.active_picker
            && Some(picker.entity_id()) != skip
        {
            picker.update(cx, |p, cx| p.refresh(project, cx));
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

    /// The relation rows currently displayed for [`Self::selection`].
    #[must_use]
    pub fn relation_rows(&self) -> &[InspectorRelationRow] {
        &self.rows
    }

    fn open_chip(&mut self, target: Selection, window: &mut Window, cx: &mut Context<Self>) {
        let Some(wb) = self.workbench.upgrade() else {
            return;
        };
        let my_id = cx.entity_id();
        wb.update(cx, |w, cx| {
            w.open_target_skipping(target, Some(my_id), window, cx);
        });
        let built = {
            let w = wb.read(cx);
            self.build_state(target, w.workspace().project(), &w.names)
        };
        self.apply_built(Some(target), built, cx);
    }

    fn render_header(&self, header: &InspectorHeader, cx: &Context<Self>) -> gpui::Div {
        let (visual_id, name, kind_label, excluded_reason) = header;
        let t = cx.theme();
        let badge = excluded_reason.as_ref().map(|reason| {
            let tip = SharedString::from(reason.clone());
            chip_box(t)
                .id("inspector-excluded-badge")
                .text_color(t.muted_foreground)
                .child(self.catalog.tr("grid-excluded-badge"))
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        });
        let meta = h_flex()
            .flex_wrap()
            .gap_1p5()
            .text_xs()
            .text_color(t.muted_foreground)
            .when(!visual_id.is_empty(), |d| d.child(visual_id.clone()))
            .when(!kind_label.is_empty(), |d| d.child(kind_label.clone()))
            .children(badge);
        let title = div()
            .text_base()
            .font_semibold()
            .text_color(t.foreground)
            .child(name.clone());
        v_flex()
            .gap_1()
            .pb_2()
            .border_b_1()
            .border_color(t.border)
            .child(meta)
            .child(title)
    }

    fn render_row(
        &self,
        row_ix: usize,
        row: &InspectorRelationRow,
        read_only: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let key = row.key;
        let active_view = self
            .active_picker
            .as_ref()
            .filter(|(k, _)| *k == key)
            .map(|(_, view)| view.clone());
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
            let id = SharedString::from(format!("inspector-chip-{row_ix}-{ix}"));
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
        let Some(header) = self.header.as_ref() else {
            return root
                .text_color(cx.theme().muted_foreground)
                .child(self.catalog.tr("workbench-inspector-empty"));
        };
        let read_only = self
            .workbench
            .upgrade()
            .is_some_and(|wb| wb.read(cx).read_only().is_some());
        let rows: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| self.render_row(ix, row, read_only, cx))
            .collect();
        let muted = cx.theme().muted_foreground;
        let heading = div()
            .text_xs()
            .font_semibold()
            .text_color(muted)
            .child(self.catalog.tr("inspector-relations"));
        root.child(self.render_header(header, cx))
            .child(v_flex().gap_2().child(heading).children(rows))
    }
}
