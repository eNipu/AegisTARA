//! The dock panels of the skeleton. They only show what `aegis-client` and
//! the project already computed.

use aegis_client::explorer::{ExplorerKey, ExplorerRow, Mark};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, Div, FocusHandle, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Render, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, WeakEntity, Window, div, px,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _};

use super::color;
use super::icons::icon;
use super::workbench::Workbench;
use crate::theme::{Tokens, dark, light};

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

/// The explorer dock: one line per [`ExplorerRow`]. Clicks and double clicks
/// go to the [`Workbench`], which owns the explorer state.
pub struct ExplorerPanel {
    focus_handle: FocusHandle,
    title: SharedString,
    rows: Vec<ExplorerRow>,
    selected: Option<ExplorerKey>,
    workbench: WeakEntity<Workbench>,
    light: Tokens,
    dark: Tokens,
}

impl ExplorerPanel {
    pub fn new(
        title: SharedString,
        workbench: WeakEntity<Workbench>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            title,
            rows: Vec::new(),
            selected: None,
            workbench,
            light: light(),
            dark: dark(),
        }
    }

    /// Shows `rows` with `selected` highlighted.
    pub fn set_rows(
        &mut self,
        rows: Vec<ExplorerRow>,
        selected: Option<ExplorerKey>,
        cx: &mut Context<Self>,
    ) {
        self.rows = rows;
        self.selected = selected;
        cx.notify();
    }

    /// The rows shown.
    pub fn rows(&self) -> &[ExplorerRow] {
        &self.rows
    }
}

dock_panel!(ExplorerPanel, "ExplorerPanel");

/// Runs `f` on the workbench, if it is still open.
fn on_workbench(
    wb: &WeakEntity<Workbench>,
    window: &mut Window,
    cx: &mut App,
    f: impl FnOnce(&mut Workbench, &mut Window, &mut Context<Workbench>),
) {
    let _ = wb.update(cx, |w, cx| f(w, window, cx));
}

/// One explorer line: chevron, icon, label, link mark, system square,
/// lint dot and count.
fn row(
    ix: usize,
    r: &ExplorerRow,
    selected: bool,
    t: &Tokens,
    wb: &WeakEntity<Workbench>,
) -> Stateful<Div> {
    let key = r.key;
    let ink = color(&t.ink);
    let grey = color(&t.line_strong);
    let chevron = div().w(px(14.)).flex_none().when(r.has_children, |d| {
        let wb = wb.clone();
        let name = if r.expanded {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        d.child(Icon::new(name).xsmall())
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                on_workbench(&wb, window, cx, |w, _, cx| w.toggle(key, cx));
            })
    });
    let square = match (key, r.system_slot) {
        (_, Some(slot)) => {
            let c = t
                .systems
                .get(usize::from(slot).saturating_sub(1))
                .map_or(grey, |h| color(h));
            Some(div().size(px(8.)).flex_none().bg(c))
        }
        (ExplorerKey::Unassigned, None) => {
            Some(div().size(px(8.)).flex_none().border_1().border_color(grey))
        }
        _ => None,
    };
    let dot = match r.mark {
        Mark::None => None,
        Mark::Warning => Some(color(&t.warning)),
        Mark::Error => Some(color(&t.error)),
    }
    .map(|c| div().size(px(6.)).flex_none().rounded_full().bg(c));
    let count = r.count.map(|n| {
        let c = if n > 0 { ink } else { grey };
        div().text_color(c).child(n.to_string())
    });
    let click = wb.clone();
    let label = SharedString::from(r.label.clone());
    let wash = color(&t.accent_wash);
    div()
        .id(ix)
        .flex()
        .items_center()
        .gap_1()
        .pl(px(4. + 14. * f32::from(r.depth)))
        .pr_2()
        .when(selected, |d| d.bg(wash))
        .child(chevron)
        .child(Icon::new(icon(r.icon)).small())
        .child(div().truncate().child(label.clone()))
        .when(r.linked, |d| d.child("\u{21e2}"))
        .children(square)
        .child(div().flex_1())
        .children(dot)
        .children(count)
        .on_click(move |ev, window, cx| {
            let open = ev.click_count() >= 2;
            on_workbench(&click, window, cx, |w, window, cx| {
                if open {
                    w.open(key, window, cx)
                } else {
                    w.select(key, cx)
                }
            });
        })
}

impl Render for ExplorerPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = if cx.theme().is_dark() {
            &self.dark
        } else {
            &self.light
        };
        let rows = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, r)| row(ix, r, self.selected == Some(r.key), t, &self.workbench));
        div()
            .id("explorer")
            .key_context("Explorer")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_y_scroll()
            .py_1()
            .text_sm()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|p, _, window, cx| window.focus(&p.focus_handle, cx)),
            )
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
    /// A folder's elements, or every problem, one line each (read-only).
    ListTab
);
text_panel!(
    /// One element: visual ID, name and kind (read-only).
    ElementTab
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
