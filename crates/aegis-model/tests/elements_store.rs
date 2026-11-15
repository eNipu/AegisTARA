//! Generic elements in the `aegis-project` snapshot and in workspace files.

mod common;

use std::fmt;
use std::sync::Arc;

use aegis_model::{
    AttrValue, ElementKind, Limit, LinkTarget, MAX_ELEMENTS_PER_KIND, Methodology, ModelError, Op,
    Project, Provenance, StoreError, ThreatId, VocabKind, WorkspaceFile, WorkspaceProblem,
};
use common::save;
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use serde_json::{Value, json};

const MU: ElementKind = ElementKind::MisuseCase;
const MISUSE_FILE: &str = "systems/main/misuse.json";

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

fn set(n: u64, key: &str, value: AttrValue) -> Op {
    Op::SetElementAttribute {
        id: common::element_id(n),
        key: key.into(),
        value: Some(value),
        provenance: who(),
    }
}

/// A project with `n` threat scenarios (`threat_id(1000 + i)`) on one asset.
fn project_with_threats(n: u64) -> (Arc<Methodology>, Project) {
    let m = common::builtin();
    let mut p = Project::new(Arc::clone(&m));
    let category = m.impact_categories().next().expect("category");
    let level = m.levels(category).next().expect("level");
    let asset = common::asset_id(1);
    let damage = common::damage_id(2);
    p.apply(Op::AddAsset {
        id: asset,
        name: "ECU".into(),
        properties: m.all_properties(),
        provenance: who(),
    })
    .expect("asset");
    p.apply(Op::AddDamageScenario {
        id: damage,
        name: "Harm".into(),
        impacts: vec![(category, level)],
        provenance: who(),
    })
    .expect("damage");
    for i in 0..n {
        p.apply(Op::AddThreatScenario {
            id: th(i),
            name: "Threat".into(),
            asset,
            category: m.threat_categories().next().expect("threat category"),
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: who(),
        })
        .expect("threat");
    }
    (m, p)
}

fn th(i: u64) -> ThreatId {
    common::threat_id(1000 + i)
}

fn link(n: u64, i: u64) -> Op {
    Op::LinkElement {
        id: common::element_id(n),
        target: LinkTarget::Threat(th(i)),
        provenance: who(),
    }
}

fn unlink(n: u64, i: u64) -> Op {
    Op::UnlinkElement {
        id: common::element_id(n),
        target: LinkTarget::Threat(th(i)),
        provenance: who(),
    }
}

fn apply_all(p: &mut Project, ops: Vec<Op>) {
    for op in ops {
        p.apply(op).expect("apply");
    }
}

fn load(bytes: &[u8], m: &Arc<Methodology>) -> Result<Project, StoreError> {
    Project::load(bytes, Arc::clone(m))
}

/// Threats, and misuse cases with a level, a choice and a text attribute
/// and links; added out of ID order.
fn rich_project() -> (Arc<Methodology>, Project) {
    let (m, mut p) = project_with_threats(3);
    apply_all(
        &mut p,
        vec![
            add(3, "Thief relays the key fob signal"),
            add(1, "Insider flashes old firmware"),
            add(2, "Bare case"),
            set(3, "note", AttrValue::Text("seen in 2024".into())),
            set(3, "cal", AttrValue::Level("cal-2".into())),
            set(3, "reach", AttrValue::Choice("remote".into())),
            set(1, "cal", AttrValue::Level("cal-4".into())),
            link(3, 2),
            link(3, 0),
            link(1, 1),
        ],
    );
    (m, p)
}

#[test]
fn snapshot_round_trip_with_elements() {
    let (m, p) = rich_project();
    let bytes = save(&p);
    let back = load(&bytes, &m).expect("load");
    assert_eq!(back, p);
    assert_eq!(save(&back), bytes);
    assert_eq!(back.element_count(MU), 3);
    let e = back.element(common::element_id(3)).expect("element");
    assert_eq!(e.provenance(), &who());
    assert_eq!(e.attribute("cal"), Some(&AttrValue::Level("cal-2".into())));
    assert_eq!(
        back.element_visual_id(common::element_id(3)).as_deref(),
        Some("MU00")
    );
}

