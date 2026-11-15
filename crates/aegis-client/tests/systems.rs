//! A component in no system shows on the Problems page once the project has
//! a system (119d).

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
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

// ---- edits (119e) ----

fn workspace() -> Workspace {
    Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

fn run(ws: &mut Workspace, edit: &Edit) -> Vec<Op> {
    let ops = edit.to_ops(ws.project(), "mira", 1).expect("builds");
    ws.apply(ops.clone(), Origin::Edit).expect("applies");
    ops
}

fn add_system(ws: &mut Workspace, name: &str) -> ElementId {
    let id = ElementId::new_v7();
    run(
        ws,
        &Edit::AddSystem {
            id,
            name: name.into(),
        },
    );
    id
}

fn add_component(ws: &mut Workspace, name: &str, parent: Option<ComponentId>) -> ComponentId {
    let id = ComponentId::new_v7();
    run(
        ws,
        &Edit::AddComponent {
            id,
            name: name.into(),
            parent,
        },
    );
    id
}

fn system_of(ws: &Workspace, c: ComponentId) -> Option<ElementId> {
    let idx = ws.project().component_idx(&c).expect("idx");
    ws.project().component(idx).expect("component").system()
}

#[test]
fn add_rename_and_remove_a_system_are_one_undo_step_each() {
    let mut ws = workspace();
    let before = ws.project().clone();
    let sys = add_system(&mut ws, "Backend");
    assert_eq!(
        ws.undo_label().expect("label").imperative(),
        "add system 'Backend'"
    );
    ws.undo().expect("undo add");
    assert!(*ws.project() == before);
    ws.redo().expect("redo add");

    let added = ws.project().clone();
    run(
        &mut ws,
        &Edit::RenameSystem {
            system: sys,
            name: "Back end".into(),
        },
    );
    assert_eq!(ws.project().element(sys).expect("el").name(), "Back end");
    ws.undo().expect("undo rename");
    assert!(*ws.project() == added);

    run(&mut ws, &Edit::RemoveSystem { system: sys });
    assert!(ws.project().element(sys).is_none());
    ws.undo().expect("undo remove");
    assert!(*ws.project() == added);
}

#[test]
fn removing_a_system_takes_its_components_out_first() {
    let mut ws = workspace();
    let sys = add_system(&mut ws, "Backend");
    let other = add_system(&mut ws, "Frontend");
    let a = add_component(&mut ws, "A", None);
    let b = add_component(&mut ws, "B", None);
    let c = add_component(&mut ws, "C", None);
    run(
        &mut ws,
        &Edit::MoveToSystem {
            components: vec![a, b],
            system: Some(sys),
        },
    );
    run(
        &mut ws,
        &Edit::MoveToSystem {
            components: vec![c],
            system: Some(other),
        },
    );
    let before = ws.project().clone();
    run(&mut ws, &Edit::RemoveSystem { system: sys });
    assert!(ws.project().element(sys).is_none());
    assert_eq!(system_of(&ws, a), None);
    assert_eq!(system_of(&ws, b), None);
    assert_eq!(system_of(&ws, c), Some(other));
    assert!(ws.project().element(other).is_some());
    let label = ws.undo_label().expect("label");
    assert_eq!(
        label.past(),
        "System 'Backend' removed and 2 components now in no system"
    );
    assert!(label.is_removal());
    ws.undo().expect("undo");
    assert!(*ws.project() == before);
}

#[test]
fn moving_to_a_system_skips_children_and_components_already_there() {
    let mut ws = workspace();
    let sys = add_system(&mut ws, "Backend");
    let a = add_component(&mut ws, "A", None);
    let b = add_component(&mut ws, "B", None);
    let c = add_component(&mut ws, "C", None);
    let child = add_component(&mut ws, "Child", Some(a));
    run(
        &mut ws,
        &Edit::MoveToSystem {
            components: vec![b],
            system: Some(sys),
        },
    );
    let ops = run(
        &mut ws,
        &Edit::MoveToSystem {
            components: vec![a, b, child],
            system: Some(sys),
        },
    );
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], Op::SetComponentSystem { .. }));

    let before = ws.project().clone();
    let ops = run(
        &mut ws,
        &Edit::MoveToSystem {
            components: vec![a, b, c],
            system: None,
        },
    );
    assert_eq!(ops.len(), 2);
    let label = ws.undo_label().expect("label");
    assert!(label.is_bulk());
    assert_eq!(label.imperative(), "move 2 components");
    ws.undo().expect("undo");
    assert!(*ws.project() == before);
}

#[test]
fn a_move_that_changes_nothing_gives_no_ops() {
    let mut ws = workspace();
    let sys = add_system(&mut ws, "Backend");
    let a = add_component(&mut ws, "A", None);
    let child = add_component(&mut ws, "Child", Some(a));
    for edit in [
        Edit::MoveToSystem {
            components: vec![a],
            system: None,
        },
        Edit::MoveToSystem {
            components: vec![child],
            system: Some(sys),
        },
    ] {
        assert!(edit.to_ops(ws.project(), "mira", 1).expect("ok").is_empty());
    }
}

#[test]
fn a_parent_or_a_removal_takes_the_component_out_of_its_system() {
    let mut ws = workspace();
    let sys = add_system(&mut ws, "Backend");
    let a = add_component(&mut ws, "A", None);
    let b = add_component(&mut ws, "B", None);
    run(
        &mut ws,
        &Edit::MoveToSystem {
            components: vec![a, b],
            system: Some(sys),
        },
    );
    let before = ws.project().clone();
    let ops = run(
        &mut ws,
        &Edit::SetComponentParent {
            component: a,
            parent: Some(b),
        },
    );
    assert_eq!(ops.len(), 2);
    assert_eq!(system_of(&ws, a), None);
    ws.undo().expect("undo parent");
    assert!(*ws.project() == before);
    assert_eq!(system_of(&ws, a), Some(sys));

    run(&mut ws, &Edit::RemoveComponent { component: b });
    assert!(ws.project().component_idx(&b).is_none());
    ws.undo().expect("undo remove");
    assert!(*ws.project() == before);
    assert_eq!(system_of(&ws, b), Some(sys));
}

#[test]
fn renaming_or_removing_what_is_not_a_system_is_missing() {
    let mut ws = workspace();
    let misuse = ElementId::new_v7();
    run(
        &mut ws,
        &Edit::AddSystem {
            id: ElementId::new_v7(),
            name: "Backend".into(),
        },
    );
    ws.apply(
        vec![Op::AddElement {
            id: misuse,
            kind: ElementKind::MisuseCase,
            name: "Spoof".into(),
            provenance: Provenance::human("mira", 1),
        }],
        Origin::Edit,
    )
    .expect("misuse case");
    for id in [misuse, ElementId::new_v7()] {
        let rename = Edit::RenameSystem {
            system: id,
            name: "X".into(),
        };
        assert_eq!(
            rename.to_ops(ws.project(), "mira", 1),
            Err(EditError::Missing)
        );
        let remove = Edit::RemoveSystem { system: id };
        assert_eq!(
            remove.to_ops(ws.project(), "mira", 1),
            Err(EditError::Missing)
        );
    }
}
