//! A component in no system shows on the Problems page once the project has
//! a system (119d).

use std::sync::Arc;

use aegis_client::pages;
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{ComponentId, ElementId, ElementKind, Methodology, Op, Provenance};

#[test]
fn a_component_in_no_system_is_a_problem_once_a_system_exists() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let who = || Provenance::human("mira", 1);
    let comp = ComponentId::new_v7();
    ws.apply(
        vec![Op::AddComponent {
            id: comp,
            name: "Secret Gateway".into(),
            parent: None,
            provenance: who(),
        }],
        Origin::Edit,
    )
    .expect("component");
    assert!(
        pages::problems(&ws)
            .iter()
            .all(|r| r.target != Some(Selection::Component(comp)))
    );

    ws.apply(
        vec![Op::AddElement {
            id: ElementId::new_v7(),
            kind: ElementKind::System,
            name: "Powertrain".into(),
            provenance: who(),
        }],
        Origin::Edit,
    )
    .expect("system");
    let rows: Vec<_> = pages::problems(&ws)
        .into_iter()
        .filter(|r| r.target == Some(Selection::Component(comp)))
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].text.contains("\"Secret Gateway\""),
        "{}",
        rows[0].text
    );
    assert!(rows[0].text.contains("is in no system"));
    assert!(!rows[0].error);
}
