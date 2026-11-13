//! Library packs (job 057, LIB-01 and LIB-02): a versioned JSON file of
//! reusable attack tree items with a license and a hash, decoded under caps,
//! and a deterministic n-gram search with facets that works for Japanese.

use std::collections::{BTreeMap, BTreeSet};

use aegis_model::GateKind;
use aegis_model::library::{
    Facet, FacetFilter, ItemKind, ItemNode, LibraryError, LibraryIndex, LibraryItem, LibraryPack,
    MAX_LIBRARY_ITEM_DEPTH, MAX_LIBRARY_ITEM_NODES, MAX_LIBRARY_ITEMS, MAX_LIBRARY_PACK_BYTES,
    PackMeta,
};

fn step(label: &str) -> ItemNode {
    ItemNode {
        label: label.into(),
        gate: None,
        feasibility: None,
        attack_type: None,
        children: vec![],
    }
}

fn gate(kind: GateKind, label: &str, children: Vec<ItemNode>) -> ItemNode {
    ItemNode {
        label: label.into(),
        gate: Some(kind),
        feasibility: None,
        attack_type: None,
        children,
    }
}

fn item(id: &str, name: &str) -> LibraryItem {
    LibraryItem {
        id: id.into(),
        kind: ItemKind::Fragment,
        name: name.into(),
        aliases: vec![],
        tags: vec![],
        facets: BTreeMap::new(),
        notes: String::new(),
        tree: gate(GateKind::Or, name, vec![step("a"), step("b")]),
    }
}

fn meta() -> PackMeta {
    PackMeta {
        id: "test".into(),
        title: "Test pack".into(),
        pack_version: "1.0.0".into(),
        license: "CC0-1.0".into(),
        sources: "Written for the tests.".into(),
    }
}

fn pack(items: Vec<LibraryItem>) -> LibraryPack {
    LibraryPack::new(meta(), items).expect("valid pack")
}

#[test]
fn a_pack_round_trips_and_carries_a_hash_of_its_items() {
    let mut a = item("can-dos", "Denial of service on a CAN bus");
    a.facets.insert(Facet::Interface, vec!["can".into()]);
    a.tree.children[0].feasibility = Some("low".into());
    let p = pack(vec![a, item("uds", "Code execution over UDS")]);
    let bytes = p.to_bytes();
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    assert!(text.contains("\"format\":\"aegis-library\""), "{text}");
    assert!(text.contains(&format!("\"hash\":\"{}\"", p.hash())));
    assert_eq!(p.hash().len(), 64);
    let again = LibraryPack::from_bytes(&bytes).expect("reads");
    assert_eq!(again, p);
    assert_eq!(again.to_bytes(), bytes, "writing is canonical");
    assert_eq!((p.license(), p.pack_version()), ("CC0-1.0", "1.0.0"));
}

#[test]
fn a_changed_item_does_not_match_the_hash() {
    let p = pack(vec![item("a", "Original name")]);
    let tampered = String::from_utf8(p.to_bytes())
        .expect("utf8")
        .replace("Original name", "Another name");
    assert_eq!(
        LibraryPack::from_bytes(tampered.as_bytes()).err(),
        Some(LibraryError::HashMismatch)
    );
}

#[test]
fn what_is_not_a_library_pack_is_refused() {
    let good = String::from_utf8(pack(vec![item("a", "A")]).to_bytes()).expect("utf8");
    let wrong_format = good.replace("aegis-library", "aegis-project");
    assert!(matches!(
        LibraryPack::from_bytes(wrong_format.as_bytes()),
        Err(LibraryError::Unsupported)
    ));
    let wrong_version = good.replace("\"version\":1", "\"version\":2");
    assert!(matches!(
        LibraryPack::from_bytes(wrong_version.as_bytes()),
        Err(LibraryError::Unsupported)
    ));
    let unknown_field = good.replacen("{", "{\"surprise\":1,", 1);
    assert!(matches!(
        LibraryPack::from_bytes(unknown_field.as_bytes()),
        Err(LibraryError::Decode(_))
    ));
    assert!(matches!(
        LibraryPack::from_bytes(b"not json"),
        Err(LibraryError::Decode(_))
    ));
    assert!(matches!(
        LibraryPack::from_bytes(b""),
        Err(LibraryError::Decode(_))
    ));
}

