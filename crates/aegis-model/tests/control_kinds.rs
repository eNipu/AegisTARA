//! Setting presets (control kinds) in a methodology definition (CFG-06).

mod common;

use std::sync::Arc;

use aegis_model::{
    ControlKindDef, ControlRule, MAX_CONTROL_KINDS, Methodology, MethodologyDef, MethodologyError,
    PackTranslationDef,
};

fn s(text: &str) -> Arc<str> {
    Arc::from(text)
}

fn base() -> Result<MethodologyDef, Box<dyn std::error::Error>> {
    Ok(MethodologyDef::read_pack(
        Methodology::iso21434_r155_pack().as_bytes(),
    )?)
}

fn kind(key: &str, interfaces: &[&str], rules: &[(&str, &str)]) -> ControlKindDef {
    ControlKindDef {
        key: s(key),
        label: s(&format!("Label {key}")),
        interface_kinds: interfaces.iter().map(|k| s(k)).collect(),
        rules: rules.iter().map(|&(a, l)| (s(a), s(l))).collect(),
    }
}

fn ok_kind(key: &str) -> ControlKindDef {
    kind(key, &[], &[("debug-access", "low")])
}

fn refused(kinds: Vec<ControlKindDef>) -> Result<MethodologyError, Box<dyn std::error::Error>> {
    let mut def = base()?;
    def.element_names.control_kinds = kinds;
    match Methodology::new(def) {
        Err(e) => Ok(e),
        Ok(_) => Err("definition was accepted".into()),
    }
}

#[test]
fn presets_load_and_come_back() -> Result<(), Box<dyn std::error::Error>> {
    let mut def = base()?;
    def.element_names.control_kinds = vec![
        kind(
            "secure-boot",
            &[],
            &[
                ("firmware-modification", "very-low"),
                ("debug-access", "low"),
            ],
        ),
        kind(
            "debug-lock",
            &["usb", "debug"],
            &[("flash-read-out", "high")],
        ),
    ];
    let m = Methodology::new(def)?;
    assert_eq!(m.control_kinds().count(), 2);
    assert_eq!(m.control_kind("nope"), None);

    let boot = m.control_kind("secure-boot").ok_or("no secure-boot")?;
    assert_eq!(m.control_kind_key(boot), Some("secure-boot"));
    assert_eq!(m.control_kind_label(boot), Some("Label secure-boot"));
    assert!(m.control_kind_interface_kinds(boot).is_empty());
    let fw = m.attack_type("firmware-modification").ok_or("fw")?;
    let dbg = m.attack_type("debug-access").ok_or("dbg")?;
    let vlow = m.feasibility_level("very-low").ok_or("vlow")?;
    let low = m.feasibility_level("low").ok_or("low")?;
    let mut expected = vec![ControlRule::new(fw, vlow), ControlRule::new(dbg, low)];
    expected.sort();
    assert_eq!(m.control_kind_rules(boot), expected.as_slice());

    let lock = m.control_kind("debug-lock").ok_or("no debug-lock")?;
    let usb = m.interface_kind("usb").ok_or("usb")?;
    let debug = m.interface_kind("debug").ok_or("debug")?;
    let mut ifaces = vec![usb, debug];
    ifaces.sort();
    assert_eq!(m.control_kind_interface_kinds(lock), ifaces.as_slice());
    let flash = m.attack_type("flash-read-out").ok_or("flash")?;
    let high = m.feasibility_level("high").ok_or("high")?;
    assert_eq!(m.control_kind_rules(lock), &[ControlRule::new(flash, high)]);
    Ok(())
}

#[test]
fn translated_label_and_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let mut def = base()?;
    def.element_names.control_kinds = vec![ok_kind("secure-boot")];
    let mut ja = PackTranslationDef::default();
    ja.control_kinds
        .insert(s("secure-boot"), s("セキュアブート"));
    def.element_names.translations.insert(s("ja"), ja);
    let m = Methodology::new(def)?;
    let k = m.control_kind("secure-boot").ok_or("no secure-boot")?;
    assert_eq!(
        m.control_kind_label_for_lang(k, "ja"),
        Some("セキュアブート")
    );
    assert_eq!(
        m.control_kind_label_for_lang(k, "fr"),
        Some("Label secure-boot")
    );
    Ok(())
}

