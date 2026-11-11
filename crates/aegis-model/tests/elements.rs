//! Tests of the generic element kind table and `ElementId`.

use aegis_model::{ElementId, ElementKind, KINDS, MAX_ELEMENT_LINKS, MAX_ELEMENTS_PER_KIND};
use aegis_model::{ModelError, TargetKind, VisualPrefix};
use std::collections::HashSet;

fn is_key(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_lowercase())
        && s.len() <= 64
        && c.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

#[test]
fn kinds_match_their_rows() {
    assert_eq!(KINDS.len(), ElementKind::ALL.len());
    for (i, k) in ElementKind::ALL.iter().enumerate() {
        assert_eq!(k.def().kind, *k);
        assert_eq!(KINDS.get(i).map(|d| d.kind), Some(*k));
        assert_eq!(ElementKind::from_key(k.key()), Some(*k));
    }
}

#[test]
fn keys_and_prefixes_are_unique_and_well_formed() {
    let visual: Vec<&str> = [
        VisualPrefix::Asset,
        VisualPrefix::Damage,
        VisualPrefix::Threat,
        VisualPrefix::Tree,
        VisualPrefix::Control,
        VisualPrefix::Goal,
        VisualPrefix::Claim,
        VisualPrefix::Component,
        VisualPrefix::Interface,
        VisualPrefix::DataFlow,
        VisualPrefix::TrustBoundary,
    ]
    .iter()
    .map(|p| p.as_str())
    .collect();
    let mut keys = HashSet::new();
    let mut prefixes = HashSet::new();
    let mut stems = HashSet::new();
    for d in KINDS {
        assert!(is_key(d.key), "{}", d.key);
        assert!(keys.insert(d.key));
        assert!((1..=3).contains(&d.prefix.len()));
        assert!(d.prefix.bytes().all(|b| b.is_ascii_uppercase()));
        assert!(prefixes.insert(d.prefix));
        assert!(!visual.contains(&d.prefix));
        assert!(
            d.file_stem
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        );
        assert!(!d.file_stem.is_empty());
        assert!(stems.insert(d.file_stem));
    }
}

#[test]
fn row_caps_fit_the_global_caps() {
    for d in KINDS {
        assert!(d.max_elements <= MAX_ELEMENTS_PER_KIND);
        assert!(d.max_links <= MAX_ELEMENT_LINKS);
        assert!(d.max_elements > 0);
        assert!(!d.targets.is_empty());
    }
}

#[test]
fn misuse_case_row() {
    let k = ElementKind::MisuseCase;
    assert_eq!(k.key(), "misuse-case");
    assert_eq!(k.prefix(), "MU");
    assert_eq!(k.def().file_stem, "misuse");
    assert!(k.allows(TargetKind::Threat));
}

#[test]
fn visual_ids_format_and_parse() {
    let k = ElementKind::MisuseCase;
    assert_eq!(k.visual_id(0), "MU00");
    assert_eq!(k.visual_id(100), "MU100");
    assert_eq!(k.parse_visual("MU07"), Some(7));
    for bad in ["MU7", "MU007", "A00", "mu00"] {
        assert_eq!(k.parse_visual(bad), None, "{bad}");
    }
}

#[test]
fn element_id_round_trips_and_refuses_v4() {
    let id = ElementId::new_v7();
    assert_eq!(id.to_string().parse::<ElementId>(), Ok(id));
    assert_eq!(
        "67e55044-10b1-426f-9247-bb680e5fe0c8".parse::<ElementId>(),
        Err(ModelError::NotVersion7)
    );
}

#[test]
fn from_key_refuses_unknown() {
    for bad in ["misuse", "", "MISUSE-CASE"] {
        assert_eq!(ElementKind::from_key(bad), None, "{bad}");
    }
}
