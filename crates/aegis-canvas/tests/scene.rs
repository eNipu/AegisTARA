//! Scene, rendering, SVG, and keyboard behaviour on fixed trees.

mod common;

use aegis_canvas::{
    Direction, Item, LayoutConfig, NodeStyle, Paint, Point, RenderOptions, Scene, Size, SvgOptions,
    SvgTheme, Tone, Viewport, render, scene_to_svg, to_svg,
};

use common::idx;

fn styled(tree: &aegis_model::AttackTree) -> Scene {
    Scene::from_tree(tree, LayoutConfig::default(), |i| NodeStyle {
        score: Some(format!("{} · Medium", 10 + i.index())),
        tone: Tone::from_level(i.index() % 5, 5),
        critical: false,
        ghost: false,
        via: None,
    })
}

#[test]
fn layout_matches_the_design_sizes() {
    let tree = common::sample();
    let scene = styled(&tree);
    let root = scene.node_rect(idx(0)).expect("root");
    assert_eq!((root.size.w, root.size.h), (210.0, 60.0));
    // Columns are one node width plus the gap apart.
    let child = scene.node_rect(idx(1)).expect("child");
    assert_eq!(child.origin.x - root.origin.x, 210.0 + 56.0);
    // Everything starts at the top-left of the world.
    assert_eq!(scene.bounds().origin.y, 0.0);
    assert_eq!(scene.bounds().origin.x, 0.0);
}

#[test]
fn shared_nodes_get_a_dashed_second_edge() {
    let tree = common::sample();
    let scene = styled(&tree);
    let links: Vec<_> = scene.edges().iter().filter(|e| e.link).collect();
    assert_eq!(links.len(), 1, "one shared subtree");
    let view = Viewport::new(Size::new(1600.0, 700.0));
    let list = render(&scene, &view, &RenderOptions::default());
    let dashed = list
        .items
        .iter()
        .filter(|i| matches!(i, Item::Curve { stroke, .. } if stroke.dashed))
        .count();
    assert_eq!(dashed, 1);
}

#[test]
fn hit_testing_finds_the_node_under_the_cursor_in_world_space() {
    let tree = common::sample();
    let scene = styled(&tree);
    for i in 0..scene.len() {
        let r = scene.node_rect(idx(i)).expect("rect");
        assert_eq!(scene.hit_test(r.center()), Some(idx(i)));
    }
    assert_eq!(scene.hit_test(Point::new(-50.0, -50.0)), None);
    // The gap between columns is empty.
    let root = scene.node_rect(idx(0)).expect("root");
    assert_eq!(
        scene.hit_test(Point::new(root.right() + 10.0, root.center().y)),
        None
    );
}

#[test]
fn the_display_list_hit_tests_like_the_scene() {
    let tree = common::sample();
    let scene = styled(&tree);
    let mut view = Viewport::new(Size::new(1600.0, 700.0));
    view.zoom = 0.8;
    view.origin = Point::new(-20.0, -10.0);
    let list = render(&scene, &view, &RenderOptions::default());
    for i in 0..scene.len() {
        let world = scene.node_rect(idx(i)).expect("rect").center();
        let screen = view.to_screen(world);
        assert_eq!(list.node_at(screen), Some(i as u32), "node {i}");
    }
}

#[test]
fn level_of_detail_drops_text_then_detail() {
    let tree = common::sample();
    let scene = styled(&tree);
    let count = |zoom: f32| {
        let mut view = Viewport::new(Size::new(3000.0, 1500.0));
        view.zoom = zoom;
        let list = render(&scene, &view, &RenderOptions::default());
        let texts = list
            .items
            .iter()
            .filter(|i| matches!(i, Item::Text { .. }))
            .count();
        let rects = list
            .items
            .iter()
            .filter(|i| matches!(i, Item::Rect { .. }))
            .count();
        (texts, rects)
    };
    let (t1, r1) = count(1.0);
    let (t2, r2) = count(0.3);
    let (t3, r3) = count(0.1);
    assert!(t1 > 0 && r1 == 2 * scene.len());
    assert_eq!((t2, r2), (0, 2 * scene.len()), "no text, boxes and bars");
    assert_eq!((t3, r3), (0, scene.len()), "one rectangle per node");
}

