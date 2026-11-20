//! Keyboard navigation, inline cell editors, and multi-cell commits for [`TableTab`].

use std::rc::Rc;

use aegis_client::explorer::{ExplorerKey, Folder};
use aegis_client::grid::{self, CellEditorKind, CellInput, GridSelection};
use aegis_client::pages::vocabulary_for_lang;
use aegis_client::picker::MultiLinkPicker;
use aegis_client::view::TableId;
use aegis_model::Project;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EntityId, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, ParentElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _, h_flex, v_flex};

use super::picker_view::MultiLinkPickerView;
use super::table_tab::TableTab;
use super::workbench::Workbench;

fn folder_for(table: TableId) -> Folder {
    match table {
        TableId::Assets => Folder::Assets,
        TableId::Damage => Folder::Damage,
        TableId::Threats => Folder::Threats,
        TableId::Risks => Folder::Risks,
        TableId::Controls => Folder::Controls,
        TableId::Goals => Folder::Goals,
        TableId::Claims => Folder::Claims,
        TableId::Problems => Folder::Problems,
    }
}

/// Active inline editor or popover inside a [`TableTab`].
#[allow(dead_code)] // SingleChoice/MultiChoice/ImpactRatings rendered in 189e1b.
pub(super) enum ActiveEditor {
    Text {
        row: usize,
        col: usize,
        input: Entity<InputState>,
        _sub: Subscription,
    },
    SingleChoice {
        row: usize,
        col: usize,
        options: Vec<String>,
        selected: Option<usize>,
        allow_clear: bool,
    },
    MultiChoice {
        row: usize,
        col: usize,
        options: Vec<(String, bool)>,
    },
    ImpactRatings {
        row: usize,
        col: usize,
        categories: Vec<(String, Vec<String>, Option<usize>)>,
    },
    LinkPicker {
        row: usize,
        col: usize,
        view: Entity<MultiLinkPickerView>,
    },
}

impl ActiveEditor {
    pub(super) fn pos(&self) -> (usize, usize) {
        match self {
            Self::Text { row, col, .. }
            | Self::SingleChoice { row, col, .. }
            | Self::MultiChoice { row, col, .. }
            | Self::ImpactRatings { row, col, .. }
            | Self::LinkPicker { row, col, .. } => (*row, *col),
        }
    }
}

impl TableTab {
    pub(super) fn sync_workbench_selection(&self, cx: &mut Context<Self>) {
        let target = self
            .selection
            .and_then(|s| self.displayed_row(s.active_row))
            .and_then(|r| r.target);
        if let Some(wb) = self.workbench.upgrade() {
            let id = cx.entity_id();
            wb.update(cx, |w, cx| w.select_element_skipping(target, Some(id), cx));
        }
    }

    pub(super) fn sync_editor_on_refresh(
        &mut self,
        project: Option<&Project>,
        skip: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        match &self.editor {
            Some(ActiveEditor::LinkPicker { view, .. }) => {
                if Some(view.entity_id()) != skip
                    && let Some(p) = project
                {
                    let view = view.clone();
                    view.update(cx, |v, cx| v.refresh(p, cx));
                }
            }
            Some(ed) => {
                let (r, c) = ed.pos();
                if r >= self.filtered_indices.len() || c >= self.visible_cols.len() {
                    self.editor = None;
                }
            }
            None => {}
        }
    }

    /// Refreshes this tab from `wb` after `apply_edit_skipping` (never double-leasing `self`).
    pub(super) fn self_refresh(&mut self, wb: &Entity<Workbench>, cx: &mut Context<Self>) {
        let project = {
            let w = wb.read(cx);
            let key = ExplorerKey::Folder(self.owner, folder_for(self.table));
            let cols = w.visible_columns_for(self.table);
            self.apply_refresh(
                w.table_title(key, self.table),
                w.workspace(),
                cols,
                w.developer_source_view(),
            );
            self.active_link_picker()
                .map(|_| w.workspace().project().clone())
        };
        self.sync_editor_on_refresh(project.as_ref(), None, cx);
        cx.notify();
    }

