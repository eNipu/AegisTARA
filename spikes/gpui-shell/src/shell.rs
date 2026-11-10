//! The `Shell` view: owns the dock area and lays out the panels.

use std::rc::Rc;

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px,
};
use gpui_component::dock::{
    BasePanel, DockArea, DockLayout, DockPlacement, DockSkin, Panel, PanelEvent, panel_handle,
};
use gpui_component::{ActiveTheme as _, v_flex};

use crate::editor_tab::EditorTab;
use crate::inspector::Inspector;
use crate::project_tree::ProjectTree;
use crate::selection::Selection;

/// The window's root view.
pub struct Shell {
    area: Entity<DockArea>,
    _skin: Rc<DockSkin>,
    _selection: Entity<Selection>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let selection = cx.new(|_| Selection::default());
        let (area, skin) = DockSkin::dock_area("shell", None, window, cx);

        let tree = cx.new(|cx| ProjectTree::new(selection.clone(), cx));
        let inspector = cx.new(|cx| Inspector::new(selection.clone(), cx));
        let attack_tree = cx.new(AttackTreeTab::new);
        let editor = cx.new(|cx| EditorTab::new(window, cx));

        let center = DockLayout::tabs()
            .panel_view(panel_handle(attack_tree), cx)
            .panel_view(panel_handle(editor), cx);
        let left = DockLayout::tabs().panel_view(panel_handle(tree), cx);
        let right = DockLayout::tabs().panel_view(panel_handle(inspector), cx);

        area.update(cx, |area, cx| {
            area.set_center(center, window, cx);
            area.set_dock(DockPlacement::Left, left, window, cx);
            area.set_dock(DockPlacement::Right, right, window, cx);
            area.set_dock_size(DockPlacement::Left, px(260.), window, cx);
            area.set_dock_size(DockPlacement::Right, px(300.), window, cx);
        });

        Self {
            area,
            _skin: skin,
            _selection: selection,
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.area.clone())
    }
}

/// Placeholder for the canvas that slice 181b draws.
pub struct AttackTreeTab {
    focus_handle: FocusHandle,
}

impl AttackTreeTab {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl BasePanel for AttackTreeTab {
    fn panel_name(&self) -> &'static str {
        "AttackTree"
    }
}

impl Panel for AttackTreeTab {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Attack tree"
    }
}

impl EventEmitter<PanelEvent> for AttackTreeTab {}

impl Focusable for AttackTreeTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AttackTreeTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("Attack tree canvas: slice 181b")
    }
}