#[test]
fn selection_and_critical_path_are_drawn() {
    let tree = common::sample();
    let mut scene = styled(&tree);
    let view = Viewport::new(Size::new(1600.0, 700.0));
    let plain = render(&scene, &view, &RenderOptions::default());
    assert!(
        !plain
            .items
            .iter()
            .any(|i| matches!(i, Item::Polyline { .. }))
    );

    let selected = render(
        &scene,
        &view,
        &RenderOptions {
            selected: Some(idx(2)),
        },
    );
    assert_eq!(
        selected
            .items
            .iter()
            .filter(|i| matches!(i, Item::Polyline { .. }))
            .count(),
        4,
        "four corner ticks"
    );
    assert!(
        selected
            .items
            .iter()
            .any(|i| matches!(i, Item::Rect { stroke: Some(s), .. } if s.paint == Paint::Accent))
    );

    scene.set_critical_path(&[idx(0), idx(2), idx(6)]);
    let crit = render(&scene, &view, &RenderOptions::default());
    let thick = crit
        .items
        .iter()
        .filter(|i| matches!(i, Item::Curve { stroke, .. } if stroke.paint == Paint::Ink))
        .count();
    assert_eq!(thick, 2, "the two edges of the path");
    // Highlighted edges are drawn after ordinary ones, so they sit on top.
    let first_ink = crit
        .items
        .iter()
        .position(|i| matches!(i, Item::Curve { stroke, .. } if stroke.paint == Paint::Ink))
        .expect("ink edge");
    let last_plain = crit
        .items
        .iter()
        .rposition(|i| matches!(i, Item::Curve { stroke, .. } if stroke.paint == Paint::Graphite))
        .expect("plain edge");
    assert!(first_ink > last_plain);
    scene.set_critical_path(&[]);
    assert_eq!(render(&scene, &view, &RenderOptions::default()), plain);
}

