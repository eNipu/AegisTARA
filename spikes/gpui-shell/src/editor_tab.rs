//! The `Item definition` tab: a multi-line text editor with sample text.

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, IntoElement, ParentElement as _, Render,
    Styled as _, Window,
};
use gpui_component::input::{Editor, EditorState};
use gpui_component::v_flex;

/// `# Item definition` then numbered lines, `lines` lines in total (at least one).
pub fn sample_text(lines: usize) -> String {
    let mut text = String::from("# Item definition");
    for n in 2..=lines {
        text += &format!("\nLine {n}: the gateway routes CAN frames between domains.");
    }
    text
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

dock_panel!(EditorTab, "ItemDefinition", "Item definition");

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
        assert!(text.starts_with("# Item definition\nLine 2: the gateway"));
    }
}
