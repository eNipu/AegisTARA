//! The project tree panel: a flat, indented outline of the sample workspace.

use gpui::{
    Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{ActiveTheme as _, h_flex, v_flex};

use crate::selection::Selection;

/// One row of the outline.
pub struct OutlineRow {
    pub depth: u8,
    pub kind: &'static str,
    pub label: String,
    pub count: Option<u32>,
}

fn row(depth: u8, kind: &'static str, label: &str, count: Option<u32>) -> OutlineRow {
    let label = label.to_string();
    OutlineRow {
        depth,
        kind,
        label,
        count,
    }
}

/// The sample outline the spike shows, in display order.
pub fn sample_outline() -> Vec<OutlineRow> {
    vec![
        row(0, "workspace", "Sample workspace", None),
        row(1, "system", "Gateway", None),
        row(2, "component", "Central gateway", None),
        row(3, "assets", "Assets", Some(12)),
        row(3, "damage_scenarios", "Damage scenarios", Some(8)),
        row(3, "threats", "Threats", Some(21)),
        row(3, "attack_trees", "Attack trees", Some(5)),
        row(4, "attack_tree", "Relay attack on key fob", None),
    ]
}

/// The `Project` panel.
pub struct ProjectTree {
    focus_handle: FocusHandle,
    rows: Vec<OutlineRow>,
    selection: Entity<Selection>,
}

impl ProjectTree {
    pub fn new(selection: Entity<Selection>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            rows: sample_outline(),
            selection,
        }
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(ix) else { return };
        let (kind, label) = (row.kind, row.label.clone());
        let detail = row.count.map(|n| format!("{n} items")).unwrap_or_default();
        self.selection.update(cx, |selection, cx| {
            *selection = Selection {
                kind,
                label,
                detail,
            };
            cx.notify();
        });
    }
}

dock_panel!(ProjectTree, "ProjectTree", "Project");

impl Render for ProjectTree {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().accent;
        let selected = self.selection.read(cx).label.clone();
        let accent = cx.theme().secondary;
        let rows = self.rows.iter().enumerate().map(|(ix, row)| {
            let count = row.count.map(|n| {
                div()
                    .text_color(muted)
                    .child(SharedString::from(n.to_string()))
            });
            h_flex()
                .id(("outline-row", ix))
                .w_full()
                .gap_2()
                .py_0p5()
                .pr_2()
                .pl(px(f32::from(row.depth) * 16. + 8.))
                .when(row.label == selected, |this| this.bg(accent))
                .hover(|style| style.bg(hover))
                .cursor_pointer()
                .child(div().flex_1().child(SharedString::from(row.label.clone())))
                .children(count)
                .on_click(cx.listener(move |this, _, _, cx| this.select(ix, cx)))
        });
        v_flex().size_full().py_1().children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_outline_has_the_expected_rows() {
        let rows = sample_outline();
        let depths: Vec<u8> = rows.iter().map(|row| row.depth).collect();
        assert_eq!(depths, vec![0, 1, 2, 3, 3, 3, 3, 4]);
        assert_eq!(rows[0].kind, "workspace");
    }
}
