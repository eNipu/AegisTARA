//! The inspector panel: shows what the project tree selected.

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Window, div, px,
};
use gpui_component::dock::{BasePanel, Panel, PanelEvent};
use gpui_component::{ActiveTheme as _, h_flex, v_flex};

use crate::selection::Selection;

/// The `Inspector` panel.
pub struct Inspector {
    focus_handle: FocusHandle,
    selection: Entity<Selection>,
    _observe: Subscription,
}

impl Inspector {
    pub fn new(selection: Entity<Selection>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&selection, |_, _, cx| cx.notify());
        Self {
            focus_handle: cx.focus_handle(),
            selection,
            _observe: observe,
        }
    }
}

impl BasePanel for Inspector {
    fn panel_name(&self) -> &'static str {
        "Inspector"
    }
}

impl Panel for Inspector {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Inspector"
    }
}

impl EventEmitter<PanelEvent> for Inspector {}

impl Focusable for Inspector {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Inspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let selection = self.selection.read(cx);
        let line = |name: &'static str, value: String| {
            h_flex()
                .gap_2()
                .child(div().w(px(56.)).text_color(muted).child(name))
                .child(SharedString::from(value))
        };
        v_flex()
            .size_full()
            .p_3()
            .gap_1()
            .child(line("Kind", selection.kind.to_string()))
            .child(line("Name", selection.label.clone()))
            .child(line("Detail", selection.detail.clone()))
    }
}