#[test]
fn caps_are_enforced_before_a_pack_is_accepted() {
    // Size.
    let huge = vec![b' '; MAX_LIBRARY_PACK_BYTES + 1];
    assert_eq!(
        LibraryPack::from_bytes(&huge).err(),
        Some(LibraryError::TooLarge)
    );
    assert_eq!(
        LibraryPack::from_reader(&mut std::io::repeat(b' ')).err(),
        Some(LibraryError::TooLarge),
        "an endless reader is cut off"
    );
    // Items.
    let many: Vec<LibraryItem> = (0..=MAX_LIBRARY_ITEMS)
        .map(|i| item(&format!("i{i}"), "n"))
        .collect();
    assert!(matches!(
        LibraryPack::new(meta(), many),
        Err(LibraryError::Limit(_))
    ));
    // Nodes in one item.
    let mut big = item("big", "Big");
    big.tree = gate(
        GateKind::Or,
        "root",
        (0..MAX_LIBRARY_ITEM_NODES)
            .map(|i| step(&format!("s{i}")))
            .collect(),
    );
    assert!(
        matches!(
            LibraryPack::new(meta(), vec![big]),
            Err(LibraryError::Limit(_))
        ),
        "root plus the cap is one too many"
    );
    let mut fits = item("fits", "Fits");
    fits.tree = gate(
        GateKind::Or,
        "root",
        (0..MAX_LIBRARY_ITEM_NODES - 1)
            .map(|i| step(&format!("s{i}")))
            .collect(),
    );
    LibraryPack::new(meta(), vec![fits]).expect("exactly at the cap");
    // Depth.
    let mut deep = step("leaf");
    for i in 0..MAX_LIBRARY_ITEM_DEPTH {
        deep = gate(GateKind::And, &format!("g{i}"), vec![deep]);
    }
    let mut deep_item = item("deep", "Deep");
    deep_item.tree = deep;
    assert!(matches!(
        LibraryPack::new(meta(), vec![deep_item]),
        Err(LibraryError::Limit(_))
    ));
    // Text.
    let mut long = item("long", "x");
    long.name = "n".repeat(257);
    assert!(matches!(
        LibraryPack::new(meta(), vec![long]),
        Err(LibraryError::Limit(_))
    ));
    let mut notes = item("notes", "x");
    notes.notes = "n".repeat(4_001);
    assert!(matches!(
        LibraryPack::new(meta(), vec![notes]),
        Err(LibraryError::Limit(_))
    ));
    let mut tags = item("tags", "x");
    tags.tags = (0..33).map(|i| format!("t{i}")).collect();
    assert!(matches!(
        LibraryPack::new(meta(), vec![tags]),
        Err(LibraryError::Limit(_))
    ));
}

#[test]
fn items_must_be_well_formed_and_unique() {
    assert!(matches!(
        LibraryPack::new(meta(), vec![item("a", "A"), item("a", "B")]),
        Err(LibraryError::Invalid(_))
    ));
    assert!(matches!(
        LibraryPack::new(meta(), vec![item("", "A")]),
        Err(LibraryError::Invalid(_))
    ));
    assert!(matches!(
        LibraryPack::new(meta(), vec![item("a", "  ")]),
        Err(LibraryError::Invalid(_))
    ));
    let mut bad_char = item("a", "A");
    bad_char.tree.label = "has\u{7}bell".into();
    assert!(matches!(
        LibraryPack::new(meta(), vec![bad_char]),
        Err(LibraryError::Invalid(_))
    ));
    // A step cannot have children.
    let mut under_step = item("a", "A");
    under_step.tree = ItemNode {
        label: "s".into(),
        gate: None,
        feasibility: None,
        attack_type: None,
        children: vec![step("c")],
    };
    assert!(matches!(
        LibraryPack::new(meta(), vec![under_step]),
        Err(LibraryError::Invalid(_))
    ));
    // A rating belongs on a step.
    let mut rated_gate = item("a", "A");
    rated_gate.tree.feasibility = Some("low".into());
    assert!(matches!(
        LibraryPack::new(meta(), vec![rated_gate]),
        Err(LibraryError::Invalid(_))
    ));
    // The license must be named.
    let mut m = meta();
    m.license = String::new();
    assert!(matches!(
        LibraryPack::new(m, vec![item("a", "A")]),
        Err(LibraryError::Invalid(_))
    ));
}

fn searchable() -> LibraryPack {
    let mut dos = item("can-dos", "Denial of service on a CAN bus");
    dos.tags = vec!["flooding".into(), "bus-off".into()];
    dos.aliases = vec!["CANバスへのサービス妨害".into()];
    dos.facets.insert(Facet::Interface, vec!["can".into()]);
    dos.facets
        .insert(Facet::Stride, vec!["denial-of-service".into()]);
    dos.facets.insert(Facet::Catalog, vec!["CAPEC-125".into()]);
    let mut uds = item("uds-exec", "Code execution on an ECU over UDS");
    uds.tags = vec!["diagnostics".into(), "reflash".into()];
    uds.aliases = vec!["UDS経由のコード実行".into()];
    uds.facets
        .insert(Facet::Interface, vec!["can".into(), "obd".into()]);
    uds.facets
        .insert(Facet::Stride, vec!["elevation-of-privilege".into()]);
    let mut debug = item("debug-keys", "Extract keys through a debug port");
    debug
        .facets
        .insert(Facet::Interface, vec!["debug-port".into()]);
    debug
        .facets
        .insert(Facet::Stride, vec!["information-disclosure".into()]);
    debug.notes = "JTAG and SWD access to key storage.".into();
    pack(vec![dos, uds, debug])
}

