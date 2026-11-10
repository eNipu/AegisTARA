//! Unit tests for the spike's pure modules, included by path because the spike is a binary.

#[allow(dead_code)]
#[path = "../src/args.rs"]
mod args;
#[allow(dead_code)]
#[path = "../src/palette.rs"]
mod palette;
#[allow(dead_code)]
#[path = "../src/sample_tree.rs"]
mod sample_tree;

use aegis_canvas::{LayoutConfig, NodeStyle, Paint, Scene, SvgTheme, Tone};
use aegis_model::ModelError;
use args::parse_nodes;
use palette::parse_hex;
use sample_tree::wide_tree;

fn argv(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn nodes_default_is_13() {
    assert_eq!(parse_nodes(&argv(&["spike"])), Ok(13));
}

#[test]
fn nodes_reads_the_count() {
    assert_eq!(
        parse_nodes(&argv(&["spike", "--nodes", "100000"])),
        Ok(100_000)
    );
}

#[test]
fn nodes_refuses_zero_words_and_too_many() {
    let cases = [
        ("0", "between 1 and 1000000, got 0"),
        ("many", "`many` is not a whole number"),
        ("1000001", "between 1 and 1000000, got 1000001"),
    ];
    for (bad, why) in cases {
        let err = parse_nodes(&argv(&["spike", "--nodes", bad])).unwrap_err();
        assert!(err.contains(why), "{bad}: {err}");
    }
    assert_eq!(
        parse_nodes(&argv(&["spike", "--nodes"])),
        Err("--nodes needs a number".to_string())
    );
}

#[test]
fn parses_hex() {
    assert_eq!(parse_hex("#E9EDF0"), Some([0xE9, 0xED, 0xF0]));
    assert_eq!(parse_hex("E9EDF0"), None);
    assert_eq!(parse_hex("#E9EDF"), None);
    assert_eq!(parse_hex("#E9EDFG"), None);
}

#[test]
fn every_paint_has_a_colour() {
    let tones = [
        Tone::Unrated,
        Tone::VeryLow,
        Tone::Low,
        Tone::Medium,
        Tone::High,
    ];
    let paints = [
        Paint::Sheet,
        Paint::Panel,
        Paint::Ink,
        Paint::Graphite,
        Paint::Rule,
        Paint::Wash,
        Paint::Accent,
        Paint::AccentWash,
    ]
    .into_iter()
    .chain(tones.map(Paint::Tone));
    for p in paints {
        assert!(parse_hex(SvgTheme::Light.hex(p)).is_some(), "{p:?}");
    }
}

#[test]
fn wide_tree_has_the_asked_size() -> Result<(), ModelError> {
    assert_eq!(wide_tree(13, 4)?.len(), 13);
    assert_eq!(wide_tree(100_000, 4)?.len(), 100_000);
    Ok(())
}

#[test]
fn wide_tree_labels_gates_by_position() -> Result<(), ModelError> {
    let tree = wide_tree(3, 4)?;
    let scene = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
    assert_eq!(scene.label(0), "Root");
    assert_eq!(scene.label(2), "Gate 2");
    Ok(())
}
