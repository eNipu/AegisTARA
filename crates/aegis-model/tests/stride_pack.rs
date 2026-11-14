use aegis_model::{Methodology, MethodologyDef};

#[test]
fn stride_services_cloud_pack_is_registered_and_canonical() {
    let bytes = Methodology::stride_services_cloud_pack();
    let definition = MethodologyDef::read_pack(bytes).expect("pack definition loads");
    let mut canonical = Vec::new();
    definition
        .write_pack(&mut canonical)
        .expect("writes canonical pack");
    assert_eq!(canonical, bytes);

    let methodology = Methodology::stride_services_cloud().expect("built-in pack loads");
    assert_eq!(methodology.key(), "stride-services-cloud");
    assert_eq!(methodology.label(), "STRIDE for Services and Cloud");
    assert_eq!(methodology.feasibility_label(), "Likelihood");
    assert_eq!(methodology.threat_category_count(), 6);
    assert_eq!(methodology.catalog_scheme_count(), 3);
}

#[test]
fn stride_taxonomy_maps_each_category_to_its_security_property() {
    let methodology = Methodology::stride_services_cloud().expect("built-in pack loads");
    for (category_key, property_key) in [
        ("spoofing", "authenticity"),
        ("tampering", "integrity"),
        ("repudiation", "non-repudiation"),
        ("information-disclosure", "confidentiality"),
        ("denial-of-service", "availability"),
        ("elevation-of-privilege", "authorization"),
    ] {
        let category = methodology
            .threat_category(category_key)
            .expect("STRIDE category");
        let property = methodology.property(property_key).expect("property");
        assert!(methodology.violates(category).contains(property));
    }
}

#[test]
fn stride_pack_parses_catalog_ids_without_including_catalog_records() {
    let methodology = Methodology::stride_services_cloud().expect("built-in pack loads");
    for (scheme_key, reference) in [
        ("capec", "CAPEC-123"),
        ("mitre-attack-enterprise", "T1059"),
        ("mitre-attack-enterprise", "T1059.001"),
        ("emb3d-threat", "TID-103"),
    ] {
        let scheme = methodology.catalog_scheme(scheme_key).expect("scheme");
        let parsed = methodology
            .parse_catalog_ref(scheme, reference)
            .expect("valid external catalog ID");
        assert_eq!(
            methodology.display_catalog_ref(&parsed).to_string(),
            reference
        );
    }

    let capec = methodology.catalog_scheme("capec").expect("CAPEC scheme");
    assert!(methodology.parse_catalog_ref(capec, "CAPEC-0123").is_err());
    let attack = methodology
        .catalog_scheme("mitre-attack-enterprise")
        .expect("ATT&CK scheme");
    assert!(methodology.parse_catalog_ref(attack, "T1059.1").is_err());
}

#[test]
fn stride_pack_carries_setting_presets() {
    let m = Methodology::stride_services_cloud().expect("built-in pack loads");
    let keys: Vec<_> = m
        .control_kinds()
        .filter_map(|k| m.control_kind_key(k))
        .collect();
    assert_eq!(
        keys,
        [
            "network-segmentation",
            "mfa",
            "secrets-vault",
            "input-validation",
            "waf",
            "hardened-baseline",
            "signed-artifacts",
            "datacenter-access",
        ]
    );
    assert!(
        m.control_kinds()
            .all(|k| m.control_kind_interface_kinds(k).is_empty())
    );

    let dc = m.control_kind("datacenter-access").expect("preset");
    let rules = m.control_kind_rules(dc);
    assert_eq!(rules.len(), 1);
    assert_eq!(
        m.attack_type_key(rules[0].attack_type()),
        Some("physical-access")
    );
    assert_eq!(
        m.feasibility_level_key(rules[0].ceiling()),
        Some("very-low")
    );

    let mfa = m.control_kind("mfa").expect("preset");
    assert_eq!(m.control_kind_label_for_lang(mfa, "ja"), Some("多要素認証"));
}
