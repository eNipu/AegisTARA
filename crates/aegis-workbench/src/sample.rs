//! The in-memory project the skeleton opens: the client sample plus two
//! systems with one component each and one component in no system, built
//! through `Edit`s like a user would.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::sample::sample_ops;
use aegis_client::workspace::Workspace;
use aegis_model::{ComponentId, ElementId, Methodology, Project};

/// The name shown for the sample project.
pub const SAMPLE_NAME: &str = "Sample project";

/// The workspace key of the sample project (layout memory).
pub const SAMPLE_KEY: &str = "sample";

const ACTOR: &str = "sample";

fn run(project: &mut Project, edit: &Edit) -> Option<()> {
    for op in edit.to_ops(project, ACTOR, 1).ok()? {
        project.apply(op).ok()?;
    }
    Some(())
}

fn build() -> Option<Project> {
    let m = Arc::new(Methodology::iso21434_r155().ok()?);
    let mut project = Project::new(Arc::clone(&m));
    for op in sample_ops(&m, ACTOR, 1)? {
        project.apply(op).ok()?;
    }
    let (gateway, brake) = (ElementId::new_v7(), ElementId::new_v7());
    for (id, name) in [(gateway, "Gateway"), (brake, "Brake ECU")] {
        let name = name.into();
        run(&mut project, &Edit::AddSystem { id, name })?;
    }
    let parts = [
        ("Central gateway", Some(gateway)),
        ("Brake controller", Some(brake)),
        ("Telematics unit", None),
    ];
    for (name, system) in parts {
        let id = ComponentId::new_v7();
        let (name, parent) = (name.into(), None);
        run(&mut project, &Edit::AddComponent { id, name, parent })?;
        if system.is_some() {
            let components = vec![id];
            run(&mut project, &Edit::MoveToSystem { components, system })?;
        }
    }
    Some(project)
}

/// The sample workspace, with an empty undo stack.
///
/// # Panics
///
/// Never in practice: the built-in pack and the fixed edits are checked by
/// `tests/sample.rs`, so a failure here is a bug in this file.
#[must_use]
pub fn workbench_sample() -> Workspace {
    let project = build().expect("the built-in sample builds");
    Workspace::new(project, 0)
}