fn names(p: &LibraryPack, hits: &[aegis_model::library::Hit]) -> Vec<String> {
    hits.iter().map(|h| p.items()[h.index].id.clone()).collect()
}

#[test]
fn search_finds_by_words_typos_tags_catalog_ids_and_notes() {
    let p = searchable();
    let ix = LibraryIndex::new(&p);
    let any = FacetFilter::default();
    assert_eq!(
        names(&p, &ix.search("denial of service", &any))
            .first()
            .map(String::as_str),
        Some("can-dos")
    );
    assert_eq!(
        names(&p, &ix.search("denail of servise", &any))
            .first()
            .map(String::as_str),
        Some("can-dos"),
        "typos still match"
    );
    assert_eq!(names(&p, &ix.search("reflash", &any)), ["uds-exec"], "tags");
    assert_eq!(
        names(&p, &ix.search("capec-125", &any)),
        ["can-dos"],
        "catalog ids"
    );
    assert_eq!(names(&p, &ix.search("JTAG", &any)), ["debug-keys"], "notes");
    assert_eq!(
        names(&p, &ix.search("DEBUG PORT", &any))
            .first()
            .map(String::as_str),
        Some("debug-keys"),
        "case"
    );
    assert!(ix.search("zzzzqqq", &any).is_empty());
    // An empty query lists everything, by name.
    assert_eq!(
        names(&p, &ix.search("", &any)),
        ["can-dos", "uds-exec", "debug-keys"]
    );
}

#[test]
fn search_works_for_japanese_without_a_word_splitter() {
    let p = searchable();
    let ix = LibraryIndex::new(&p);
    let any = FacetFilter::default();
    assert_eq!(
        names(&p, &ix.search("サービス妨害", &any))
            .first()
            .map(String::as_str),
        Some("can-dos")
    );
    assert_eq!(
        names(&p, &ix.search("妨害", &any)),
        ["can-dos"],
        "two characters are enough"
    );
    assert_eq!(
        names(&p, &ix.search("さーびす妨害", &any))
            .first()
            .map(String::as_str),
        Some("can-dos"),
        "hiragana finds katakana"
    );
    assert_eq!(
        names(&p, &ix.search("ＣＡＮバス", &any))
            .first()
            .map(String::as_str),
        Some("can-dos"),
        "full-width letters"
    );
    assert_eq!(
        names(&p, &ix.search("コード実行", &any))
            .first()
            .map(String::as_str),
        Some("uds-exec")
    );
}

#[test]
fn search_is_deterministic_and_ranks_the_closer_match_first() {
    let p = searchable();
    let ix = LibraryIndex::new(&p);
    let any = FacetFilter::default();
    let first = ix.search("can bus", &any);
    for _ in 0..5 {
        assert_eq!(ix.search("can bus", &any), first);
    }
    assert_eq!(
        names(&p, &first).first().map(String::as_str),
        Some("can-dos"),
        "name beats a facet value"
    );
    assert!(first.windows(2).all(|w| w[0].score >= w[1].score));
}

#[test]
fn facets_narrow_the_list_and_count_what_is_left() {
    let p = searchable();
    let ix = LibraryIndex::new(&p);
    let mut filter = FacetFilter::default();
    let all = ix.facet_counts(&filter);
    assert!(all.contains(&(Facet::Interface, "can".into(), 2)));
    assert!(all.contains(&(Facet::Interface, "obd".into(), 1)));
    filter.insert((Facet::Interface, "can".into()));
    assert_eq!(names(&p, &ix.search("", &filter)), ["can-dos", "uds-exec"]);
    filter.insert((Facet::Stride, "denial-of-service".into()));
    assert_eq!(names(&p, &ix.search("", &filter)), ["can-dos"]);
    // The counts follow the filter: "obd" no longer matches anything.
    let left = ix.facet_counts(&filter);
    assert!(
        left.iter()
            .all(|(f, v, _)| !(*f == Facet::Interface && v == "obd"))
    );
    // A filter and a query work together.
    let mut only_can = FacetFilter::default();
    only_can.insert((Facet::Interface, "can".into()));
    assert_eq!(
        names(&p, &ix.search("keys", &only_can)),
        Vec::<String>::new()
    );
    let _ = BTreeSet::<u8>::new();
}

