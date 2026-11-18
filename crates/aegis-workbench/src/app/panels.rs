//! The dock panels of the skeleton. They only show what `aegis-client` and
//! the project already computed.

use aegis_client::explorer::ExplorerRow;
use gpui::{
    Context, FocusHandle, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Window, div, px,
};
use gpui_component::ActiveTheme as _;

/// Implements the dock panel traits for a view with a `focus_handle` and a
/// `title` field.
macro_rules! dock_panel {
    ($ty:ty, $name:expr) => {
        impl gpui_component::dock::BasePanel for $ty {
            fn panel_name(&self) -> &'static str {
                $name
            }
        }
        impl gpui_component::dock::Panel for $ty {
            fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                self.title.clone()
            }
        }
        impl gpui::EventEmitter<gpui_component::dock::PanelEvent> for $ty {}
        impl gpui::Focusable for $ty {
            fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
                self.focus_handle.clone()
            }
        }
    };
}

/// The explorer dock: the visible rows, indented by depth, with counts.
/// Slice 182d replaces it with the interactive tree.
pub struct ExplorerPanel {
    focus_handle: FocusHandle,
    title: SharedString,
    rows: Vec<ExplorerRow>,
}

impl ExplorerPanel {
    pub fn new(title: SharedString, rows: Vec<ExplorerRow>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        Self {
            focus_handle,
            title,
            rows,
        }
    }

    /// The rows shown.
    pub fn rows(&self) -> &[ExplorerRow] {
        &self.rows
    }
}

dock_panel!(ExplorerPanel, "ExplorerPanel");

impl Render for ExplorerPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let rows = self.rows.iter().map(|r| {
            let count = r.count.map(|n| n.to_string()).unwrap_or_default();
            div()
                .flex()
                .gap_2()
                .pl(px(8. + 14. * f32::from(r.depth)))
                .child(r.label.clone())
                .child(div().text_color(muted).child(count))
        });
        div()
            .size_full()
            .py_1()
            .text_sm()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .children(rows)
    }
}

/// Defines a panel of a few lines of text: the Overview tab, the inspector
/// and the bottom dock of the skeleton.
macro_rules! text_panel {
    ($(#[$doc:meta])* $ty:ident) => {
        $(#[$doc])*
        pub struct $ty {
            focus_handle: FocusHandle,
            title: SharedString,
            heading: Option<SharedString>,
            lines: Vec<SharedString>,
        }

        impl $ty {
            pub fn new(
                title: SharedString,
                heading: Option<SharedString>,
                lines: Vec<SharedString>,
                cx: &mut Context<Self>,
            ) -> Self {
                let focus_handle = cx.focus_handle();
                Self { focus_handle, title, heading, lines }
            }
        }

        dock_panel!($ty, stringify!($ty));

        impl Render for $ty {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                text_body(self.heading.clone(), &self.lines, cx)
            }
        }
    };
}

text_panel!(
    /// The Overview tab: the project name and its element counts.
    OverviewTab
);
text_panel!(
    /// The inspector dock; empty until something is selected.
    InspectorPanel
);
text_panel!(
    /// The bottom dock; problems and changes arrive in later jobs.
    BottomPanel
);

fn text_body(heading: Option<SharedString>, lines: &[SharedString], cx: &gpui::App) -> gpui::Div {
    let theme = cx.theme();
    let heading = heading.map(|h| div().text_lg().text_color(theme.foreground).child(h));
    div()
        .size_full()
        .p_3()
        .flex()
        .flex_col()
        .gap_1()
        .text_sm()
        .text_color(theme.muted_foreground)
        .children(heading)
        .children(lines.iter().map(|l| div().child(l.clone())))
}
