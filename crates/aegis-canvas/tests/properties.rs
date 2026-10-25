//! Property tests: the engine's invariants on random trees.

mod common;

use aegis_canvas::{
    Direction, LayoutConfig, NodeStyle, Point, Rect, RenderOptions, Scene, Size, Viewport, render,
};
use aegis_model::NodeIdx;
use common::idx;
use proptest::prelude::*;

type Spec = (Vec<(u32, bool)>, Vec<(u32, u32)>);

fn spec() -> impl Strategy<Value = Spec> {
    (
        prop::collection::vec((any::<u32>(), prop::bool::weighted(0.4)), 0..120),
        prop::collection::vec((any::<u32>(), any::<u32>()), 0..10),
    )
}

fn scene_of(spec: &[(u32, bool)], links: &[(u32, u32)]) -> (aegis_model::AttackTree, Scene) {
    let tree = common::build(spec, links);
    let scene = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
    (tree, scene)
}

fn rect(scene: &Scene, i: usize) -> Rect {
    scene.node_rect(idx(i)).expect("rect")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// No two node boxes overlap: the first thing a reader would notice.
    #[test]
    fn nodes_never_overlap((spec, links) in spec()) {
        let (_, scene) = scene_of(&spec, &links);
        for a in 0..scene.len() {
            for b in (a + 1)..scene.len() {
                let (ra, rb) = (rect(&scene, a), rect(&scene, b));
                let overlap = ra.origin.x < rb.right() && rb.origin.x < ra.right()
                    && ra.origin.y < rb.bottom() && rb.origin.y < ra.bottom();
                prop_assert!(!overlap, "{a} overlaps {b}");
            }
        }
    }

    /// Children sit to the right of their owner, and an owner with children
    /// is centred over the first and last of them.
    #[test]
    fn owners_are_left_of_and_centred_over_their_children((spec, links) in spec()) {
        let (tree, scene) = scene_of(&spec, &links);
        for (idx, _, _) in tree.iter() {
            let owned: Vec<NodeIdx> = tree
                .children(idx)
                .iter()
                .copied()
                .filter(|c| tree.owner(*c) == Some(idx))
                .collect();
            let Some((first, last)) = owned.first().zip(owned.last()) else { continue };
            let here = rect(&scene, idx.index());
            for c in &owned {
                prop_assert!(rect(&scene, c.index()).origin.x > here.right());
            }
            let mid = (rect(&scene, first.index()).center().y + rect(&scene, last.index()).center().y) / 2.0;
            prop_assert!((here.center().y - mid).abs() < 0.01, "node {} not centred", idx.index());
        }
    }

    /// The same tree always gives the same picture (P4).
    #[test]
    fn layout_and_rendering_are_deterministic((spec, links) in spec()) {
        let (tree, a) = scene_of(&spec, &links);
        let b = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
        let view = Viewport::new(Size::new(900.0, 600.0));
        prop_assert_eq!(
            render(&a, &view, &RenderOptions::default()),
            render(&b, &view, &RenderOptions::default())
        );
    }

    /// The index agrees with a linear scan: hit testing and culling.
    #[test]
    fn index_queries_match_a_linear_scan(
        (spec, links) in spec(),
        x in -200.0f32..2000.0, y in -200.0f32..1500.0, w in 1.0f32..900.0, h in 1.0f32..700.0,
    ) {
        let (_, scene) = scene_of(&spec, &links);
        let area = Rect::new(x, y, w, h);
        let expect: Vec<u32> = (0..scene.len() as u32)
            .filter(|i| rect(&scene, *i as usize).intersects(&area))
            .collect();
        prop_assert_eq!(scene.nodes_in(&area), expect);

        let p = Point::new(x, y);
        let hit = scene.hit_test(p).map(|i| i.index());
        let scan = (0..scene.len()).find(|i| rect(&scene, *i).contains(p));
        prop_assert_eq!(hit, scan);
    }

    /// Culling is lossless: a frame drawn from the visible part equals the
    /// same frame drawn from everything, restricted to what shows.
    #[test]
    fn rendering_draws_exactly_the_visible_nodes(
        (spec, links) in spec(),
        ox in -300.0f32..1500.0, oy in -300.0f32..900.0, zoom in 0.25f32..2.0,
    ) {
        let (_, scene) = scene_of(&spec, &links);
        let mut view = Viewport::new(Size::new(700.0, 500.0));
        view.origin = Point::new(ox, oy);
        view.zoom = zoom;
        let list = render(&scene, &view, &RenderOptions::default());
        let drawn: std::collections::BTreeSet<u32> = list
            .items
            .iter()
            .filter_map(|i| match i {
                aegis_canvas::Item::Rect { node: Some(n), .. } => Some(*n),
                _ => None,
            })
            .collect();
        let world = view.world_rect().inflate(4.0 / zoom);
        let expect: std::collections::BTreeSet<u32> = (0..scene.len() as u32)
            .filter(|i| rect(&scene, *i as usize).intersects(&world))
            .collect();
        prop_assert_eq!(drawn, expect);
    }

    /// Restyling a node changes its look and nothing about the geometry.
    #[test]
    fn restyling_does_not_move_anything((spec, links) in spec(), pick in any::<usize>()) {
        let (_, mut scene) = scene_of(&spec, &links);
        let before: Vec<Rect> = (0..scene.len()).map(|i| rect(&scene, i)).collect();
        let node = idx(pick % scene.len());
        scene.restyle(node, NodeStyle { score: Some("99".into()), ..NodeStyle::default() });
        let after: Vec<Rect> = (0..scene.len()).map(|i| rect(&scene, i)).collect();
        prop_assert_eq!(before, after);
        prop_assert_eq!(scene.style(node.index() as u32).and_then(|s| s.score.clone()), Some("99".to_string()));
    }

    /// Keyboard navigation always lands on a real, different node, and left
    /// undoes right.
    #[test]
    fn navigation_is_consistent((spec, links) in spec()) {
        let (_, scene) = scene_of(&spec, &links);
        for i in 0..scene.len() {
            let from = idx(i);
            for dir in [Direction::Left, Direction::Right, Direction::Up, Direction::Down] {
                if let Some(to) = scene.navigate(from, dir) {
                    prop_assert_ne!(to, from);
                    prop_assert!(to.index() < scene.len());
                }
            }
            if let Some(child) = scene.navigate(from, Direction::Right) {
                prop_assert_eq!(scene.navigate(child, Direction::Left), Some(from));
            }
            if let Some(below) = scene.navigate(from, Direction::Down) {
                prop_assert_eq!(scene.navigate(below, Direction::Up), Some(from));
            }
        }
    }
}
