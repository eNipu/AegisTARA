//! The process map (UI-18): one box per sidebar group, with the group's
//! element count, its lint findings and the pack's clause references.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::pages::process_map;
use aegis_client::view::{Page, PageGroup};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{AssetId, Methodology, MethodologyDef};

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), "mira", 1_792_956_600_000)
        .expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn box_of(ws: &Workspace, group: PageGroup) -> aegis_client::pages::ProcessMapBox {
    process_map(ws)
        .into_iter()
        .find(|b| b.group == group)
        .expect("a box per group")
}

#[test]
fn every_group_has_a_box_with_a_count_and_an_accessible_name() {
    let mut ws = Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")));
    let boxes = process_map(&ws);
    assert_eq!(boxes.len(), PageGroup::ALL.len(), "one box per group");
    assert_eq!(
        boxes.iter().map(|b| b.group).collect::<Vec<_>>(),
        PageGroup::ALL.to_vec(),
        "sidebar order"
    );
    for b in &boxes {
        assert_eq!(b.target, Page::first_in_group(b.group));
        assert_eq!(b.count, 0, "an empty project has no elements");
        assert_eq!((b.findings, b.errors), (0, 0));
        assert_eq!(b.mark, "✓");
        assert!(b.clause_refs.is_empty(), "the builtin packs declare none");
        assert_eq!(
            b.accessible_name,
            format!("{} {} {}", b.label, b.count, b.mark)
        );
        assert!(b.accessible_name.contains(&b.label));
        assert!(b.accessible_name.contains(&b.count.to_string()));
        assert!(b.accessible_name.contains(&b.mark));
    }

    // One asset: the assets-and-threats box counts it and carries the warning
    // that it has no threat scenario.
    apply(
        &mut ws,
        &Edit::AddAsset {
            id: AssetId::new_v7(),
            name: "Brake request on CAN-2".into(),
            properties: vec![0],
        },
    );
    let at = box_of(&ws, PageGroup::AssetsThreats);
    assert_eq!(at.count, 1);
    assert_eq!(at.findings, 1);
    assert_eq!(at.errors, 0);
    assert_eq!(at.mark, "⚠ 1");
    assert_eq!(at.target, Page::Assets, "the group opens its first page");
    assert!(at.accessible_name.contains("Assets and threats"));
    assert!(at.accessible_name.contains('1'));
    assert!(at.accessible_name.contains("⚠ 1"));

    // No finding is a stylised mark too, not a bare count.
    assert_eq!(box_of(&ws, PageGroup::Impact).mark, "✓");
}

#[test]
fn clause_refs_are_the_groups_refs_unique_and_in_page_order() {
    let mut def =
        MethodologyDef::read_pack(Methodology::iso21434_r155_pack().as_bytes()).expect("pack");
    def.clause_refs = [
        (
            "assets".into(),
            vec!["ISO 21434 RQ-09-01".into(), "CRA Annex I 1.2".into()],
        ),
        ("threats".into(), vec!["CRA Annex I 1.2".into()]),
        ("risks".into(), vec!["ISO 21434 RQ-10-02".into()]),
    ]
    .into_iter()
    .collect();
    let m = Arc::new(Methodology::new(def).expect("valid def with clause refs"));
    let ws = Workspace::empty(m);

    assert_eq!(
        box_of(&ws, PageGroup::AssetsThreats).clause_refs,
        ["ISO 21434 RQ-09-01", "CRA Annex I 1.2"],
        "assets then threats, the shared ref listed once"
    );
    assert_eq!(
        box_of(&ws, PageGroup::Risk).clause_refs,
        ["ISO 21434 RQ-10-02"]
    );
    assert!(
        box_of(&ws, PageGroup::ItemDefinition)
            .clause_refs
            .is_empty(),
        "pages without refs contribute nothing"
    );
}

#[test]
fn every_page_has_a_unique_key() {
    let keys: Vec<&str> = Page::ALL.iter().map(|p| p.key()).collect();
    assert_eq!(keys.len(), Page::ALL.len());
    let mut unique = keys.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "{keys:?}");
    assert_eq!(Page::Overview.key(), "overview");
    assert_eq!(Page::Risks.key(), "risks");
    assert_eq!(Page::Admin.key(), "admin");
}
