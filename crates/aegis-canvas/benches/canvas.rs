//! Canvas benchmarks against the standards budgets (section 2).
//!
//! | Budget | Bench |
//! |---|---|
//! | Frame time <= 16 ms while panning, `large` (100,000 nodes) | `frame` |
//! | Layout, full, O(n) | `layout` at 10k / 100k / 1M nodes |
//! | Hit test O(log n + k) | `hit_test` |
//!
//! Run: `cargo bench -p aegis-canvas`.

#[path = "../tests/common/mod.rs"]
mod common;

use aegis_canvas::layout::{Forest, layout};
use aegis_canvas::{LayoutConfig, NodeStyle, Point, RenderOptions, Scene, Size, Viewport, render};

fn main() {
    divan::main();
}

const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

/// A parent array of a tree with fan-out 4.
fn parents(n: usize) -> Vec<Option<u32>> {
    (0..n)
        .map(|i| {
            if i == 0 {
                None
            } else {
                Some(((i - 1) / 4) as u32)
            }
        })
        .collect()
}

/// Buchheim layout alone. The numbers should grow in proportion to n: that
/// is the point of the algorithm (Walker's original is O(n^2) worst case).
#[divan::bench(args = SIZES)]
fn layout_only(bencher: divan::Bencher, n: usize) {
    let forest = Forest::from_parents(&parents(n));
    bencher.bench_local(|| layout(&forest, 0, 1.0));
}

/// A deep chain: the shape that breaks recursive implementations.
#[divan::bench]
fn layout_chain_1m(bencher: divan::Bencher) {
    let parents: Vec<Option<u32>> = (0..1_000_000usize)
        .map(|i| i.checked_sub(1).map(|p| p as u32))
        .collect();
    let forest = Forest::from_parents(&parents);
    bencher.bench_local(|| layout(&forest, 0, 1.0));
}

/// Model tree to laid-out scene with indexes: what happens when the
/// structure changes.
#[divan::bench(args = [10_000, 100_000])]
fn scene_build(bencher: divan::Bencher, n: usize) {
    let tree = common::wide(n, 4);
    bencher
        .bench_local(|| Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default()));
}

/// One frame at 1:1 on a 1920x1080 window over a 100,000 node tree. Budget: 16 ms.
#[divan::bench]
fn frame(bencher: divan::Bencher) {
    let tree = common::wide(100_000, 4);
    let scene = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
    let mut view = Viewport::new(Size::new(1920.0, 1080.0));
    view.origin = Point::new(
        scene.bounds().size.w / 2.0 - 960.0,
        scene.bounds().size.h / 2.0 - 540.0,
    );
    bencher.bench_local(|| render(&scene, &view, &RenderOptions::default()));
}

/// The same window at the smallest zoom: tens of thousands of nodes are
/// visible and each is one rectangle (the worst frame).
#[divan::bench]
fn frame_zoomed_out(bencher: divan::Bencher) {
    let tree = common::wide(100_000, 4);
    let scene = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
    let mut view = Viewport::new(Size::new(1920.0, 1080.0));
    view.zoom = aegis_canvas::viewport::MIN_ZOOM;
    view.origin = Point::new(0.0, scene.bounds().size.h / 2.0);
    bencher.bench_local(|| render(&scene, &view, &RenderOptions::default()));
}

/// Mouse move over a 100,000 node tree.
#[divan::bench]
fn hit_test(bencher: divan::Bencher) {
    let tree = common::wide(100_000, 4);
    let scene = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
    let c = scene.bounds().center();
    bencher.bench_local(|| scene.hit_test(Point::new(c.x, c.y)));
}
