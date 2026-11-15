//! Systems: a top-level component can be put in a system, moved or taken
//! out, with an exact inverse, in the snapshot, the op log and workspace
//! files (119b).

mod common;

use std::sync::Arc;

use aegis_model::{
    ComponentId, ElementId, ElementKind, LintCode, Methodology, ModelError, Op, Project,
    Provenance, StoreError,
};
use common::{builtin, component_id, element_id, read_log, save, write_log};
use serde_json::{Value, json};

const SYS_A: u64 = 1;
const SYS_B: u64 = 2;
const MISUSE: u64 = 3;

fn who() -> Provenance {
    Provenance::human("alice", 1)
}

fn add_system(n: u64, name: &str) -> Op {
    Op::AddElement {
        id: element_id(n),
        kind: ElementKind::System,
        name: name.into(),
        provenance: who(),
    }
}

fn add_component(n: u64, name: &str, parent: Option<u64>) -> Op {
    Op::AddComponent {
        id: component_id(n),
        name: name.into(),
        parent: parent.map(component_id),
        provenance: who(),
    }
}

fn put(component: u64, system: Option<u64>) -> Op {
    Op::SetComponentSystem {
        id: component_id(component),
        system: system.map(element_id),
        provenance: who(),
    }
}

/// Two systems, a misuse case, a top-level component 10, a child 11 of it
/// and a second top-level component 12.
fn project() -> (Arc<Methodology>, Project) {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    for op in [
        add_system(SYS_A, "Secret Powertrain"),
        add_system(SYS_B, "Secret Body"),
        Op::AddElement {
            id: element_id(MISUSE),
            kind: ElementKind::MisuseCase,
            name: "Secret misuse".into(),
            provenance: who(),
        },
        add_component(10, "Secret Gateway", None),
        add_component(11, "Secret HSM", Some(10)),
        add_component(12, "Secret Telematics", None),
    ] {
        p.apply(op).expect("setup");
    }
    (m, p)
}

fn system_of(p: &Project, n: u64) -> Option<ElementId> {
    let idx = p.component_idx(&component_id(n)).expect("component");
    p.component(idx).expect("live").system()
}

fn refused(p: &Project, op: Op) -> ModelError {
    let before = p.clone();
    let mut q = p.clone();
    let from_validate = p.validate(&op).expect_err("validate refuses");
    let from_apply = q.apply(op).expect_err("apply refuses");
    assert_eq!(from_validate.to_string(), from_apply.to_string());
    assert!(q == before, "a refused op changed the project");
    from_apply
}

#[test]
fn set_move_and_clear_are_undone_by_their_inverses() {
    let (_, mut p) = project();
    for (op, expect) in [
        (put(10, Some(SYS_A)), Some(SYS_A)),
        (put(10, Some(SYS_B)), Some(SYS_B)),
        (put(10, None), None),
    ] {
        let before = p.clone();
        let inverse = p.apply(op.clone()).expect("apply");
        assert_eq!(system_of(&p, 10), expect.map(element_id));
        let after = p.clone();
        p.apply(inverse.clone()).expect("undo");
        assert!(p == before, "inverse not exact");
        p.apply(op).expect("redo");
        assert!(p == after);
        assert!(matches!(inverse, Op::SetComponentSystem { .. }));
    }
}

#[test]
fn an_unknown_component_is_refused() {
    let (_, p) = project();
    let err = refused(&p, put(99, Some(SYS_A)));
    assert!(matches!(err, ModelError::UnknownComponent(id) if id == component_id(99)));
}

#[test]
fn a_child_component_cannot_have_a_system() {
    let (_, p) = project();
    let err = refused(&p, put(11, Some(SYS_A)));
    assert!(
        matches!(err, ModelError::ChildComponentSystem { component } if component == component_id(11))
    );
    // Taking a child out of no system is allowed.
    let mut q = p.clone();
    q.apply(put(11, None)).expect("clear on a child");
}

#[test]
fn an_unknown_element_is_refused() {
    let (_, p) = project();
    let err = refused(&p, put(10, Some(77)));
    assert!(matches!(err, ModelError::UnknownElement(id) if id == element_id(77)));
}

