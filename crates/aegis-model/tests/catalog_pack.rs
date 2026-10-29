use aegis_model::catalog_pack::{CatalogPack, CatalogPackError, MAX_CATALOG_PACK_BYTES};

#[test]
fn built_in_catalog_packs_are_metadata_only_and_retain_verified_terms() {
    let packs = [
        (
            CatalogPack::builtin_capec(),
            "capec",
            "https://capec.mitre.org/about/termsofuse.html",
            "CAPEC™ — Common Attack Pattern Enumeration and Classification",
        ),
        (
            CatalogPack::builtin_mitre_attack(),
            "mitre-attack",
            "https://attack.mitre.org/resources/legal-and-branding/terms-of-use/",
            "MITRE ATT&CK®",
        ),
        (
            CatalogPack::builtin_emb3d(),
            "emb3d",
            "https://emb3d.mitre.org/subtabs/terms-of-use.html",
            "MITRE EMB3D™",
        ),
    ];

    for (pack, key, terms_url, name) in packs {
        assert!(pack.metadata_only());
        assert_eq!(pack.catalog().key(), key);
        assert_eq!(pack.catalog().name(), name);
        assert!(!pack.catalog().description().trim().is_empty());
        assert!(pack.catalog().source_url().starts_with("https://"));
        assert_eq!(pack.catalog().terms_url(), terms_url);
        assert!(pack.catalog().attribution().contains("MITRE"));
        assert!(pack.catalog().license_terms().contains("royalty-free"));

        let encoded = pack.to_bytes().expect("serializes");
        let decoded = CatalogPack::from_bytes(&encoded).expect("round trips");
        assert_eq!(decoded, pack);
        assert!(
            String::from_utf8(encoded)
                .expect("UTF-8 JSON")
                .contains("\"metadata_only\":true")
        );
    }

    let emb3d = CatalogPack::builtin_emb3d();
    assert!(emb3d.catalog().license_terms().contains("EMB3D@mitre.org"));
    assert!(emb3d.catalog().conditions().contains("U.S. Government"));
    let attack = CatalogPack::builtin_mitre_attack();
    assert!(attack.catalog().conditions().contains("endorsement"));
}

#[test]
fn built_in_catalog_packs_load_from_the_checked_in_files() {
    for bytes in [
        CatalogPack::builtin_capec_pack().as_bytes(),
        CatalogPack::builtin_mitre_attack_pack().as_bytes(),
        CatalogPack::builtin_emb3d_pack().as_bytes(),
    ] {
        CatalogPack::from_bytes(bytes).expect("checked-in catalog pack loads");
    }
}

#[test]
fn catalog_pack_rejects_unverified_or_unbounded_payload_shapes() {
    let valid = CatalogPack::builtin_capec_pack();
    let not_metadata_only = valid.replace("\"metadata_only\": true", "\"metadata_only\": false");
    assert!(matches!(
        CatalogPack::from_bytes(not_metadata_only.as_bytes()),
        Err(CatalogPackError::Invalid(
            "built-in pack must be metadata-only"
        ))
    ));

    let insecure_url = valid.replace("https://capec.mitre.org/", "http://capec.mitre.org/");
    assert!(matches!(
        CatalogPack::from_bytes(insecure_url.as_bytes()),
        Err(CatalogPackError::Invalid("catalog URLs must use HTTPS"))
    ));

    let unknown_field = valid.replace(
        "\"metadata_only\": true",
        "\"unexpected\": true, \"metadata_only\": true",
    );
    assert!(matches!(
        CatalogPack::from_bytes(unknown_field.as_bytes()),
        Err(CatalogPackError::Decode(_))
    ));

    assert_eq!(
        CatalogPack::from_bytes(&vec![b' '; MAX_CATALOG_PACK_BYTES + 1]).err(),
        Some(CatalogPackError::TooLarge)
    );
}