/// The top-level keys of a JSON object, in file order.
fn top_keys(bytes: &[u8]) -> Vec<String> {
    struct Keys;
    impl<'de> Visitor<'de> for Keys {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an object")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<String>, A::Error> {
            let mut keys = Vec::new();
            while let Some(k) = map.next_key::<String>()? {
                map.next_value::<IgnoredAny>()?;
                keys.push(k);
            }
            Ok(keys)
        }
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    de.deserialize_map(Keys).expect("object")
}

#[test]
fn snapshot_shape() {
    let (_, p) = rich_project();
    let bytes = save(&p);
    assert_eq!(
        top_keys(&bytes).last().map(String::as_str),
        Some("elements")
    );
    let v: Value = serde_json::from_slice(&bytes).expect("json");
    let kinds = v["elements"].as_object().expect("elements object");
    assert_eq!(kinds.keys().collect::<Vec<_>>(), ["misuse-case"]);
    let list = kinds["misuse-case"].as_array().expect("list");
    let ids: Vec<String> = (1..=3).map(|n| common::element_id(n).to_string()).collect();
    let got: Vec<&str> = list.iter().map(|e| e["id"].as_str().expect("id")).collect();
    assert_eq!(got, ids);
    let e3 = &list[2];
    assert_eq!(e3["visual_id"], "MU00");
    assert_eq!(e3["name"], "Thief relays the key fob signal");
    assert_eq!(
        e3["attributes"],
        json!({
            "cal": { "level": "cal-2" },
            "note": { "text": "seen in 2024" },
            "reach": { "choice": "remote" },
        })
    );
    assert_eq!(
        e3["links"],
        json!([{ "threat": th(0).to_string() }, { "threat": th(2).to_string() }])
    );
    assert_eq!(e3["provenance"]["actor"], "alice");
    // Attribute keys in byte order in the file.
    let text = std::str::from_utf8(&bytes).expect("utf8");
    let pos = |s: &str| text.find(s).expect(s);
    assert!(pos(r#""cal":{"level":"cal-2"}"#) < pos(r#""note":"#));
    assert!(pos(r#""note":"#) < pos(r#""reach":"#));
    // An element with no attributes and no links has neither key.
    let bare = list[1].as_object().expect("object");
    assert_eq!(bare["visual_id"], "MU02");
    assert!(!bare.contains_key("attributes"));
    assert!(!bare.contains_key("links"));
    assert!(list[0].get("links").is_some());
}

#[test]
fn projects_without_elements_save_no_elements_key() {
    let m = common::builtin();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/project-v1.json"
    ))
    .expect("fixture");
    let p = load(&bytes, &m).expect("load");
    let v: Value = serde_json::from_slice(&save(&p)).expect("json");
    assert!(v.get("elements").is_none());

    let (_, mut p) = project_with_threats(1);
    let before = save(&p);
    p.apply(add(1, "Short-lived")).expect("add");
    assert_ne!(save(&p), before);
    p.apply(Op::RemoveElement {
        id: common::element_id(1),
    })
    .expect("remove");
    assert_eq!(save(&p), before);
}

/// The saved bytes of `p` (which has no elements) with `elements` appended
/// as the last key.
fn with_elements(p: &Project, elements: &str) -> Vec<u8> {
    let text = String::from_utf8(save(p)).expect("utf8");
    let body = text.trim_end().strip_suffix('}').expect("object");
    format!("{body},\"elements\":{elements}}}").into_bytes()
}

fn element_json(n: u64, visual: &str, attributes: &str, links: &str) -> String {
    let mut s = format!(
        r#"{{"id":"{}","visual_id":"{visual}","name":"Case {n}""#,
        common::element_id(n)
    );
    if !attributes.is_empty() {
        s.push_str(&format!(r#","attributes":{attributes}"#));
    }
    if !links.is_empty() {
        s.push_str(&format!(r#","links":{links}"#));
    }
    s.push('}');
    s
}

fn misuse(elements: &[String]) -> String {
    format!(r#"{{"misuse-case":[{}]}}"#, elements.join(","))
}

fn load_with(elements: &str) -> Result<Project, StoreError> {
    let (m, p) = project_with_threats(1);
    load(&with_elements(&p, elements), &m)
}

#[test]
fn snapshot_accepts_a_written_element() {
    // The refusals below start from this accepted shape.
    let p = load_with(&misuse(&[element_json(
        1,
        "MU05",
        r#"{"cal":{"level":"cal-2"}}"#,
        &format!(r#"[{{"threat":"{}"}}]"#, th(0)),
    )]))
    .expect("load");
    assert_eq!(
        p.element_visual_id(common::element_id(1)).as_deref(),
        Some("MU05")
    );
}

#[test]
fn snapshot_refusals() {
    assert!(matches!(
        load_with(r#"{"nope":[]}"#),
        Err(StoreError::UnknownKey { kind: VocabKind::ElementKind, key }) if &*key == "nope"
    ));
    assert!(matches!(
        load_with(r#"{"misuse-case":[],"misuse-case":[]}"#),
        Err(StoreError::Decode { .. })
    ));
    for visual in ["A00", "MU7"] {
        assert!(
            matches!(
                load_with(&misuse(&[element_json(1, visual, "", "")])),
                Err(StoreError::InvalidId)
            ),
            "{visual}"
        );
    }
    assert!(matches!(
        load_with(&misuse(&[
            element_json(1, "MU03", "", ""),
            element_json(2, "MU03", "", "")
        ])),
        Err(StoreError::DuplicateEntry)
    ));
    let stranger = common::threat_id(77);
    assert!(matches!(
        load_with(&misuse(&[element_json(
            1,
            "MU00",
            "",
            &format!(r#"[{{"threat":"{stranger}"}}]"#)
        )])),
        Err(StoreError::Model(ModelError::UnknownThreatScenario(t))) if t == stranger
    ));
    assert!(matches!(
        load_with(&misuse(&[
            element_json(1, "MU00", "", ""),
            element_json(1, "MU01", "", "")
        ])),
        Err(StoreError::Model(ModelError::DuplicateElement(id))) if id == common::element_id(1)
    ));
    assert!(matches!(
        load_with(&misuse(&[element_json(
            1,
            "MU00",
            r#"{"Bad":{"text":"x"}}"#,
            ""
        )])),
        Err(StoreError::Model(ModelError::InvalidAttributeKey { element }))
            if element == common::element_id(1)
    ));
    let extra = format!(
        r#"{{"id":"{}","visual_id":"MU00","name":"x","extra":1}}"#,
        common::element_id(1)
    );
    assert!(matches!(
        load_with(&misuse(&[extra])),
        Err(StoreError::Decode { .. })
    ));
}

#[test]
fn snapshot_caps() {
    let many = |n: usize| -> Vec<String> {
        (0..n)
            .map(|i| element_json(i as u64 + 1, &MU.visual_id(i as u32), "", ""))
            .collect()
    };
    let p = load_with(&misuse(&many(MAX_ELEMENTS_PER_KIND))).expect("at the cap");
    assert_eq!(p.element_count(MU), MAX_ELEMENTS_PER_KIND);
    assert!(matches!(
        load_with(&misuse(&many(MAX_ELEMENTS_PER_KIND + 1))),
        Err(StoreError::LimitExceeded(Limit::Elements { max })) if max == MAX_ELEMENTS_PER_KIND
    ));

    let attrs = |n: usize| -> String {
        let pairs: Vec<String> = (0..n)
            .map(|i| format!(r#""a{i}":{{"text":"x"}}"#))
            .collect();
        format!("{{{}}}", pairs.join(","))
    };
    let p = load_with(&misuse(&[element_json(1, "MU00", &attrs(32), "")])).expect("32");
    let e = p.element(common::element_id(1)).expect("element");
    assert_eq!(e.attributes().len(), 32);
    assert!(matches!(
        load_with(&misuse(&[element_json(1, "MU00", &attrs(33), "")])),
        Err(StoreError::LimitExceeded(Limit::ElementAttributes {
            max: 32
        }))
    ));

    let links = |n: u64| -> String {
        let items: Vec<String> = (0..n)
            .map(|i| format!(r#"{{"threat":"{}"}}"#, common::threat_id(50_000 + i)))
            .collect();
        format!("[{}]", items.join(","))
    };
    assert!(matches!(
        load_with(&misuse(&[element_json(1, "MU00", "", &links(257))])),
        Err(StoreError::LimitExceeded(Limit::ElementLinks { max: 256 }))
    ));
    // 256 links pass the cap; these threats are not in the file.
    assert!(matches!(
        load_with(&misuse(&[element_json(1, "MU00", "", &links(256))])),
        Err(StoreError::Model(ModelError::UnknownThreatScenario(t)))
            if t == common::threat_id(50_000)
    ));
}

// ---------------------------------------------------------------------------
// Workspace files.
// ---------------------------------------------------------------------------

fn files(p: &Project) -> Vec<WorkspaceFile> {
    p.to_workspace_files().expect("workspace files")
}

fn read(files: &[WorkspaceFile], m: &Arc<Methodology>) -> Result<Project, StoreError> {
    Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(m),
    )
}

fn json_of(f: &WorkspaceFile) -> Value {
    serde_json::from_slice(&f.bytes).expect("json")
}

#[test]
fn workspace_round_trip_with_150_misuse_cases() {
    let (m, mut p) = project_with_threats(4);
    // Added in reverse ID order, so visual numbers do not follow IDs.
    for n in (1..=150).rev() {
        p.apply(add(n, &format!("Case {n}"))).expect("add");
        if n % 3 == 0 {
            p.apply(link(n, n % 4)).expect("link");
        }
        if n % 5 == 0 {
            p.apply(set(n, "cal", AttrValue::Level("cal-1".into())))
                .expect("set");
        }
    }
    let all = files(&p);
    let holders: Vec<&WorkspaceFile> = all
        .iter()
        .filter(|f| json_of(f).get("elements").is_some())
        .collect();
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].path, MISUSE_FILE);
    let v = json_of(holders[0]);
    assert_eq!(v["kind"], "misuse-case");
    assert_eq!(v["elements"].as_array().map(Vec::len), Some(150));
    let back = read(&all, &m).expect("read back");
    assert_eq!(back, p);
    assert_eq!(save(&back), save(&p));
    let mut reversed = all.clone();
    reversed.reverse();
    assert_eq!(save(&read(&reversed, &m).expect("reversed")), save(&p));
}

#[test]
fn workspace_has_no_misuse_file_without_elements() {
    let (_, p) = project_with_threats(2);
    let all = files(&p);
    assert!(all.iter().all(|f| f.path != MISUSE_FILE));
    assert!(all.iter().all(|f| json_of(f).get("elements").is_none()));
}

/// One asset, one damage scenario, threats `th(0)` and `th(1)`, misuse
/// cases 1 and 2 both linking `th(0)`; nothing links `th(1)`.
fn linked_project() -> (Arc<Methodology>, Project) {
    let (m, mut p) = project_with_threats(2);
    apply_all(
        &mut p,
        vec![add(1, "First"), add(2, "Second"), link(1, 0), link(2, 0)],
    );
    (m, p)
}

fn refusal(p: &mut Project, op: Op) -> ModelError {
    let before = p.clone();
    let err = p.apply(op).expect_err("refused");
    assert_eq!(*p, before);
    err
}

/// The link guard on a loaded project: the count is rebuilt, not doubled,
/// and falls with each unlink.
fn assert_link_guard_survives(mut p: Project) {
    let remove = Op::RemoveThreatScenario { id: th(0) };
    assert_eq!(
        refusal(&mut p, remove.clone()),
        ModelError::ThreatLinkedFromElements {
            threat: th(0),
            elements: 2
        }
    );
    p.apply(unlink(1, 0)).expect("unlink 1");
    assert_eq!(
        refusal(&mut p, remove.clone()),
        ModelError::ThreatLinkedFromElements {
            threat: th(0),
            elements: 1
        }
    );
    p.apply(unlink(2, 0)).expect("unlink 2");
    p.apply(remove).expect("remove the unlinked threat");
    assert!(p.threat_scenario_idx(&th(0)).is_none());
    p.apply(Op::RemoveThreatScenario { id: th(1) })
        .expect("a threat nothing links is not blocked");
}

#[test]
fn loaded_snapshot_refuses_to_remove_a_linked_threat() {
    let (m, p) = linked_project();
    let back = load(&save(&p), &m).expect("load");
    assert_link_guard_survives(back);
}

#[test]
fn loaded_workspace_refuses_to_remove_a_linked_threat() {
    let (m, p) = linked_project();
    let back = read(&files(&p), &m).expect("read");
    assert_link_guard_survives(back);
}

fn rewrite(files: &mut [WorkspaceFile], path: &str, edit: impl FnOnce(&mut Value)) {
    let f = files.iter_mut().find(|f| f.path == path).expect("file");
    let mut v = json_of(f);
    edit(&mut v);
    f.bytes = serde_json::to_vec(&v).expect("encode");
}

#[test]
fn workspace_refusals() {
    let (m, p) = linked_project();
    let good = files(&p);
    assert!(read(&good, &m).is_ok());

    let mut all = good.clone();
    rewrite(&mut all, MISUSE_FILE, |v| v["kind"] = json!("assets"));
    assert!(matches!(
        read(&all, &m),
        Err(StoreError::WorkspaceEnvelope { path, problem: WorkspaceProblem::Kind })
            if path == MISUSE_FILE
    ));

    let mut all = good.clone();
    rewrite(&mut all, MISUSE_FILE, |v| v["assets"] = json!([]));
    assert!(matches!(
        read(&all, &m),
        Err(StoreError::WorkspaceUnknownField { path, field })
            if path == MISUSE_FILE && field == "assets"
    ));

    let mut all = good.clone();
    rewrite(&mut all, MISUSE_FILE, |v| v["elements"] = json!({}));
    assert!(matches!(
        read(&all, &m),
        Err(StoreError::WorkspaceEnvelope { path, problem: WorkspaceProblem::Json })
            if path == MISUSE_FILE
    ));

    let stranger = common::threat_id(77);
    let mut all = good;
    rewrite(&mut all, MISUSE_FILE, |v| {
        v["elements"][0]["links"] = json!([{ "threat": stranger.to_string() }]);
    });
    match read(&all, &m) {
        Err(StoreError::InWorkspace { path, source }) => {
            assert_eq!(path, MISUSE_FILE);
            assert!(matches!(
                *source,
                StoreError::Model(ModelError::UnknownThreatScenario(t)) if t == stranger
            ));
        }
        other => panic!("unexpected {other:?}"),
    }
}

// Systems (119a).

fn add_system(n: u64, name: &str) -> Op {
    Op::AddElement {
        id: common::element_id(n),
        kind: ElementKind::System,
        name: name.into(),
        provenance: who(),
    }
}

#[test]
fn systems_round_trip_in_the_snapshot() {
    let m = common::builtin();
    let mut p = Project::new(Arc::clone(&m));
    apply_all(
        &mut p,
        vec![add_system(2, "Gateway"), add_system(1, "Brake")],
    );
    let bytes = save(&p);
    let v: Value = serde_json::from_slice(&bytes).expect("json");
    let kinds = v["elements"].as_object().expect("elements object");
    assert_eq!(kinds.keys().collect::<Vec<_>>(), ["system"]);
    assert_eq!(kinds["system"].as_array().map(Vec::len), Some(2));
    let back = load(&bytes, &m).expect("load");
    assert_eq!(back, p);
    assert_eq!(back.element_count(ElementKind::System), 2);
    assert_eq!(save(&back), bytes);
}

#[test]
fn op_log_round_trips_a_system_add() {
    let m = common::builtin();
    let ops = vec![add_system(1, "Brake")];
    let bytes = common::write_log(&m, &ops);
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    assert!(text.contains(r#""kind":"system""#), "{text}");
    let read: Vec<Op> = common::read_log(&m, &bytes)
        .into_iter()
        .map(|r| r.op)
        .collect();
    assert_eq!(read, ops);
}