#[test]
fn keyboard_navigation_walks_the_tree() {
    let tree = common::sample();
    let scene = styled(&tree);
    let root = idx(0);
    // Right goes to the child nearest in height; left goes back.
    let child = scene.navigate(root, Direction::Right).expect("child");
    assert_eq!(scene.navigate(child, Direction::Left), Some(root));
    assert_eq!(
        scene.navigate(root, Direction::Left),
        None,
        "root has no owner"
    );
    assert_eq!(scene.navigate(root, Direction::Up), None);
    // Up and down move within the column, in order of height.
    let mut at = scene.navigate(root, Direction::Right).expect("first");
    let mut column = vec![at];
    while let Some(next) = scene.navigate(at, Direction::Down) {
        column.push(next);
        at = next;
    }
    assert!(column.len() >= 2);
    let ys: Vec<f32> = column
        .iter()
        .map(|c| scene.node_rect(*c).expect("rect").origin.y)
        .collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn a_single_node_tree_works_everywhere() {
    let tree = aegis_model::AttackTree::new(
        aegis_model::NodeId::new_v7(),
        aegis_model::Node::step("Only step"),
    )
    .expect("tree");
    let scene = styled(&tree);
    assert_eq!(scene.len(), 1);
    let svg = scene_to_svg(&scene, &SvgOptions::light(), &RenderOptions::default());
    assert!(svg.contains("Only step"));
    assert_eq!(scene.navigate(idx(0), Direction::Down), None);
}

#[test]
fn svg_is_well_formed_escaped_and_deterministic() {
    let tree = common::sample();
    let scene = styled(&tree);
    let a = scene_to_svg(&scene, &SvgOptions::light(), &RenderOptions::default());
    let b = scene_to_svg(&scene, &SvgOptions::light(), &RenderOptions::default());
    assert_eq!(a, b);
    // The label with markup characters is escaped, never raw.
    assert!(a.contains("&lt;antenna&gt; &amp;"));
    assert!(!a.contains("<antenna>"));
    assert!(a.starts_with("<svg ") && a.trim_end().ends_with("</svg>"));
    // No NaN or infinity, no negative zero.
    assert!(
        !a.contains("NaN") && !a.contains("inf") && !a.contains("-0 ") && !a.contains("\"-0\"")
    );
    // Dark theme only changes colors.
    let dark = scene_to_svg(
        &scene,
        &SvgOptions {
            theme: SvgTheme::Dark,
            ..SvgOptions::light()
        },
        &RenderOptions::default(),
    );
    assert_ne!(a, dark);
    assert_eq!(a.matches("<rect").count(), dark.matches("<rect").count());
}

#[test]
fn svg_scales_to_the_requested_width_through_the_view_box() {
    let tree = common::sample();
    let scene = styled(&tree);
    let natural = scene_to_svg(&scene, &SvgOptions::light(), &RenderOptions::default());
    let narrow = scene_to_svg(
        &scene,
        &SvgOptions {
            display_width: Some(400.0),
            ..SvgOptions::light()
        },
        &RenderOptions::default(),
    );
    let view_box = |s: &str| {
        s.split("viewBox=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .map(str::to_owned)
    };
    assert_eq!(view_box(&natural), view_box(&narrow), "same coordinates");
    assert!(narrow.contains("width=\"400\""));
}

#[test]
fn snapshot_of_the_sample_tree_as_svg() {
    let tree = common::sample();
    let mut scene = styled(&tree);
    scene.set_critical_path(&[idx(0), idx(2), idx(6)]);
    let svg = scene_to_svg(
        &scene,
        &SvgOptions::light(),
        &RenderOptions {
            selected: Some(idx(3)),
        },
    );
    insta::assert_snapshot!("sample_tree_light", svg);
}

#[test]
fn snapshot_of_the_display_list_of_a_zoomed_out_frame() {
    let tree = common::sample();
    let scene = styled(&tree);
    let mut view = Viewport::new(Size::new(480.0, 260.0));
    view.zoom = 0.3;
    view.origin = Point::new(-30.0, -20.0);
    let list = render(&scene, &view, &RenderOptions::default());
    let svg = to_svg(
        &list,
        view.size,
        &SvgOptions {
            theme: SvgTheme::Dark,
            ..SvgOptions::light()
        },
    );
    insta::assert_snapshot!("sample_tree_dark_zoomed_out", svg);
}

#[test]
fn ghost_nodes_render_with_dashed_accent_edges_and_boxes() {
    let tree = common::sample();
    let scene = Scene::from_tree(&tree, LayoutConfig::default(), |i| NodeStyle {
        score: Some("AI · High".into()),
        tone: Tone::High,
        critical: false,
        ghost: i.index() == 3,
        via: None,
    });
    let view = Viewport::new(Size::new(1600.0, 700.0));
    let list = render(&scene, &view, &RenderOptions::default());

    let ghost_rect = list.items.iter().find(|it| {
        matches!(
            it,
            Item::Rect {
                node: Some(3),
                fill: Some(Paint::AccentWash),
                stroke: Some(s),
                ..
            } if s.dashed && s.paint == Paint::Accent
        )
    });
    assert!(
        ghost_rect.is_some(),
        "ghost node must use AccentWash fill and dashed Accent border"
    );

    let ghost_kind = list.items.iter().any(|it| {
        matches!(
            it,
            Item::Text { text, paint: Paint::Accent, bold: true, .. }
            if text == "ghost · step"
        )
    });
    assert!(ghost_kind, "ghost node must display 'ghost · step' label");

    let svg = scene_to_svg(&scene, &SvgOptions::light(), &RenderOptions::default());
    assert!(svg.contains("ghost · step"));
}

#[test]
fn a_via_chip_is_drawn_on_a_step_and_named_for_screen_readers() {
    let tree = aegis_model::AttackTree::new(
        aegis_model::NodeId::new_v7(),
        aegis_model::Node::step("Flash firmware"),
    )
    .expect("tree");
    let scene = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle {
        score: Some("High".into()),
        via: Some("ADAS ECU / JTAG".into()),
        ..NodeStyle::default()
    });
    let view = Viewport::new(Size::new(800.0, 400.0));
    let list = render(&scene, &view, &RenderOptions::default());
    let text = format!("{list:?}");
    assert!(text.contains("\u{27f6} ADAS ECU"), "{text}");
    let a = aegis_canvas::access::describe(&scene, 0).expect("node");
    assert_eq!(a.via.as_deref(), Some("ADAS ECU / JTAG"));
}
