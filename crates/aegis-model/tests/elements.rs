//! Tests of the generic element kind table, `ElementId` and the element ops.

mod common;

use std::sync::Arc;

use aegis_model::{
    EntityKind, Methodology, Op, OpLogReader, Project, Provenance, StoreError, VocabKind,
};

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

// ---------------------------------------------------------------------------
// Element ops.
// ---------------------------------------------------------------------------

const MU: ElementKind = ElementKind::MisuseCase;

fn who() -> Provenance {
    Provenance::human("alice", 1)
}

fn add(n: u64, name: &str) -> Op {
    Op::AddElement {
        id: common::element_id(n),
        kind: MU,
        name: name.into(),
        provenance: who(),
    }
}

fn project() -> (Arc<Methodology>, Project) {
    let m = common::builtin();
    (Arc::clone(&m), Project::new(m))
}

fn names(p: &Project) -> Vec<String> {
    p.elements(MU).map(|(_, e)| e.name().to_owned()).collect()
}

#[test]
fn add_rename_remove_round_trip() {
    let (_, mut p) = project();
    let id = common::element_id(1);
    let rename = Op::RenameElement {
        id,
        name: "Renamed".into(),
        provenance: Provenance::human("bob", 2),
    };
    let remove = Op::RemoveElement { id };
    let steps = [
        (add(1, "Spoofed fob"), vec!["Spoofed fob"], 1),
        (rename, vec!["Renamed"], 1),
        (remove, vec![], 0),
    ];
    for (op, expected, count) in steps {
        let before = p.clone();
        let inverse = p.apply(op.clone()).expect("apply");
        assert_eq!(names(&p), expected);
        assert_eq!(p.element_count(MU), count);
        assert_eq!(p.element(id).is_some(), count == 1);
        let after = p.clone();
        let again = p.apply(inverse.clone()).expect("inverse");
        assert_eq!(p, before, "{op:?} then {inverse:?}");
        assert_eq!(again, op);
        p.apply(again).expect("redo");
        assert_eq!(p, after);
    }
    let (_, empty) = project();
    assert_eq!(p, empty);
}

#[test]
fn add_reads_back_the_element() {
    let (_, mut p) = project();
    p.apply(add(1, "Spoofed fob")).expect("add");
    let e = p.element(common::element_id(1)).expect("live");
    assert_eq!(e.kind(), MU);
    assert_eq!(e.name(), "Spoofed fob");
    assert_eq!(e.provenance(), &who());
}

#[test]
fn visual_ids_count_per_kind_and_are_not_reused() {
    let (_, mut p) = project();
    for n in 0..3 {
        p.apply(add(n, "x")).expect("add");
    }
    let vis = |p: &Project, n| p.element_visual_id(common::element_id(n));
    assert_eq!(vis(&p, 0).as_deref(), Some("MU00"));
    assert_eq!(vis(&p, 1).as_deref(), Some("MU01"));
    assert_eq!(vis(&p, 2).as_deref(), Some("MU02"));
    let undo_b = p
        .apply(Op::RemoveElement {
            id: common::element_id(1),
        })
        .expect("remove");
    assert_eq!(vis(&p, 1), None);
    p.apply(add(3, "d")).expect("add");
    assert_eq!(vis(&p, 3).as_deref(), Some("MU03"));
    p.apply(undo_b).expect("undo");
    assert_eq!(vis(&p, 1).as_deref(), Some("MU01"));
}

fn refusal(p: &mut Project, op: Op) -> ModelError {
    let before = p.clone();
    let invalid = p.validate(&op).expect_err("validate refuses");
    let err = p.apply(op).expect_err("apply refuses");
    assert_eq!(invalid, err);
    assert_eq!(*p, before);
    err
}

