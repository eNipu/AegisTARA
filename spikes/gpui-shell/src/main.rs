//! Throwaway spike (job 181): one GPUI window with gpui-component's dock, a
//! project tree, an inspector and a text editor tab. Never used by production
//! crates (decision D8).

mod editor_tab;
mod inspector;
mod project_tree;
mod selection;
mod shell;

use gpui::{AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

fn main() {
    gpui_platform::application().run(|cx| {
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
            let shell = cx.new(|cx| shell::Shell::new(window, cx));
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