    fn edit_workbench(&self, cx: &mut Context<Self>) -> Option<Entity<Workbench>> {
        if self.source_mode {
            return None;
        }
        let wb = self.workbench.upgrade()?;
        if wb.read(cx).read_only().is_some() {
            wb.update(cx, |w, cx| _ = w.refuse_read_only(cx));
            return None;
        }
        Some(wb)
    }

    fn prepare_cell(
        &self,
        p: &Project,
    ) -> Option<(usize, usize, CellEditorKind, Option<MultiLinkPicker>)> {
        let sel = self.selection?;
        let (row, col) = (sel.active_row, sel.active_col);
        let target = self.displayed_row(row)?.target?;
        let spec = *self.visible_cols.get(col)?;
        let vocab = vocabulary_for_lang(p.methodology(), self.catalog.language().code());
        let kind = grid::cell_editor(p, self.table, Some(target), spec, &vocab, &self.catalog);
        let picker = match &kind {
            CellEditorKind::ReadOnly => return None,
            CellEditorKind::LinkPicker { relation } => {
                Some(MultiLinkPicker::open(p, *relation, target))
            }
            _ => None,
        };
        Some((row, col, kind, picker))
    }

    fn open_editor_kind(
        &mut self,
        (row, col, kind, picker): (usize, usize, CellEditorKind, Option<MultiLinkPicker>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use ActiveEditor as E;
        use CellEditorKind as K;
        let editor = match kind {
            K::ReadOnly => return false,
            K::Text { current } => {
                let input = cx.new(|cx| InputState::new(window, cx).default_value(current));
                input.update(cx, |s, cx| s.focus(window, cx));
                let _sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, w, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        this.commit_text_edit(w, cx);
                    }
                });
                E::Text {
                    row,
                    col,
                    input,
                    _sub,
                }
            }
            K::SingleChoice {
                options: o,
                selected: s,
                allow_clear: c,
            } => E::SingleChoice {
                row,
                col,
                options: o,
                selected: s,
                allow_clear: c,
            },
            K::MultiChoice { options } => E::MultiChoice { row, col, options },
            K::ImpactRatings { categories } => E::ImpactRatings {
                row,
                col,
                categories,
            },
            K::LinkPicker { .. } => {
                let Some(picker) = picker else {
                    return false;
                };
                let (cat, wb, me) = (
                    Rc::clone(&self.catalog),
                    self.workbench.clone(),
                    cx.weak_entity(),
                );
                let view = cx.new(|cx| {
                    let mut v = MultiLinkPickerView::from_picker(picker, cat, wb, window, cx);
                    v.set_on_close(move |w, cx| {
                        let _ = me.update(cx, |this, cx| this.cancel_edit(w, cx));
                    });
                    v
                });
                E::LinkPicker { row, col, view }
            }
        };
        self.editor = Some(editor);
        cx.notify();
        true
    }

    /// Opens an inline editor using `project` directly without borrowing `Workbench`.
    #[allow(dead_code)] // Used by 189e2's grid-add-row without re-leasing Workbench.
    pub(super) fn start_edit_with_project(
        &mut self,
        project: &Project,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.source_mode {
            return false;
        }
        self.prepare_cell(project)
            .is_some_and(|p| self.open_editor_kind(p, window, cx))
    }

    /// Opens the inline editor for the active cell, refusing in read-only mode.
    pub fn start_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(wb) = self.edit_workbench(cx) else {
            return false;
        };
        let prepared = self.prepare_cell(wb.read(cx).workspace().project());
        prepared.is_some_and(|p| self.open_editor_kind(p, window, cx))
    }

    /// Closes any active inline editor without committing and restores focus.
    pub fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// Whether an inline cell editor or popover is currently active.
    #[must_use]
    pub fn is_editing(&self) -> bool {
        self.editor.is_some()
    }

    /// Active inline multi-link picker view, when editing a `Links` cell.
    #[must_use]
    pub fn active_link_picker(&self) -> Option<&Entity<MultiLinkPickerView>> {
        match &self.editor {
            Some(ActiveEditor::LinkPicker { view, .. }) => Some(view),
            _ => None,
        }
    }

    /// Commits `input` across every selected row in the active (or editor) column.
    ///
    /// Preserves `self.editor` on error so the user can fix input.
    pub fn commit_input(
        &mut self,
        input: CellInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(wb) = self.edit_workbench(cx) else {
            return false;
        };
        let Some(sel) = self.selection else {
            return false;
        };
        let col_ix = self.editor.as_ref().map_or(sel.active_col, |e| e.pos().1);
        let Some(&col) = self.visible_cols.get(col_ix) else {
            return false;
        };
        let targets: Vec<_> = sel
            .row_range()
            .filter_map(|r| self.displayed_row(r)?.target)
            .collect();
        let res = {
            let p = wb.read(cx).workspace().project();
            grid::edit_cells(p, self.table, &targets, col, &input)
        };
        let edit = match res {
            Ok(edit) => edit,
            Err(err) => {
                wb.update(cx, |w, cx| w.set_note(Some(err.to_string()), cx));
                return false;
            }
        };
        let id = cx.entity_id();
        if !wb.update(cx, |w, cx| w.apply_edit_skipping(&edit, Some(id), cx)) {
            return false;
        }
        self.editor = None;
        self.self_refresh(&wb, cx);
        window.focus(&self.focus_handle, cx);
        true
    }

    /// Reads the active [`ActiveEditor::Text`] input and commits it via [`Self::commit_input`].
    pub fn commit_text_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(ActiveEditor::Text { input, .. }) = &self.editor else {
            return false;
        };
        let val = input.read(cx).value().to_string();
        self.commit_input(CellInput::Text(val), window, cx)
    }

    /// Handles logical grid keys (`"escape"`, arrows, `"tab"`, `"enter"`, `"f2"`).
    pub fn handle_grid_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if key == "escape" {
            if self.editor.is_some() {
                self.cancel_edit(window, cx);
                return true;
            }
            return false;
        }
        if self.editor.is_some() || self.source_mode || self.filter_col.is_some() {
            return false;
        }
        match key {
            "up" | "down" | "left" | "right" | "tab" => {
                let (rows, cols) = (self.filtered_indices.len(), self.visible_cols.len());
                if rows == 0 || cols == 0 {
                    return false;
                }
                if let Some(sel) = &mut self.selection {
                    let (dr, dc, ext) = match key {
                        "up" => (-1, 0, shift),
                        "down" => (1, 0, shift),
                        "left" => (0, -1, shift),
                        "right" => (0, 1, shift),
                        _ => (0, if shift { -1 } else { 1 }, false),
                    };
                    sel.move_by(dr, dc, rows, cols, ext);
                } else {
                    self.selection = Some(GridSelection::single(0, 0));
                }
                self.sync_workbench_selection(cx);
                cx.notify();
                true
            }
            "enter" | "f2" => self.start_edit(window, cx),
            _ => false,
        }
    }

    pub(super) fn handle_key_event(
        &mut self,
        ev: &KeyDownEvent,
        capture: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let m = ev.keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return;
        }
        let key = ev.keystroke.key.as_str();
        let handled = if capture {
            key == "escape"
                && self.editor.is_some()
                && !matches!(self.editor, Some(ActiveEditor::LinkPicker { .. }))
                && self.handle_grid_key("escape", false, w, cx)
        } else {
            self.handle_grid_key(key, m.shift, w, cx)
        };
        if handled {
            cx.stop_propagation();
        }
    }

    pub(super) fn render_editor_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Some(ActiveEditor::LinkPicker { col, view, .. }) = &self.editor else {
            return None;
        };
        let th = cx.theme();
        let heading = self.headings.get(*col).cloned().unwrap_or_default();
        let header = h_flex()
            .justify_between()
            .px_2()
            .py_1()
            .bg(th.title_bar)
            .border_b_1()
            .border_color(th.border)
            .child(div().text_xs().font_semibold().child(heading))
            .child(
                Button::new("table-editor-close")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .on_click(cx.listener(|this, _, w, cx| this.cancel_edit(w, cx))),
            );
        Some(
            v_flex()
                .id("table-editor-overlay")
                .absolute()
                .top(px(44.))
                .right(px(12.))
                .w(px(360.))
                .bg(th.background)
                .border_1()
                .border_color(th.border)
                .rounded_md()
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(header)
                .child(div().p_2().child(view.clone()))
                .into_any_element(),
        )
    }
}
