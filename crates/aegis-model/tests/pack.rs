//! Methodology pack files (job 012): the second methodology as a pack
//! (AC6) and the strict v1 format (AC8).

mod common;

use std::sync::Arc;

use aegis_model::{
    Methodology, MethodologyDef, MethodologyError, PackError, Project, SegmentDef, StoreError,
};
use common::{pack_bytes, second_def};

/// The second methodology as a pack fixture.
const SECOND: &str = include_str!("fixtures/packs/med-device-demo.json");

// ---------------------------------------------------------------------------
// AC6: a second methodology is a data change.
// ---------------------------------------------------------------------------

#[test]
fn second_pack_loads_equal_to_the_definition() {
    let m = Methodology::read_pack(SECOND.as_bytes()).expect("pack loads");
    assert_eq!(Methodology::new(second_def()).ok(), Some(m));
    let def = MethodologyDef::read_pack(SECOND.as_bytes()).expect("pack decodes");
    assert_eq!(def, second_def());
}

#[test]
fn second_pack_is_canonical() {
    assert_eq!(pack_bytes(&second_def()), SECOND.as_bytes());
}

#[test]
fn second_pack_every_op() {
    let m = Methodology::read_pack(SECOND.as_bytes()).expect("pack loads");
    common::second_every_op(Arc::new(m));
}

#[test]
fn second_pack_loads_projects_saved_under_the_definition() {
    // A project saved under the hand-built methodology loads under the
    // pack-loaded one: same key, same vocabulary.
    let built = common::second();
    let loaded = Arc::new(Methodology::read_pack(SECOND.as_bytes()).expect("pack loads"));
    let mut p = Project::new(Arc::clone(&built));
    let pump = common::asset_id(1);
    p.apply(aegis_model::Op::AddAsset {
        id: pump,
        name: "Pump".into(),
        properties: built.all_properties(),
        provenance: aegis_model::Provenance::default(),
    })
    .expect("add asset");
    let bytes = common::save(&p);
    let back = Project::load(bytes.as_slice(), loaded).expect("loads");
    assert_eq!(common::save(&back), bytes);
}

// ---------------------------------------------------------------------------
// AC8: strict format.
// ---------------------------------------------------------------------------

/// A small valid pack over 11 lines, so a test can name the line it breaks.
const TINY: &str = r#"{
"format": "aegis-methodology-pack",
"version": 1,
"methodology": {
"key": "tiny", "version": "1", "label": "Tiny",
"impact_categories": [{"key": "harm", "label": "Harm", "levels": [{"key": "low", "label": "Low"}, {"key": "high", "label": "High"}], "risk_matrix": [["low", "high"], ["high", "high"]]}],
"properties": [{"key": "i", "label": "I"}],
"threat_categories": [{"key": "t", "label": "T", "violates": ["i"]}],
"catalog_schemes": [{"key": "s", "label": "S", "syntax": {"prefix": "", "required": 1, "segments": [{"min": 1, "max": 9}]}}], "feasibility_levels": [{"key": "low", "label": "Low"}, {"key": "high", "label": "High"}], "risk_levels": [{"key": "low", "label": "Low"}, {"key": "high", "label": "High"}]
}
}
"#;

/// `TINY` with `from` replaced by `to` (exactly once).
fn edit(from: &str, to: &str) -> String {
    assert_eq!(TINY.matches(from).count(), 1, "{from:?}");
    TINY.replacen(from, to, 1)
}

fn read(text: &str) -> Result<Methodology, PackError> {
    Methodology::read_pack(text.as_bytes())
}

/// The line of a `Decode` error, or a description of what came instead.
fn decode_line(text: &str) -> Result<u64, String> {
    match read(text) {
        Err(PackError::Decode { line, .. }) => Ok(line),
        other => Err(format!("{other:?}")),
    }
}

#[test]
fn pack_rejects_nothing_in_the_tiny_pack() {
    let m = read(TINY).expect("valid");
    assert_eq!(m.key(), "tiny");
    assert_eq!(m.catalog_scheme_count(), 1);
}

