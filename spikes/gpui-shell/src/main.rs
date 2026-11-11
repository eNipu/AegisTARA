//! Throwaway GPUI shell spike (job 181); never used by production crates (D8).

/// Implements the dock panel traits for a view that has a `focus_handle` field.
macro_rules! dock_panel {
    ($ty:ty, $name:literal, $title:literal) => {
        impl gpui_component::dock::BasePanel for $ty {
            fn panel_name(&self) -> &'static str {
                $name
            }
        }
        impl gpui_component::dock::Panel for $ty {
            fn title(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl gpui::IntoElement {
                $title
            }
        }
        impl gpui::EventEmitter<gpui_component::dock::PanelEvent> for $ty {}
        impl gpui::Focusable for $ty {
            fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
                self.focus_handle.clone()
            }
        }
    };
}

mod args;
mod canvas_tab;
mod editor_tab;
mod inspector;
mod measure;
mod palette;
mod project_tree;
mod sample_tree;
mod selection;
mod shell;

use gpui::{AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

fn main() {
    let started = std::time::Instant::now();
    let args = match args::parse_args(&std::env::args().collect::<Vec<_>>()) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let tree = match sample_tree::wide_tree(args.nodes, 4) {
        Ok(tree) => tree,
        Err(err) => {
            eprintln!("could not build the sample tree: {err}");
            std::process::exit(2);
        }
    };
    gpui_platform::application().run(move |cx| {
        gpui_component::init(cx);
        let bounds = Bounds::centered(None, size(px(1400.), px(900.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("AegisTARA spike - GPUI shell".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            let shell = cx.new(|cx| shell::Shell::new(&tree, started, &args, window, cx));
            cx.new(|cx| gpui_component::Root::new(shell, window, cx))
        });
        if let Err(err) = opened {
            eprintln!("could not open the window: {err:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
