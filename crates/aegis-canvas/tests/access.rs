//! What a screen reader is told about the nodes of an attack tree (job 051,
//! gap G14). The sample tree: node 0 "Steal vehicle" (OR) has the gates
//! 1 "Keyless entry relay attack" (sequential AND) and 2 "Reprogram key via
//! OBD-II port" (AND); 1 owns the steps 3 and 4 and also reaches the shared
//! step 5, which 2 owns together with the steps 6 and 7.

mod common;

use aegis_canvas::access::{describe, sentence, visible};
use aegis_canvas::{LayoutConfig, NodeKind, NodeStyle, Point, Scene, Size, Tone, Viewport};

fn scene() -> Scene {
    let tree = common::sample();
    Scene::from_tree(&tree, LayoutConfig::default(), |i| NodeStyle {
        score: (i.index() >= 3).then(|| format!("{} · Medium", 10 + i.index())),
        tone: Tone::Medium,
        critical: i.index() == 4,
    })
}

#[test]
fn the_root_is_a_gate_with_children_and_no_parent() {
    let s = scene();
    let a = describe(&s, 0).expect("root");
    assert_eq!(a.label, "Steal vehicle");
    assert!(matches!(a.kind, NodeKind::Gate(_)));
    assert_eq!((a.depth, a.children, a.parent), (0, 2, None));
    assert_eq!(
        sentence(&a, false),
        "Steal vehicle. OR gate with 2 children. Root of the tree."
    );
}

#[test]
fn a_step_says_where_it_sits_and_how_it_is_rated() {
    let s = scene();
    let a = describe(&s, 4).expect("relay step");
    assert_eq!((a.depth, a.position, a.siblings), (2, 2, 2));
    assert_eq!(
        a.parent_label.as_deref(),
        Some("Keyless entry relay attack")
    );
    assert_eq!(
        sentence(&a, false),
        "Relay to vehicle <antenna> & start. Step. Child 2 of 2 under Keyless entry relay attack. \
         Rating 14 · Medium. On the deciding path."
    );
}

#[test]
fn a_gate_in_the_middle_counts_the_shared_child_too() {
    let s = scene();
    let a = describe(&s, 1).expect("keyless");
    assert_eq!(a.children, 3, "two own steps and the shared one");
    assert_eq!(
        sentence(&a, false),
        "Keyless entry relay attack. Sequential AND gate with 3 children. \
         Child 1 of 2 under Steal vehicle."
    );
}

#[test]
fn a_shared_step_says_it_is_reached_from_another_gate() {
    let s = scene();
    let a = describe(&s, 5).expect("shared");
    assert_eq!(a.shared_from, 1);
    let text = sentence(&a, false);
    assert!(
        text.contains("Child 1 of 3 under Reprogram key via OBD-II port"),
        "{text}"
    );
    assert!(text.contains("Also reached from 1 other gate."), "{text}");
}

#[test]
fn a_one_child_gate_and_the_selection_are_worded_properly() {
    let s = scene();
    let a = describe(&s, 3).expect("step");
    let text = sentence(&a, true);
    assert!(text.ends_with("Selected."), "{text}");
    assert!(!sentence(&a, false).contains("Selected"));
    // Singular and plural.
    let tree = common::build(&[(0, false)], &[]);
    let one = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
    let root = describe(&one, 0).expect("root");
    assert!(
        sentence(&root, false).contains("with 1 child."),
        "{}",
        sentence(&root, false)
    );
}

#[test]
fn a_node_that_does_not_exist_has_no_description() {
    let s = scene();
    assert!(describe(&s, 99).is_none());
}

#[test]
fn only_the_nodes_on_screen_are_offered_with_their_screen_boxes() {
    let s = scene();
    // The whole tree fits: every node, in node order, at its screen place.
    let mut view = Viewport::new(Size::new(1600.0, 700.0));
    view.fit(s.bounds(), 10.0);
    let all = visible(&s, &view);
    assert_eq!(
        all.iter().map(|n| n.access.node).collect::<Vec<_>>(),
        (0..8).collect::<Vec<_>>()
    );
    let root = &all[0];
    let rect = s.node_rect(s.node_idx(0).expect("idx")).expect("rect");
    let at = view.to_screen(Point::new(rect.origin.x, rect.origin.y));
    assert!((root.x - at.x).abs() < 0.01 && (root.y - at.y).abs() < 0.01);
    assert!((root.w - rect.size.w * view.zoom).abs() < 0.01);

    // Zoomed on the root only, the rest is not offered (off screen means
    // not in the accessible list, and keyboard navigation reveals the node
    // it moves to).
    let mut close = Viewport::new(Size::new(300.0, 100.0));
    close.fit(rect, 5.0);
    let few = visible(&s, &close);
    assert!(
        few.len() < 8 && few.iter().any(|n| n.access.node == 0),
        "{}",
        few.len()
    );
}

#[test]
fn a_big_tree_is_described_in_time_for_what_is_visible_only() {
    // 5000 nodes; a small viewport must not describe them all (A9).
    let tree = common::wide(5000, 4);
    let s = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
    let mut view = Viewport::new(Size::new(800.0, 600.0));
    view.fit(aegis_canvas::Rect::new(0.0, 0.0, 400.0, 300.0), 0.0);
    let started = std::time::Instant::now();
    let few = visible(&s, &view);
    assert!(few.len() < 60, "{}", few.len());
    assert!(
        started.elapsed() < std::time::Duration::from_millis(50),
        "{:?}",
        started.elapsed()
    );
}