#[test]
fn the_built_in_demo_pack_is_valid_open_and_useful() {
    let p = LibraryPack::builtin_demo();
    assert!(p.items().len() >= 8, "{} items", p.items().len());
    assert!(!p.license().is_empty() && !p.sources().is_empty());
    assert!(
        p.sources().contains("CAPEC") || p.sources().contains("R155"),
        "{}",
        p.sources()
    );
    let kinds: BTreeSet<_> = p.items().iter().map(|i| i.kind).collect();
    assert!(
        kinds.contains(&ItemKind::Leaf)
            && kinds.contains(&ItemKind::Fragment)
            && kinds.contains(&ItemKind::Tree)
    );
    // It round-trips and every rating key is a methodology key.
    assert_eq!(LibraryPack::from_bytes(&p.to_bytes()).expect("reads"), p);
    let m = aegis_model::Methodology::iso21434_r155().expect("builtin");
    for it in p.items() {
        let mut stack = vec![&it.tree];
        while let Some(n) = stack.pop() {
            if let Some(k) = &n.feasibility {
                assert!(m.feasibility_level(k).is_some(), "{}: {k}", it.id);
            }
            stack.extend(n.children.iter());
        }
    }
    // Japanese works on the shipped content.
    let ix = LibraryIndex::new(&p);
    assert!(
        !ix.search("サービス妨害", &FacetFilter::default())
            .is_empty()
    );
    assert!(
        !ix.search("denial of service", &FacetFilter::default())
            .is_empty()
    );
}

/// Authoring aid: after editing `packs/library-demo.json` by hand, run
/// `cargo test --test library rehash_the_demo_pack -- --ignored` to write the
/// canonical bytes with the new hash.
#[test]
#[ignore = "rewrites the demo pack file"]
fn rehash_the_demo_pack() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/library-demo.json");
    let fixed = LibraryPack::rehash(&std::fs::read(&path).expect("read")).expect("rehash");
    std::fs::write(&path, fixed).expect("write");
}

#[test]
fn rehash_fixes_only_the_hash() {
    let p = pack(vec![item("a", "Original name")]);
    let edited = String::from_utf8(p.to_bytes())
        .expect("utf8")
        .replace("Original name", "Edited name");
    let fixed = LibraryPack::rehash(edited.as_bytes()).expect("rehashed");
    let again = LibraryPack::from_bytes(&fixed).expect("now it reads");
    assert_eq!(again.items()[0].name, "Edited name");
    assert_ne!(again.hash(), p.hash());
    // Anything else wrong is still refused.
    let broken = edited.replace("aegis-library", "other");
    assert!(LibraryPack::rehash(broken.as_bytes()).is_err());
}

#[test]
fn a_step_may_suggest_an_attack_type_by_key() {
    let mut a = item("a", "A");
    a.tree.children[0].attack_type = Some("debug-access".into());
    let p = pack(vec![a]);
    let bytes = p.to_bytes();
    let again = LibraryPack::from_bytes(&bytes).expect("reads");
    assert_eq!(
        again.items()[0].tree.children[0].attack_type.as_deref(),
        Some("debug-access")
    );
    assert_eq!(again, p);
}

#[test]
fn a_bad_attack_type_is_refused_with_its_own_error() {
    let mut on_gate = item("a", "A");
    on_gate.tree.attack_type = Some("debug-access".into());
    assert_eq!(
        LibraryPack::new(meta(), vec![on_gate]).err(),
        Some(LibraryError::Invalid("a gate has an attack type"))
    );
    let mut long = item("a", "A");
    long.tree.children[0].attack_type = Some("k".repeat(65));
    assert_eq!(
        LibraryPack::new(meta(), vec![long]).err(),
        Some(LibraryError::Limit("an attack type key is too long"))
    );
    let mut blank = item("a", "A");
    blank.tree.children[0].attack_type = Some("  ".into());
    assert_eq!(
        LibraryPack::new(meta(), vec![blank]).err(),
        Some(LibraryError::Invalid("an attack type key is empty"))
    );
}

#[test]
fn steps_without_an_attack_type_do_not_write_the_field() {
    let text = String::from_utf8(pack(vec![item("a", "A")]).to_bytes()).expect("utf8");
    assert!(!text.contains("attack_type"), "{text}");
}

#[test]
fn the_demo_pack_suggests_attack_types() {
    let demo = LibraryPack::builtin_demo();
    let mut n = 0;
    for it in demo.items() {
        let mut stack = vec![&it.tree];
        while let Some(x) = stack.pop() {
            n += usize::from(x.attack_type.is_some());
            stack.extend(x.children.iter());
        }
    }
    assert!(n >= 2, "{n}");
}