#[test]
fn add_refusals() {
    let (_, mut p) = project();
    p.apply(add(1, "first")).expect("add");
    assert_eq!(
        refusal(&mut p, add(1, "again")),
        ModelError::DuplicateElement(common::element_id(1))
    );
    for blank in ["", "  "] {
        assert_eq!(
            refusal(&mut p, add(2, blank)),
            ModelError::EmptyName {
                kind: EntityKind::Element
            }
        );
    }
    assert_eq!(
        refusal(&mut p, add(2, &"n".repeat(257))),
        ModelError::NameTooLong {
            kind: EntityKind::Element,
            max: 256
        }
    );
    let blank_actor = Op::AddElement {
        id: common::element_id(2),
        kind: MU,
        name: "ok".into(),
        provenance: Provenance::human("  ", 1),
    };
    assert_eq!(refusal(&mut p, blank_actor), ModelError::EmptyActor);
}

#[test]
fn kind_is_capped() {
    let (_, mut p) = project();
    for n in 0..MAX_ELEMENTS_PER_KIND as u64 {
        p.apply(add(n, "x")).expect("add");
    }
    assert_eq!(
        refusal(&mut p, add(MAX_ELEMENTS_PER_KIND as u64, "x")),
        ModelError::ProjectCapacityExceeded {
            kind: EntityKind::Element
        }
    );
}

#[test]
fn rename_and_remove_unknown() {
    let (_, mut p) = project();
    let id = common::element_id(9);
    let rename = Op::RenameElement {
        id,
        name: "x".into(),
        provenance: who(),
    };
    assert_eq!(refusal(&mut p, rename), ModelError::UnknownElement(id));
    assert_eq!(
        refusal(&mut p, Op::RemoveElement { id }),
        ModelError::UnknownElement(id)
    );
}

#[test]
fn rename_refuses_a_bad_name() {
    let (_, mut p) = project();
    p.apply(add(1, "first")).expect("add");
    let rename = Op::RenameElement {
        id: common::element_id(1),
        name: " ".into(),
        provenance: who(),
    };
    assert_eq!(
        refusal(&mut p, rename),
        ModelError::EmptyName {
            kind: EntityKind::Element
        }
    );
}

#[test]
fn rename_refuses_a_blank_actor() {
    let (_, mut p) = project();
    p.apply(add(1, "first")).expect("add");
    let rename = Op::RenameElement {
        id: common::element_id(1),
        name: "ok".into(),
        provenance: Provenance::human("  ", 1),
    };
    assert_eq!(refusal(&mut p, rename), ModelError::EmptyActor);
}

fn log_ops() -> Vec<Op> {
    vec![
        add(1, "Spoofed fob"),
        Op::RenameElement {
            id: common::element_id(1),
            name: "Renamed".into(),
            provenance: Provenance::human("bob", 2),
        },
        Op::RemoveElement {
            id: common::element_id(1),
        },
    ]
}

