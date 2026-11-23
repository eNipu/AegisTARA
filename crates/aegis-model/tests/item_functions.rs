//! Tests for Functions of the item (`ElementKind::Function`, 160-item-functions).

use std::sync::Arc;

use aegis_model::{
    ComponentId, ElementId, ElementKind, LinkTarget, LintCode, Methodology, ModelError, Op,
    OpCodec, Project, Provenance, TargetKind,
};

#[test]
fn function_lifecycle_component_guard_lint_and_persistence() {
    let m = Arc::new(Methodology::iso21434_r155().expect("built-in"));
    let mut p = Project::new(Arc::clone(&m));

    assert_eq!(ElementKind::Function.key(), "function");
    assert_eq!(ElementKind::Function.prefix(), "F");
    assert!(ElementKind::Function.allows(TargetKind::Component));
    assert!(!ElementKind::Function.allows(TargetKind::Threat));

    let comp1 = ComponentId::new_v7();
    let comp2 = ComponentId::new_v7();
    for (id, name) in [(comp1, "Brake ECU"), (comp2, "Wheel Sensor")] {
        p.apply(Op::AddComponent {
            id,
            name: name.into(),
            parent: None,
            provenance: Provenance::default(),
        })
        .expect("comp");
    }

    let func = ElementId::new_v7();
    p.apply(Op::AddElement {
        id: func,
        kind: ElementKind::Function,
        name: "Emergency Braking".into(),
        provenance: Provenance::default(),
    })
    .expect("add function");
    assert_eq!(p.element_visual_id(func).as_deref(), Some("F00"));

    // Unimplemented function fires FunctionWithoutComponents lint.
    assert!(
        p.lint()
            .iter()
            .any(|f| f.code() == LintCode::FunctionWithoutComponents { function: func })
    );

    // Link implementing components (in reverse order; stored sorted).
    let link2 = Op::LinkElement {
        id: func,
        target: LinkTarget::Component(comp2),
        provenance: Provenance::default(),
    };
    let link1 = Op::LinkElement {
        id: func,
        target: LinkTarget::Component(comp1),
        provenance: Provenance::default(),
    };
    let inv2 = p.apply(link2.clone()).expect("link2");
    let _inv1 = p.apply(link1.clone()).expect("link1");

    // Lint clears once at least one component implements the function.
    assert!(
        !p.lint()
            .iter()
            .any(|f| f.code() == LintCode::FunctionWithoutComponents { function: func })
    );

    // Removing a component while a function links it is rejected.
    assert!(matches!(
        p.apply(Op::RemoveComponent { id: comp1 }),
        Err(ModelError::ComponentLinkedFromElements {
            component,
            elements: 1
        }) if component == comp1
    ));

    // Snapshot, OpCodec, and workspace files round-trip cleanly.
    let mut bytes = Vec::new();
    p.save(&mut bytes).expect("save");
    let reloaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("reload");
    assert_eq!(reloaded, p);

    let codec = OpCodec::new(Arc::clone(&m));
    let raw = codec.encode(&link2).expect("encode");
    assert_eq!(codec.decode(&raw).expect("decode"), link2);

    let files = p.to_workspace_files().expect("to workspace");
    assert!(files.iter().any(|f| f.path.ends_with("functions.json")));
    let from_ws = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("from workspace");
    assert_eq!(from_ws, p);

    // Unlink and verify undo restores cleanly.
    p.apply(inv2).expect("unlink comp2");
    assert_eq!(
        p.element(func).expect("func").links(),
        &[LinkTarget::Component(comp1)]
    );
}