#[test]
fn too_many_control_kinds() -> Result<(), Box<dyn std::error::Error>> {
    let kinds = (0..=MAX_CONTROL_KINDS)
        .map(|i| ok_kind(&format!("k{i}")))
        .collect();
    assert_eq!(
        refused(kinds)?,
        MethodologyError::TooManyControlKinds { max: 64 }
    );
    Ok(())
}

#[test]
fn duplicate_control_kind() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        refused(vec![ok_kind("a"), ok_kind("a")])?,
        MethodologyError::DuplicateControlKind(s("a"))
    );
    Ok(())
}

#[test]
fn unknown_interface_kind() -> Result<(), Box<dyn std::error::Error>> {
    let k = kind("a", &["nope"], &[("debug-access", "low")]);
    assert_eq!(
        refused(vec![k])?,
        MethodologyError::UnknownControlKindInterfaceKind {
            control_kind: s("a"),
            interface_kind: s("nope"),
        }
    );
    Ok(())
}

#[test]
fn repeated_interface_kind() -> Result<(), Box<dyn std::error::Error>> {
    let k = kind("a", &["can", "can"], &[("debug-access", "low")]);
    assert_eq!(
        refused(vec![k])?,
        MethodologyError::RepeatedControlKindInterfaceKind {
            control_kind: s("a"),
            interface_kind: s("can"),
        }
    );
    Ok(())
}

#[test]
fn zero_rules() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        refused(vec![kind("a", &[], &[])])?,
        MethodologyError::ControlKindRuleCount {
            control_kind: s("a"),
            max: 16,
        }
    );
    Ok(())
}

#[test]
fn seventeen_rules() -> Result<(), Box<dyn std::error::Error>> {
    let mut def = base()?;
    add_attack_types(&mut def, 7);
    let mut k = ok_kind("a");
    k.rules = all_rules(&def, 17);
    def.element_names.control_kinds = vec![k];
    match Methodology::new(def) {
        Err(e) => assert_eq!(
            e,
            MethodologyError::ControlKindRuleCount {
                control_kind: s("a"),
                max: 16,
            }
        ),
        Ok(_) => return Err("accepted".into()),
    }
    Ok(())
}

#[test]
fn unknown_attack_type() -> Result<(), Box<dyn std::error::Error>> {
    let k = kind("a", &[], &[("nope", "low")]);
    assert_eq!(
        refused(vec![k])?,
        MethodologyError::UnknownControlKindAttackType {
            control_kind: s("a"),
            attack_type: s("nope"),
        }
    );
    Ok(())
}

#[test]
fn unknown_feasibility_level() -> Result<(), Box<dyn std::error::Error>> {
    let k = kind("a", &[], &[("debug-access", "nope")]);
    assert_eq!(
        refused(vec![k])?,
        MethodologyError::UnknownControlKindFeasibilityLevel {
            control_kind: s("a"),
            level: s("nope"),
        }
    );
    Ok(())
}

#[test]
fn limits_are_inclusive() -> Result<(), Box<dyn std::error::Error>> {
    let mut def = base()?;
    add_attack_types(&mut def, 6);
    let mut kinds: Vec<ControlKindDef> = (0..MAX_CONTROL_KINDS)
        .map(|i| ok_kind(&format!("k{i}")))
        .collect();
    kinds[0].rules = all_rules(&def, 16);
    def.element_names.control_kinds = kinds;
    let m = Methodology::new(def)?;
    assert_eq!(m.control_kinds().count(), 64);
    let first = m.control_kind("k0").ok_or("no k0")?;
    assert_eq!(m.control_kind_rules(first).len(), 16);
    Ok(())
}

/// Adds `n` attack types named `extra0`, `extra1`, ...
fn add_attack_types(def: &mut MethodologyDef, n: usize) {
    for i in 0..n {
        def.element_names.attack_types.push(aegis_model::LevelDef {
            key: s(&format!("extra{i}")),
            label: s(&format!("Extra {i}")),
        });
    }
}

/// The first `n` attack types of `def`, each mapped to `low`.
fn all_rules(def: &MethodologyDef, n: usize) -> std::collections::BTreeMap<Arc<str>, Arc<str>> {
    def.element_names
        .attack_types
        .iter()
        .take(n)
        .map(|a| (Arc::clone(&a.key), s("low")))
        .collect()
}