#[test]
fn ops_round_trip_through_the_op_log() {
    let m = common::builtin();
    let ops = log_ops();
    let bytes = common::write_log(&m, &ops);
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    assert!(text.contains(r#""op":"add_element""#), "{text}");
    assert!(text.contains(r#""kind":"misuse-case""#), "{text}");
    let read: Vec<Op> = common::read_log(&m, &bytes)
        .into_iter()
        .map(|r| r.op)
        .collect();
    assert_eq!(read, ops);
}

#[test]
fn op_log_refuses_an_unknown_kind() {
    let m = common::builtin();
    let bytes = common::write_log(&m, &[add(1, "x")]);
    let text = String::from_utf8(bytes).expect("utf8");
    let bad = text.replace("\"misuse-case\"", "\"misuse-cas\"");
    assert_ne!(bad, text);
    let results: Vec<_> = OpLogReader::open(bad.as_bytes(), m)
        .expect("header")
        .collect();
    assert_eq!(results.len(), 1);
    assert!(
        matches!(
            &results[0],
            Err(StoreError::UnknownKey {
                kind: VocabKind::ElementKind,
                ..
            })
        ),
        "{:?}",
        results[0]
    );
}

#[test]
fn errors_do_not_contain_names() {
    let (_, mut p) = project();
    let secret = common::SENTINEL;
    p.apply(add(1, "first")).expect("add");
    let errors = [
        refusal(&mut p, add(1, secret)),
        refusal(&mut p, add(2, &format!("{secret}{}", "n".repeat(257)))),
        refusal(
            &mut p,
            Op::RenameElement {
                id: common::element_id(9),
                name: secret.into(),
                provenance: who(),
            },
        ),
        refusal(
            &mut p,
            Op::RemoveElement {
                id: common::element_id(9),
            },
        ),
    ];
    for e in errors {
        assert!(!e.to_string().contains(secret), "{e}");
    }
}

// ---------------------------------------------------------------------------
// Element attributes (205c).
// ---------------------------------------------------------------------------

use aegis_model::{AttrValue, MAX_ELEMENT_ATTRIBUTES};

fn set_attr(n: u64, key: &str, value: Option<AttrValue>) -> Op {
    Op::SetElementAttribute {
        id: common::element_id(n),
        key: key.into(),
        value,
        provenance: who(),
    }
}

fn lvl(s: &str) -> Option<AttrValue> {
    Some(AttrValue::Level(s.into()))
}

#[test]
fn set_and_clear_attributes_round_trip() {
    let (_, mut p) = project();
    p.apply(add(1, "Spoofed fob")).expect("add");
    let id = common::element_id(1);
    let steps = [
        set_attr(1, "cal", lvl("cal-2")),
        set_attr(1, "approach", Some(AttrValue::Choice("manual".into()))),
        set_attr(1, "note", Some(AttrValue::Text("Free text".into()))),
        set_attr(1, "cal", lvl("cal-3")),
        set_attr(1, "approach", None),
    ];
    for op in steps {
        let before = p.clone();
        let inverse = p.apply(op.clone()).expect("apply");
        let after = p.clone();
        let keys: Vec<_> = p
            .element(id)
            .expect("live")
            .attributes()
            .iter()
            .map(|(k, _)| k.to_string())
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        let again = p.apply(inverse.clone()).expect("inverse");
        assert_eq!(p, before, "{op:?} then {inverse:?}");
        assert_eq!(again, op);
        p.apply(again).expect("redo");
        assert_eq!(p, after);
    }
    let e = p.element(id).expect("live");
    assert_eq!(e.attribute("cal"), lvl("cal-3").as_ref());
    assert_eq!(e.attribute("approach"), None);
    // Replacing returns the old value.
    let inverse = p.apply(set_attr(1, "cal", lvl("cal-4"))).expect("replace");
    assert_eq!(inverse, set_attr(1, "cal", lvl("cal-3")));
}

#[test]
fn clearing_an_unset_key_is_allowed() {
    let (_, mut p) = project();
    p.apply(add(1, "x")).expect("add");
    let before = p.clone();
    let op = set_attr(1, "cal", None);
    let inverse = p.apply(op.clone()).expect("clear");
    assert_eq!(inverse, op);
    assert!(
        p.element(common::element_id(1))
            .expect("live")
            .attributes()
            .is_empty()
    );
    p.apply(inverse).expect("undo");
    assert_eq!(p, before);
}

#[test]
fn attribute_refusals() {
    let (_, mut p) = project();
    let id = common::element_id(1);
    assert_eq!(
        refusal(&mut p, set_attr(1, "cal", lvl("a"))),
        ModelError::UnknownElement(id)
    );
    p.apply(add(1, "x")).expect("add");
    let long = "a".repeat(65);
    for key in ["", "Cal", "1cal", "cal level", long.as_str()] {
        assert_eq!(
            refusal(&mut p, set_attr(1, key, lvl("a"))),
            ModelError::InvalidAttributeKey { element: id },
            "{key:?}"
        );
    }
    let text_long = "a".repeat(257);
    let values = [
        AttrValue::Level("".into()),
        AttrValue::Choice("A".into()),
        AttrValue::Text("".into()),
        AttrValue::Text("  ".into()),
        AttrValue::Text(text_long.as_str().into()),
        AttrValue::Text("a\u{0}b".into()),
    ];
    for v in values {
        assert_eq!(
            refusal(&mut p, set_attr(1, "cal", Some(v.clone()))),
            ModelError::InvalidAttributeValue { element: id },
            "{v:?}"
        );
    }
    for i in 0..MAX_ELEMENT_ATTRIBUTES {
        p.apply(set_attr(1, &format!("k{i:02}"), lvl("a")))
            .expect("fill");
    }
    assert_eq!(
        refusal(&mut p, set_attr(1, "extra", lvl("a"))),
        ModelError::TooManyAttributes {
            element: id,
            max: 32
        }
    );
    p.apply(set_attr(1, "k00", lvl("b")))
        .expect("replace on full");
    p.apply(set_attr(1, "k01", None)).expect("clear on full");
    p.apply(set_attr(1, "extra", lvl("a"))).expect("room again");
    let bad_prov = Op::SetElementAttribute {
        id,
        key: "k00".into(),
        value: lvl("c"),
        provenance: Provenance::human("  ", 1),
    };
    assert_eq!(refusal(&mut p, bad_prov), ModelError::EmptyActor);
}

#[test]
fn remove_is_refused_while_attributes_are_set() {
    let (_, mut p) = project();
    let id = common::element_id(1);
    p.apply(add(1, "x")).expect("add");
    p.apply(set_attr(1, "cal", lvl("a"))).expect("set");
    assert_eq!(
        refusal(&mut p, Op::RemoveElement { id }),
        ModelError::ElementHasAttributes {
            element: id,
            count: 1
        }
    );
    p.apply(set_attr(1, "cal", None)).expect("clear");
    let before = p.clone();
    let inverse = p.apply(Op::RemoveElement { id }).expect("remove");
    assert!(p.element(id).is_none());
    p.apply(inverse).expect("undo");
    assert_eq!(p, before);
    assert!(p.element(id).expect("live").attributes().is_empty());
}

fn attr_ops() -> Vec<Op> {
    vec![
        set_attr(1, "cal", lvl("cal-2")),
        set_attr(1, "approach", Some(AttrValue::Choice("manual".into()))),
        set_attr(1, "note", Some(AttrValue::Text("Free text".into()))),
        set_attr(1, "cal", None),
    ]
}

#[test]
fn attribute_ops_round_trip_through_the_op_log() {
    let m = common::builtin();
    let ops = attr_ops();
    let bytes = common::write_log(&m, &ops);
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    assert!(text.contains(r#""op":"set_element_attribute""#), "{text}");
    assert!(text.contains(r#""value":{"level":"cal-2"}"#), "{text}");
    assert!(text.contains(r#""value":{"choice":"manual"}"#), "{text}");
    assert!(text.contains(r#""value":{"text":"Free text"}"#), "{text}");
    assert!(text.contains(r#""value":null"#), "{text}");
    let read: Vec<Op> = common::read_log(&m, &bytes)
        .into_iter()
        .map(|r| r.op)
        .collect();
    assert_eq!(read, ops);
}

#[test]
fn op_log_refuses_a_missing_value_and_an_unknown_value_type() {
    let m = common::builtin();
    let bytes = common::write_log(&m, &[set_attr(1, "cal", lvl("cal-2"))]);
    let text = String::from_utf8(bytes).expect("utf8");
    let no_value = text.replace(r#","value":{"level":"cal-2"}"#, "");
    let bad_type = text.replace(r#"{"level":"cal-2"}"#, r#"{"number":1}"#);
    for bad in [no_value, bad_type] {
        assert_ne!(bad, text);
        let results: Vec<_> = OpLogReader::open(bad.as_bytes(), Arc::clone(&m))
            .expect("header")
            .collect();
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], Err(StoreError::Decode { .. })),
            "{:?}",
            results[0]
        );
    }
}
