//! Immutable methodology pack pins (223-methodology-pins, ADR 0074).

use std::sync::Arc;

use aegis_model::{
    Methodology, MethodologyDef, MethodologyPin, Project, StoreError, inspect_methodology_pin,
};

fn modified_matrix_same_key_and_version() -> (Arc<Methodology>, Arc<Methodology>) {
    let original_def =
        MethodologyDef::read_pack(Methodology::iso21434_r155_pack().as_bytes()).expect("read pack");
    let mut altered_def = original_def.clone();
    // Keep the exact same key and version, but flip one risk matrix cell.
    assert_eq!(altered_def.key, original_def.key);
    assert_eq!(altered_def.version, original_def.version);
    let highest = altered_def
        .risk_levels
        .last()
        .expect("risk levels")
        .key
        .clone();
    let lowest = altered_def
        .risk_levels
        .first()
        .expect("risk levels")
        .key
        .clone();
    let cell = &mut altered_def.impact_categories[0].risk_matrix[0][0];
    *cell = if *cell == lowest { highest } else { lowest };
    (
        Arc::new(Methodology::new(original_def).expect("original")),
        Arc::new(Methodology::new(altered_def).expect("altered")),
    )
}

#[test]
fn changed_matrix_with_same_key_and_version_is_rejected_on_load_and_workspace_import() {
    let (pack_a, pack_b) = modified_matrix_same_key_and_version();
    assert_eq!(pack_a.key(), pack_b.key());
    assert_eq!(pack_a.version(), pack_b.version());
    assert_ne!(
        pack_a.pack_digest(),
        pack_b.pack_digest(),
        "matrix change must produce a distinct content digest"
    );

    let project = Project::new(Arc::clone(&pack_a));
    assert_eq!(project.methodology_pin(), Some(pack_a.pack_digest()));

    let mut snapshot = Vec::new();
    project.save(&mut snapshot).expect("save");
    assert_eq!(
        inspect_methodology_pin(&snapshot).expect("inspect"),
        MethodologyPin::Pinned {
            key: pack_a.key().to_owned(),
            version: pack_a.version().to_owned(),
            digest: pack_a.pack_digest().to_owned(),
        }
    );

    // Loading with the exact pinned pack succeeds and preserves the pin.
    let reloaded = Project::load(snapshot.as_slice(), Arc::clone(&pack_a)).expect("load pack_a");
    assert_eq!(reloaded.methodology_pin(), Some(pack_a.pack_digest()));

    // Loading with a modified pack that shares key+version MUST fail.
    let err = Project::load(snapshot.as_slice(), Arc::clone(&pack_b)).expect_err("must reject");
    assert!(
        matches!(err, StoreError::MethodologyMismatch { .. }),
        "expected MethodologyMismatch, got {err:?}"
    );

    // Export/import via workspace files preserves the pin and rejects pack_b.
    let files = project.to_workspace_files().expect("to_workspace_files");
    let pairs = || files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice()));
    let imported =
        Project::from_workspace_files(pairs(), Arc::clone(&pack_a)).expect("import pack_a");
    assert_eq!(imported.methodology_pin(), Some(pack_a.pack_digest()));

    let ws_err =
        Project::from_workspace_files(pairs(), Arc::clone(&pack_b)).expect_err("must reject");
    assert!(
        matches!(
            ws_err,
            StoreError::MethodologyMismatch { .. } | StoreError::WorkspaceMethodology { .. }
        ),
        "expected MethodologyMismatch or WorkspaceMethodology on workspace import, got {ws_err:?}"
    );
}

#[test]
fn legacy_unpinned_fixture_preserves_unpinned_state_until_explicitly_pinned() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let legacy_bytes = std::fs::read("tests/fixtures/project-v1.json").expect("v1 fixture");

    assert_eq!(
        inspect_methodology_pin(&legacy_bytes).expect("inspect legacy"),
        MethodologyPin::LegacyUnpinned {
            key: m.key().to_owned(),
            version: m.version().to_owned(),
        }
    );

    let mut project = Project::load(legacy_bytes.as_slice(), Arc::clone(&m)).expect("load legacy");
    assert_eq!(
        project.methodology_pin(),
        None,
        "loading an unpinned legacy fixture must not fabricate a pin"
    );

    let mut resaved = Vec::new();
    project.save(&mut resaved).expect("resave legacy");
    assert_eq!(resaved, legacy_bytes);

    // Explicit pinning upgrades the project to a pinned snapshot.
    project.pin_methodology();
    assert_eq!(project.methodology_pin(), Some(m.pack_digest()));
    let mut pinned = Vec::new();
    project.save(&mut pinned).expect("save pinned");
    assert!(matches!(
        inspect_methodology_pin(&pinned).expect("inspect pinned"),
        MethodologyPin::Pinned { digest, .. } if digest == m.pack_digest()
    ));
}