// Pack files (124b).

use aegis_model::LevelDef;
use common::{pack_bytes, second_def};
use std::collections::BTreeMap;

fn lv(k: &str, l: &str) -> LevelDef {
    LevelDef {
        key: k.into(),
        label: l.into(),
    }
}

fn rule_map(pairs: &[(&str, &str)]) -> BTreeMap<Arc<str>, Arc<str>> {
    pairs.iter().map(|&(a, l)| (a.into(), l.into())).collect()
}

/// The second definition with two attack types, two interface kinds, a
/// component preset, an interface preset and a `ja` label for each.
fn pack_presets() -> MethodologyDef {
    let mut d = second_def();
    let low = Arc::clone(&d.feasibility_levels[0].key);
    d.element_names.attack_types = vec![
        lv("bootloader-exploit", "Bootloader exploit"),
        lv("debug-access", "Debug access"),
    ];
    d.element_names.interface_kinds = vec![lv("debug", "Debug"), lv("usb", "USB")];
    d.element_names.control_kinds = vec![
        ControlKindDef {
            key: "secure-boot".into(),
            label: "Secure boot".into(),
            interface_kinds: Vec::new(),
            rules: rule_map(&[("bootloader-exploit", &low)]),
        },
        ControlKindDef {
            key: "debug-port-locked".into(),
            label: "Debug port locked".into(),
            interface_kinds: vec!["debug".into()],
            rules: rule_map(&[("debug-access", &low), ("bootloader-exploit", &low)]),
        },
    ];
    d.element_names.translations.insert(
        "ja".into(),
        PackTranslationDef {
            control_kinds: rule_map(&[("secure-boot", "セキュアブート")]),
            ..PackTranslationDef::default()
        },
    );
    d
}

fn pack_text(d: &MethodologyDef) -> String {
    String::from_utf8(pack_bytes(d)).expect("utf-8")
}

#[test]
fn presets_round_trip_and_the_model_accepts_them() {
    let d = pack_presets();
    let back = MethodologyDef::read_pack(pack_bytes(&d).as_slice()).expect("reads");
    assert_eq!(back, d);
    assert!(Methodology::new(back).is_ok());
}

#[test]
fn presets_write_the_same_bytes_twice() {
    let d = pack_presets();
    let first = pack_bytes(&d);
    assert_eq!(first, pack_bytes(&d));
    let back = MethodologyDef::read_pack(first.as_slice()).expect("reads");
    assert_eq!(pack_bytes(&back), first);
}

#[test]
fn presets_are_written_in_the_documented_shape() {
    let t = pack_text(&pack_presets());
    assert!(t.contains("    \"control_kinds\": [\n      {\n        \"key\": \"secure-boot\","));
    assert!(t.contains("        \"interface_kinds\": [\n          \"debug\"\n        ],\n"));
    assert!(t.contains("        \"rules\": {\n          \"bootloader-exploit\": "));
    // Rules are in key order.
    let a = t.find("\"bootloader-exploit\": \"").expect("rule");
    let b = t.find("\"debug-access\": \"").expect("rule");
    assert!(a < b);
    assert!(t.contains("\"control_kinds\": {\n          \"secure-boot\": \"セキュアブート\""));
    // Presets come after attack types and before translations.
    let at = t.find("\"attack_types\": [").expect("attack types");
    let ck = t.find("\"control_kinds\": [").expect("control kinds");
    let tr = t.find("\"translations\"").expect("translations");
    assert!(at < ck && ck < tr);
}

#[test]
fn a_component_preset_has_no_interface_kinds_line() {
    let mut d = pack_presets();
    d.element_names.control_kinds.truncate(1);
    d.element_names.translations.clear();
    let t = pack_text(&d);
    assert!(t.contains("\"secure-boot\""));
    assert!(!t.contains("interface_kinds\": [\n          \"debug\""));
    let start = t.find("\"control_kinds\"").expect("control kinds");
    assert!(!t[start..].contains("\"interface_kinds\""));
}

#[test]
fn no_presets_write_no_control_kinds() {
    assert!(!pack_text(&second_def()).contains("control_kinds"));
}
