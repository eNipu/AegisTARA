//! The `Workbench` view: top bar, dock area and status bar over the sample.

use std::rc::Rc;

use aegis_client::explorer::{ExplorerIndex, ExplorerState};
use aegis_client::i18n::Catalog;
use aegis_client::settings::Appearance;
use aegis_client::workspace::Workspace;
use gpui::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, Window, div, px,
};
use gpui_component::ActiveTheme as _;
use gpui_component::dock::{DockArea, DockLayout, DockPlacement, DockSkin, panel_handle};

use super::apply_appearance;
use super::panels::{BottomPanel, ExplorerPanel, InspectorPanel, OverviewTab};
use crate::sample::{SAMPLE_NAME, workbench_sample};

/// The window's root view.
pub struct Workbench {
    workspace: Workspace,
    index: ExplorerIndex,
    state: ExplorerState,
    explorer: Entity<ExplorerPanel>,
    area: Entity<DockArea>,
    status: SharedString,
    _skin: Rc<DockSkin>,
    _appearance: Subscription,
}

impl Workbench {
    /// The workbench over the sample project, with texts from `catalog`.
    pub fn new(
        catalog: &Catalog,
        appearance: Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = workbench_sample();
        let project = workspace.project();
        let names = project.methodology().element_names().clone();
        let index = ExplorerIndex::build(&workspace, &names, catalog, SAMPLE_NAME);
        let state = ExplorerState::default();
        let tr = |id: &str| SharedString::from(catalog.tr(id));

        let counts = [
            (&names.asset, project.asset_count()),
            (&names.threat_scenario, project.threat_scenario_count()),
            (&names.attack_tree, project.attack_tree_count()),
            (&names.security_control, project.security_control_count()),
        ]
        .map(|(n, count)| SharedString::from(format!("{}: {count}", n.plural)));
        let heading = Some(SharedString::from(SAMPLE_NAME));
        let overview =
            cx.new(|cx| OverviewTab::new(tr("workbench-overview"), heading, counts.into(), cx));
        let explorer_title = tr("workbench-explorer");
        let explorer = cx.new(|cx| ExplorerPanel::new(explorer_title, index.rows(&state), cx));
        let inspector_text = vec![tr("workbench-inspector-empty")];
        let inspector =
            cx.new(|cx| InspectorPanel::new(tr("workbench-inspector"), None, inspector_text, cx));
        let bottom_text = vec![tr("workbench-bottom-empty")];
        let bottom = cx.new(|cx| BottomPanel::new(tr("workbench-bottom"), None, bottom_text, cx));

        let (area, skin) = DockSkin::dock_area("workbench", None, window, cx);
        let center = DockLayout::tabs().panel_view(panel_handle(overview), cx);
        let left = DockLayout::tabs().panel_view(panel_handle(explorer.clone()), cx);
        let right = DockLayout::tabs().panel_view(panel_handle(inspector), cx);
        let below = DockLayout::tabs().panel_view(panel_handle(bottom), cx);
        area.update(cx, |area, cx| {
            area.set_center(center, window, cx);
            area.set_dock(DockPlacement::Left, left, window, cx);
            area.set_dock(DockPlacement::Right, right, window, cx);
            area.set_dock(DockPlacement::Bottom, below, window, cx);
            area.set_dock_size(DockPlacement::Left, px(280.), window, cx);
            area.set_dock_size(DockPlacement::Right, px(320.), window, cx);
            area.set_dock_size(DockPlacement::Bottom, px(200.), window, cx);
        });

        let status = format!(
            "{}  ·  {}",
            project.methodology().label(),
            catalog.tr("workbench-status-unsaved")
        )
        .into();
        let follow = cx.observe_window_appearance(window, move |_, window, cx| {
            if appearance == Appearance::System {
                apply_appearance(appearance, window, cx);
            }
        });
        Self {
            workspace,
            index,
            state,
            explorer,
            area,
            status,
            _skin: skin,
            _appearance: follow,
        }
    }

    /// The number of rows the explorer dock shows.
    pub fn explorer_row_count(&self, cx: &App) -> usize {
        self.explorer.read(cx).rows().len()
    }

    /// The workspace shown.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// The explorer tree of the workspace.
    pub fn index(&self) -> &ExplorerIndex {
        &self.index
    }

    /// What is open and selected in the explorer.
    pub fn state(&self) -> &ExplorerState {
        &self.state
    }
}

impl Render for Workbench {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bar = || {
            div()
                .flex()
                .items_center()
                .px_3()
                .bg(theme.title_bar)
                .border_color(theme.border)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(bar().h(px(36.)).border_b_1().child(SAMPLE_NAME))
            .child(div().flex_1().min_h_0().child(self.area.clone()))
            .child(
                bar()
                    .h(px(24.))
                    .border_t_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(self.status.clone()),
            )
    }
}
