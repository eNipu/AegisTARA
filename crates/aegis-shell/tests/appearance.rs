//! Appearance modes (UI-07.5) and the headless renderer (UI-07.9).

use aegis_client::settings::{Appearance, Settings};
use aegis_shell::AppWindow;
use aegis_shell::headless::{Headless, RENDERER};
use aegis_shell::renderer::{self, Renderer};
use slint::ComponentHandle as _;

const SIZE: (u32, u32) = (900, 700);

/// The brightness of a pixel near the bottom-left corner, which is plain
/// window background on the server screen.
fn corner_brightness(headless: &Headless) -> u32 {
    let frame = headless.render();
    let [r, g, b] = frame.pixel(4, frame.height - 4);
    u32::from(r) + u32::from(g) + u32::from(b)
}

#[test]
fn light_and_dark_repaint_live_and_system_hands_back_to_the_os() {
    let headless = Headless::install(SIZE.0, SIZE.1);
    let window = AppWindow::new().expect("window");
    window
        .window()
        .set_size(slint::PhysicalSize::new(SIZE.0, SIZE.1));
    window.show().expect("show");

    assert_eq!(window.get_appearance(), 0, "the default is System");

    window.set_appearance(2);
    let dark = corner_brightness(&headless);
    window.set_appearance(1);
    let light = corner_brightness(&headless);
    assert!(
        dark < 150 && light > 600,
        "dark {dark}, light {light}: the same window must repaint"
    );

    // System: the headless platform has no OS appearance, so only check that
    // the window stays drawable and picks one of the two palettes.
    window.set_appearance(0);
    let system = corner_brightness(&headless);
    assert!(!(150..=600).contains(&system), "system {system}");
}

#[test]
fn the_appearance_round_trips_through_the_settings() {
    for a in [Appearance::System, Appearance::Light, Appearance::Dark] {
        let s = Settings {
            appearance: a.code().into(),
            ..Settings::default()
        };
        assert_eq!(Appearance::from_index(s.appearance().index()), a);
    }
}

#[test]
fn headless_windows_stay_on_the_software_renderer() {
    // The environment may ask for the GPU renderer; the test platform never
    // reads it and still draws (see `Headless::install`).
    assert_eq!(renderer::parse(Some("skia")).renderer, Renderer::Skia);
    assert_eq!(RENDERER, Renderer::Software);
    assert_eq!(RENDERER.name(), "software");
    let headless = Headless::install(SIZE.0, SIZE.1);
    let window = AppWindow::new().expect("window");
    window.show().expect("show");
    let frame = headless.render();
    assert_eq!((frame.width, frame.height), SIZE);
}