#[test]
fn a_misuse_case_is_not_a_system() {
    let (_, p) = project();
    let err = refused(&p, put(10, Some(MISUSE)));
    assert!(matches!(err, ModelError::NotASystem { element } if element == element_id(MISUSE)));
}

#[test]
fn a_blank_actor_is_refused() {
    let (_, p) = project();
    for system in [Some(SYS_A), None] {
        let err = refused(
            &p,
            Op::SetComponentSystem {
                id: component_id(10),
                system: system.map(element_id),
                provenance: Provenance::human("  ", 1),
            },
        );
        assert!(matches!(err, ModelError::EmptyActor), "{err:?}");
    }
}

#[test]
fn a_component_with_a_system_cannot_get_a_parent() {
    let (_, mut p) = project();
    p.apply(put(12, Some(SYS_A))).expect("put");
    let err = refused(
        &p,
        Op::SetComponentParent {
            id: component_id(12),
            parent: Some(component_id(10)),
            provenance: who(),
        },
    );
    assert!(
        matches!(err, ModelError::ComponentHasSystem { component } if component == component_id(12))
    );
    // Clearing the parent stays allowed.
    let mut q = p.clone();
    q.apply(Op::SetComponentParent {
        id: component_id(12),
        parent: None,
        provenance: who(),
    })
    .expect("clear parent");
}

#[test]
fn a_component_with_a_system_cannot_be_removed() {
    let (_, mut p) = project();
    p.apply(put(12, Some(SYS_A))).expect("put");
    let err = refused(
        &p,
        Op::RemoveComponent {
            id: component_id(12),
        },
    );
    assert!(
        matches!(err, ModelError::ComponentHasSystem { component } if component == component_id(12))
    );
    p.apply(put(12, None)).expect("take out");
    p.apply(Op::RemoveComponent {
        id: component_id(12),
    })
    .expect("remove");
}

#[test]
fn a_system_with_components_cannot_be_removed() {
    let (_, mut p) = project();
    p.apply(put(10, Some(SYS_A))).expect("put");
    p.apply(put(12, Some(SYS_A))).expect("put");
    let err = refused(
        &p,
        Op::RemoveElement {
            id: element_id(SYS_A),
        },
    );
    assert!(matches!(
        err,
        ModelError::SystemHasComponents { system, components: 2 } if system == element_id(SYS_A)
    ));
    // The other system holds none.
    p.apply(Op::RemoveElement {
        id: element_id(SYS_B),
    })
    .expect("remove empty system");
}

#[test]
fn error_texts_name_ids_never_names() {
    let (_, mut p) = project();
    p.apply(put(10, Some(SYS_A))).expect("put");
    let errors = [
        refused(&p, put(11, Some(SYS_A))),
        refused(&p, put(10, Some(MISUSE))),
        refused(
            &p,
            Op::RemoveComponent {
                id: component_id(10),
            },
        ),
        refused(
            &p,
            Op::RemoveElement {
                id: element_id(SYS_A),
            },
        ),
    ];
    for e in errors {
        assert!(!e.to_string().contains("Secret"), "{e}");
    }
    let id: ComponentId = component_id(10);
    assert!(
        ModelError::ChildComponentSystem { component: id }
            .to_string()
            .contains(&id.to_string())
    );
}

#[test]
fn diff_marks_a_moved_component_and_lists_an_added_system() {
    let (_, base) = project();
    let mut curr = base.clone();
    curr.apply(add_system(20, "New system")).expect("add");
    curr.apply(put(10, Some(SYS_A))).expect("put");
    let d = base.diff(&curr).expect("diff");
    assert_eq!(d.components().modified(), [component_id(10)]);
    assert!(d.components().added().is_empty());
    assert_eq!(d.elements().added(), [element_id(20)]);
}

/// Two systems and three components, two of them in systems.
fn full_project() -> (Arc<Methodology>, Project) {
    let (m, mut p) = project();
    p.apply(put(10, Some(SYS_A))).expect("put");
    p.apply(put(12, Some(SYS_B))).expect("put");
    (m, p)
}

