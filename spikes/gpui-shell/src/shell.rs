//! The `Shell` view: owns the dock area and lays out the panels.

use aegis_model::AttackTree;
use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window,
    div, px,
};
use gpui_component::dock::{DockArea, DockLayout, DockPlacement, DockSkin, panel_handle};
use std::rc::Rc;
use std::time::Instant;

use crate::args::Args;

use crate::canvas_tab::AttackTreeTab;
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
    pub fn new(
        tree_data: &AttackTree,
        started: Instant,
        args: &Args,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let selection = cx.new(|_| Selection::default());
        let (area, skin) = DockSkin::dock_area("shell", None, window, cx);
        let tree = cx.new(|cx| ProjectTree::new(selection.clone(), cx));
        let inspector = cx.new(|cx| Inspector::new(selection.clone(), cx));
        let attack_tree =
            cx.new(|cx| AttackTreeTab::new(tree_data, selection.clone(), started, args, cx));
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
