//! The `Item definition` tab: a multi-line text editor with sample text.

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window,
};
use gpui_component::dock::{BasePanel, Panel, PanelEvent};
use gpui_component::input::{Editor, EditorState};
use gpui_component::v_flex;

/// `# Item definition` followed by numbered lines, `lines` lines in total.
pub fn sample_text(lines: usize) -> String {
    let mut out = Vec::with_capacity(lines);
    if lines > 0 {
        out.push("# Item definition".to_string());
    }
    for n in 2..=lines {
        out.push(format!(
            "Line {n}: the gateway routes CAN frames between domains."
        ));
    }
    out.join("\n")
}

/// The `Item definition` panel.
pub struct EditorTab {
    focus_handle: FocusHandle,
    editor: Entity<EditorState>,
}

impl EditorTab {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| EditorState::new(window, cx).default_value(sample_text(2000)));
        Self {
            focus_handle: cx.focus_handle(),
            editor,
        }
    }
}

impl BasePanel for EditorTab {
    fn panel_name(&self) -> &'static str {
        "ItemDefinition"
    }
}

impl Panel for EditorTab {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Item definition"
    }
}

impl EventEmitter<PanelEvent> for EditorTab {}

impl Focusable for EditorTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(Editor::new(&self.editor).h_full().bordered(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_text_has_2000_lines() {
        let text = sample_text(2000);
        assert_eq!(text.lines().count(), 2000);
        assert!(text.starts_with("# Item definition\n"));
        assert_eq!(
            text.lines().nth(1),
            Some("Line 2: the gateway routes CAN frames between domains.")
        );
    }
}
