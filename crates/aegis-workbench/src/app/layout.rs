//! Which docks are open and how big they are, read from and written to the
//! dock area (remembered per workspace in the settings file).

use aegis_client::settings::WorkbenchLayout;
use gpui::{Context, Window, px};
use gpui_component::dock::{DockArea, DockPlacement};

/// The layout of a workspace seen for the first time.
#[must_use]
pub fn default_layout(key: &str) -> WorkbenchLayout {
    WorkbenchLayout {
        key: key.to_owned(),
        left_open: true,
        right_open: true,
        bottom_open: true,
        left_width: 280,
        right_width: 320,
        bottom_height: 200,
    }
}

/// The docks as they are now.
#[must_use]
pub fn read_layout(area: &DockArea, key: &str) -> WorkbenchLayout {
    let size = |p| {
        let s = area.dock_size(p).map_or(0.0, f32::from).round();
        // In range: the dock clamps its size, and the settings clamp again.
        s.clamp(0.0, 10_000.0) as u32
    };
    WorkbenchLayout {
        key: key.to_owned(),
        left_open: area.is_dock_open(DockPlacement::Left),
        right_open: area.is_dock_open(DockPlacement::Right),
        bottom_open: area.is_dock_open(DockPlacement::Bottom),
        left_width: size(DockPlacement::Left),
        right_width: size(DockPlacement::Right),
        bottom_height: size(DockPlacement::Bottom),
    }
}

/// Sizes, opens and closes the docks as `layout` says.
pub fn apply_layout(
    area: &mut DockArea,
    layout: &WorkbenchLayout,
    window: &mut Window,
    cx: &mut Context<DockArea>,
) {
    let docks = [
        (DockPlacement::Left, layout.left_open, layout.left_width),
        (DockPlacement::Right, layout.right_open, layout.right_width),
        (
            DockPlacement::Bottom,
            layout.bottom_open,
            layout.bottom_height,
        ),
    ];
    for (placement, open, size) in docks {
        // Sizes are at most 800 (clamped by the settings), exact in f32.
        area.set_dock_size(placement, px(size as f32), window, cx);
        if area.is_dock_open(placement) != open {
            area.toggle_dock(placement, window, cx);
        }
    }
}
