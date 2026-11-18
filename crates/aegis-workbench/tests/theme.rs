//! The palette files: they parse, and the text colours are readable.

use aegis_workbench::theme::{Tokens, contrast, dark, light, parse_hex};

fn hex(s: &str) -> [u8; 3] {
    parse_hex(s).unwrap_or_else(|| panic!("not a colour: {s}"))
}

fn all(t: &Tokens) -> Vec<&str> {
    let mut v = vec![
        &t.surface,
        &t.sidebar,
        &t.raised,
        &t.line,
        &t.line_strong,
        &t.ink,
        &t.ink_muted,
        &t.accent,
        &t.accent_wash,
        &t.error,
        &t.warning,
    ];
    v.extend(t.systems.iter());
    v.into_iter().map(String::as_str).collect()
}

#[test]
fn both_files_parse_and_every_colour_is_hex() {
    for t in [light(), dark()] {
        for c in all(&t) {
            assert!(parse_hex(c).is_some(), "{c}");
        }
    }
    assert_eq!(parse_hex("#12345"), None);
    assert_eq!(parse_hex("123456"), None);
    assert_eq!(parse_hex("#12345G"), None);
}

#[test]
fn the_formula_matches_the_documented_ratios() {
    let white = hex("#FFFFFF");
    let muted = contrast(hex("#4F5D6B"), white);
    let accent = contrast(white, hex("#2F6DB5"));
    // project-explorer.md section 8 rounds this to 6.5; the WCAG formula
    // (and any outside checker) gives 6.75 for #4F5D6B on white.
    assert!((muted - 6.75).abs() <= 0.01, "{muted}");
    assert!((accent - 5.3).abs() <= 0.1, "{accent}");
    assert!((contrast(white, hex("#000000")) - 21.0).abs() < 1e-9);
}

#[test]
fn text_colours_reach_4_5_in_both_themes() {
    for t in [light(), dark()] {
        let surface = hex(&t.surface);
        for c in [&t.ink, &t.ink_muted, &t.accent] {
            let r = contrast(hex(c), surface);
            assert!(r >= 4.5, "{c} on {}: {r}", t.surface);
        }
        let r = contrast(hex(&t.ink), hex(&t.accent_wash));
        assert!(r >= 4.5, "ink on {}: {r}", t.accent_wash);
    }
}

#[test]
fn the_six_system_colours_are_distinct() {
    for t in [light(), dark()] {
        let mut s: Vec<_> = t.systems.iter().map(|c| hex(c)).collect();
        s.sort_unstable();
        s.dedup();
        assert_eq!(s.len(), 6);
    }
}