#[test]
fn snapshot_op_log_and_workspace_files_round_trip() {
    let (m, p) = full_project();
    let bytes = save(&p);
    let loaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert!(loaded == p);
    assert_eq!(save(&loaded), bytes);
    assert_eq!(system_of(&loaded, 10), Some(element_id(SYS_A)));
    assert_eq!(system_of(&loaded, 11), None);

    let ops = [
        add_system(SYS_A, "a"),
        add_component(10, "c", None),
        put(10, Some(SYS_A)),
        put(10, None),
    ];
    let log = write_log(&m, &ops);
    assert!(String::from_utf8_lossy(&log).contains("\"set_component_system\""));
    let read: Vec<Op> = read_log(&m, &log).into_iter().map(|r| r.op).collect();
    assert_eq!(read, ops);

    let files = p.to_workspace_files().expect("write");
    let back = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("read");
    assert_eq!(save(&back), bytes);
    assert!(back == p);
}

fn edited(p: &Project, edit: impl FnOnce(&mut Vec<Value>)) -> Vec<u8> {
    let mut v: Value = serde_json::from_slice(&save(p)).expect("json");
    let list = v["components"].as_array_mut().expect("components");
    edit(list);
    serde_json::to_vec(&v).expect("bytes")
}

fn component_entry(list: &mut [Value], n: u64) -> &mut Value {
    let id = component_id(n).to_string();
    list.iter_mut()
        .find(|c| c["id"] == json!(id))
        .expect("component entry")
}

#[test]
fn a_snapshot_with_a_system_on_a_child_is_refused() {
    let (m, p) = full_project();
    let bytes = edited(&p, |list| {
        component_entry(list, 11)["system"] = json!(element_id(SYS_A).to_string());
    });
    let err = Project::load(bytes.as_slice(), m).expect_err("refused");
    assert!(
        matches!(&err, StoreError::Model(ModelError::ChildComponentSystem { component }) if *component == component_id(11)),
        "{err:?}"
    );
}

#[test]
fn a_snapshot_naming_a_missing_system_is_refused() {
    let (m, p) = full_project();
    let bytes = edited(&p, |list| {
        component_entry(list, 10)["system"] = json!(element_id(99).to_string());
    });
    let err = Project::load(bytes.as_slice(), m).expect_err("refused");
    assert!(
        matches!(&err, StoreError::Model(ModelError::UnknownElement(id)) if *id == element_id(99)),
        "{err:?}"
    );
}

#[test]
fn the_v1_fixture_saves_the_same_bytes_without_a_system_key() {
    let m = builtin();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/project-v1.json"
    ))
    .expect("fixture");
    let p = Project::load(bytes.as_slice(), m).expect("load");
    let saved = save(&p);
    assert!(
        !String::from_utf8(saved.clone())
            .expect("utf8")
            .contains("\"system\"")
    );
    let again = Project::load(saved.as_slice(), Arc::clone(p.methodology())).expect("reload");
    assert_eq!(save(&again), saved);
}

fn without_system(p: &Project) -> Vec<ComponentId> {
    p.lint()
        .iter()
        .filter_map(|f| match f.code() {
            LintCode::ComponentWithoutSystem { component } => Some(component),
            _ => None,
        })
        .collect()
}

#[test]
fn no_finding_without_systems() {
    let mut p = Project::new(builtin());
    p.apply(add_component(10, "Secret Gateway", None))
        .expect("add");
    assert!(without_system(&p).is_empty());
}

#[test]
fn top_level_components_in_no_system_are_reported_and_children_follow_their_parent() {
    let (_, mut p) = project();
    // 10 and 12 are top-level in no system; 11 is a child of 10.
    assert_eq!(without_system(&p), vec![component_id(10), component_id(12)]);
    let f = p
        .lint()
        .into_iter()
        .find(|f| matches!(f.code(), LintCode::ComponentWithoutSystem { .. }))
        .expect("finding");
    assert!(f.is_warning());

    p.apply(put(10, Some(SYS_A))).expect("put");
    // 10 is in a system, its child 11 is not reported, 12 still is.
    assert_eq!(without_system(&p), vec![component_id(12)]);
    p.apply(put(12, Some(SYS_B))).expect("put");
    assert!(without_system(&p).is_empty());
    p.apply(put(12, None)).expect("clear");
    assert_eq!(without_system(&p), vec![component_id(12)]);
}