#[test]
fn pack_rejects_wrong_format_name() {
    for name in ["aegis-project", "aegis-oplog", "", "AEGIS-METHODOLOGY-PACK"] {
        let text = edit(
            r#""format": "aegis-methodology-pack""#,
            &format!(r#""format": "{name}""#),
        );
        assert!(
            matches!(
                read(&text),
                Err(PackError::UnsupportedFormat { version: None })
            ),
            "{name:?}"
        );
    }
    // Checked as soon as it is read: what follows does not matter.
    let early = r#"{"format": "spreadsheet", "methodology": 7"#;
    assert!(matches!(
        read(early),
        Err(PackError::UnsupportedFormat { version: None })
    ));
}

#[test]
fn pack_rejects_newer_format_version() {
    let text = edit(r#""version": 1,"#, r#""version": 2,"#);
    let err = read(&text).err();
    assert!(matches!(
        err,
        Some(PackError::UnsupportedFormat { version: Some(2) })
    ));
    // Same text as the store's error.
    let store = StoreError::UnsupportedFormat { version: Some(2) };
    assert_eq!(err.map(|e| e.to_string()), Some(store.to_string()));
}

#[test]
fn pack_rejects_invalid_format_version() {
    for v in ["0", "-1", "1.5", "1.0", "-0.5"] {
        let text = edit(r#""version": 1,"#, &format!(r#""version": {v},"#));
        assert!(
            matches!(
                read(&text),
                Err(PackError::UnsupportedFormat { version: None })
            ),
            "{v}"
        );
    }
    let text = edit(r#""version": 1,"#, r#""version": "1","#);
    assert_eq!(decode_line(&text), Ok(3));
}

#[test]
fn pack_rejects_unknown_fields() {
    let cases = [
        (r#""version": 1,"#, r#""version": 1, "extra": 1,"#, 3),
        (r#""label": "Tiny","#, r#""label": "Tiny", "extra": 1,"#, 5),
        (
            r#""key": "i", "label": "I""#,
            r#""key": "i", "label": "I", "x": 0"#,
            7,
        ),
        (
            r#""min": 1, "max": 9"#,
            r#""min": 1, "max": 9, "width": 2"#,
            9,
        ),
        (r#""prefix": "","#, r#""prefix": "", "suffix": "","#, 9),
    ];
    for (from, to, line) in cases {
        assert_eq!(decode_line(&edit(from, to)), Ok(line), "{to}");
    }
}

#[test]
fn pack_rejects_missing_fields() {
    let cases = [
        // A missing field is reported where its object closes.
        (r#""key": "tiny", "#, "", 10),
        (r#""properties": [{"key": "i", "label": "I"}],"#, "", 10),
        (r#""catalog_schemes""#, r#""catalog_schemes_x""#, 9),
        (r#""violates": ["i"]"#, r#""violates2": ["i"]"#, 8),
        (r#""required": 1, "#, "", 9),
    ];
    for (from, to, line) in cases {
        assert_eq!(decode_line(&edit(from, to)), Ok(line), "{from}");
    }
    let no_methodology = r#"{"format": "aegis-methodology-pack", "version": 1}"#;
    assert_eq!(decode_line(no_methodology), Ok(1));
}

#[test]
fn pack_rejects_repeated_fields() {
    let cases = [
        (r#""key": "tiny","#, r#""key": "tiny", "key": "tiny","#, 5),
        (r#""version": 1,"#, r#""version": 1, "version": 1,"#, 3),
        (
            r#""min": 1, "max": 9"#,
            r#""min": 1, "max": 9, "max": 9"#,
            9,
        ),
        (r#""max": 9}"#, r#""max": 9, "digits": 1, "digits": 1}"#, 9),
    ];
    for (from, to, line) in cases {
        assert_eq!(decode_line(&edit(from, to)), Ok(line), "{to}");
    }
}

#[test]
fn pack_rejects_mistyped_fields() {
    let cases = [
        (r#""label": "Tiny""#, r#""label": 5"#, 5),
        (r#""label": "Tiny""#, r#""label": null"#, 5),
        (r#""violates": ["i"]"#, r#""violates": "i""#, 8),
        (r#""violates": ["i"]"#, r#""violates": [1]"#, 8),
        (r#""required": 1"#, r#""required": "1""#, 9),
        (r#""required": 1"#, r#""required": true"#, 9),
        (
            r#""properties": [{"key": "i", "label": "I"}]"#,
            r#""properties": {}"#,
            7,
        ),
        // An object written as an array of its field values.
        (r#"{"key": "i", "label": "I"}"#, r#"["i", "I"]"#, 7),
    ];
    for (from, to, line) in cases {
        assert_eq!(decode_line(&edit(from, to)), Ok(line), "{to}");
    }
}

#[test]
fn pack_rejects_numbers_out_of_range() {
    let cases = [
        (r#""min": 1"#, r#""min": 70000"#),
        (r#""max": 9"#, r#""max": 65536"#),
        (r#""min": 1"#, r#""min": -1"#),
        (r#""min": 1"#, r#""min": 1.5"#),
        (r#""required": 1"#, r#""required": 256"#),
        (r#""max": 9}"#, r#""max": 9, "digits": 300}"#),
    ];
    for (from, to) in cases {
        assert_eq!(decode_line(&edit(from, to)), Ok(9), "{to}");
    }
    // At the top of each range, decoding succeeds (the model may object).
    let at = edit(r#""min": 1, "max": 9"#, r#""min": 65535, "max": 65535"#);
    assert!(read(&at).is_ok());
}

#[test]
fn pack_rejects_trailing_data() {
    assert_eq!(decode_line(&format!("{TINY}x")), Ok(12));
    assert_eq!(decode_line(&format!("{TINY}{{}}")), Ok(12));
    // Trailing whitespace is fine.
    assert!(read(&format!("{TINY} \n\t\r\n")).is_ok());
}

#[test]
fn pack_rejects_bad_json() {
    // Cut short: the error is at the end of the input (column 0 on an
    // empty input).
    assert!(matches!(
        read(""),
        Err(PackError::Decode { line: 1, column: 0 })
    ));
    assert_eq!(decode_line(&TINY[..200]), Ok(6));
    let text = edit(r#""label": "Tiny","#, r#""label": "Tiny""#);
    assert_eq!(decode_line(&text), Ok(6));
    let bad_utf8 = [TINY.as_bytes(), &[0xff]].concat();
    assert!(matches!(
        Methodology::read_pack(bad_utf8.as_slice()),
        Err(PackError::Decode { .. })
    ));
}

#[test]
fn pack_rejects_model_rule_breaks_as_methodology_errors() {
    let text = edit(
        r#""properties": [{"key": "i", "label": "I"}]"#,
        r#""properties": [{"key": "i", "label": "I"}, {"key": "i", "label": "Again"}]"#,
    );
    assert!(matches!(
        read(&text),
        Err(PackError::Methodology(MethodologyError::DuplicateProperty(k))) if &*k == "i"
    ));
    let text = edit(r#""violates": ["i"]"#, r#""violates": ["x"]"#);
    assert!(matches!(
        read(&text),
        Err(PackError::Methodology(
            MethodologyError::UnknownViolatedProperty { .. }
        ))
    ));
    // `MethodologyDef::read_pack` alone does not validate.
    assert!(MethodologyDef::read_pack(text.as_bytes()).is_ok());
    let shown = read(&text).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(shown.starts_with("invalid methodology: "), "{shown}");
    // Feasibility scale rules are checked too.
    let one_f = edit(
        r#""feasibility_levels": [{"key": "low", "label": "Low"}, {"key": "high", "label": "High"}]"#,
        r#""feasibility_levels": [{"key": "low", "label": "Low"}]"#,
    );
    assert!(matches!(
        read(&one_f),
        Err(PackError::Methodology(
            MethodologyError::FeasibilityLevelCount { min: 2, max: 16 }
        ))
    ));
    let dup_f = edit(
        r#""feasibility_levels": [{"key": "low", "label": "Low"}, {"key": "high", "label": "High"}]"#,
        r#""feasibility_levels": [{"key": "low", "label": "Low"}, {"key": "low", "label": "Again"}]"#,
    );
    assert!(matches!(
        read(&dup_f),
        Err(PackError::Methodology(MethodologyError::DuplicateFeasibilityLevel(k))) if &*k == "low"
    ));
    // Risk scale and matrix rules are checked too.
    let one_r = edit(
        r#""risk_levels": [{"key": "low", "label": "Low"}, {"key": "high", "label": "High"}]"#,
        r#""risk_levels": [{"key": "low", "label": "Low"}]"#,
    );
    assert!(matches!(
        read(&one_r),
        Err(PackError::Methodology(MethodologyError::RiskLevelCount {
            min: 2,
            max: 16
        }))
    ));
    let bad_matrix = edit(
        r#""risk_matrix": [["low", "high"], ["high", "high"]]"#,
        r#""risk_matrix": [["low", "high"], ["high", "unknown"]]"#,
    );
    assert!(matches!(
        read(&bad_matrix),
        Err(PackError::Methodology(
            MethodologyError::UnknownRiskLevel { .. }
        ))
    ));
}

#[test]
fn pack_rejects_neither_missing_nor_null_digits() {
    let digits = |to: &str| {
        let text = edit(r#""max": 9}"#, to);
        MethodologyDef::read_pack(text.as_bytes())
            .ok()
            .and_then(|d| d.catalog_schemes.first().map(|s| s.syntax.segments.clone()))
    };
    let none = Some(vec![SegmentDef {
        min: 1,
        max: 9,
        digits: None,
    }]);
    assert_eq!(digits(r#""max": 9}"#), none);
    assert_eq!(digits(r#""max": 9, "digits": null}"#), none);
    assert_eq!(
        digits(r#""max": 9, "digits": 1}"#),
        Some(vec![SegmentDef {
            min: 1,
            max: 9,
            digits: Some(1),
        }])
    );
    // The writer leaves out `None` and writes `Some`.
    let mut def = MethodologyDef::read_pack(TINY.as_bytes()).expect("valid");
    let text = String::from_utf8(pack_bytes(&def)).expect("utf-8");
    assert!(!text.contains("digits"));
    def.catalog_schemes[0].syntax.segments[0].digits = Some(1);
    let text = String::from_utf8(pack_bytes(&def)).expect("utf-8");
    assert!(text.contains(r#""digits": 1"#));
}

#[test]
fn pack_rejects_nothing_when_fields_are_reordered() {
    // Field order is free on input; the writer's order is canonical.
    let text = r#"{"methodology": {"risk_levels": [{"label": "L", "key": "l"}, {"label": "H", "key": "h"}],
        "feasibility_levels": [{"label": "L", "key": "l"}, {"label": "H", "key": "h"}],
        "catalog_schemes": [], "threat_categories": [{"violates": ["i"], "label": "T", "key": "t"}],
        "properties": [{"label": "I", "key": "i"}], "impact_categories": [{"risk_matrix": [["l", "h"], ["h", "h"]],
        "levels": [{"label": "L", "key": "l"}, {"label": "H", "key": "h"}], "label": "C", "key": "c"}],
        "label": "X", "version": "1", "key": "x"},
        "version": 1, "format": "aegis-methodology-pack"}"#;
    let def = MethodologyDef::read_pack(text.as_bytes()).expect("valid");
    let canonical = pack_bytes(&def);
    assert_eq!(
        MethodologyDef::read_pack(canonical.as_slice()).ok(),
        Some(def)
    );
    assert!(
        canonical.starts_with(b"{\n  \"format\": \"aegis-methodology-pack\",\n  \"version\": 1,\n")
    );
    assert!(canonical.ends_with(b"}\n"));
}

#[test]
fn pack_writer_keeps_non_ascii_raw_and_escapes_controls() {
    let mut def = second_def();
    def.label = "Médical \u{1f600} \"q\" \\ \u{1}\n".into();
    let text = String::from_utf8(pack_bytes(&def)).expect("utf-8");
    assert!(text.contains(r#""label": "Médical 😀 \"q\" \\ \u0001\n","#));
    assert_eq!(MethodologyDef::read_pack(text.as_bytes()).ok(), Some(def));
}

#[test]
fn a_pack_without_attack_types_loads_with_none() {
    let m = Methodology::read_pack(TINY.as_bytes()).expect("valid");
    assert_eq!(m.attack_type_count(), 0);
    assert_eq!(m.attack_types().count(), 0);
    assert_eq!(m.attack_type("firmware-modification"), None);
}

#[test]
fn a_pack_without_control_kinds_loads_with_none() {
    let m = Methodology::read_pack(TINY.as_bytes()).expect("valid");
    assert_eq!(m.control_kinds().len(), 0);
}

/// `TINY` with one attack type, one level key and a preset body `kind`.
fn with_kind(kind: &str) -> String {
    edit(
        r#""properties": [{"key": "i", "label": "I"}],"#,
        &format!(
            r#""attack_types": [{{"key": "a", "label": "A"}}], "control_kinds": [{kind}], "properties": [{{"key": "i", "label": "I"}}],"#
        ),
    )
}

#[test]
fn a_control_kind_loads() {
    let text = with_kind(r#"{"key": "k", "label": "K", "rules": {"a": "low"}}"#);
    let m = read(&text).expect("valid");
    assert_eq!(m.control_kinds().len(), 1);
}

#[test]
fn a_control_kind_with_an_unknown_field_is_refused() {
    let text = with_kind(r#"{"key": "k", "label": "K", "rules": {"a": "low"}, "x": 1}"#);
    assert!(matches!(read(&text), Err(PackError::Decode { .. })));
}

#[test]
fn a_control_kind_without_rules_is_refused() {
    let text = with_kind(r#"{"key": "k", "label": "K"}"#);
    assert!(matches!(read(&text), Err(PackError::Decode { .. })));
}

#[test]
fn a_repeated_rule_key_is_refused() {
    let text = with_kind(r#"{"key": "k", "label": "K", "rules": {"a": "low", "a": "high"}}"#);
    assert!(matches!(read(&text), Err(PackError::Decode { .. })));
}
