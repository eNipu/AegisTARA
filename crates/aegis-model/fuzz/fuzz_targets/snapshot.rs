//! Fuzzes `Project::load` (the `aegis-project` v1 snapshot decoder).
//!
//! Property: any input either fails with a `StoreError` or loads a project
//! that saves and loads again as an equal project. A panic anywhere is a
//! bug. Seeds: `seeds/snapshot/` (the golden v1 snapshot).

#![no_main]

use std::sync::{Arc, OnceLock};

use aegis_model::{Methodology, Project};
use libfuzzer_sys::fuzz_target;

fn methodology() -> Arc<Methodology> {
    static M: OnceLock<Arc<Methodology>> = OnceLock::new();
    Arc::clone(M.get_or_init(|| {
        Arc::new(Methodology::iso21434_r155().expect("built-in methodology is valid"))
    }))
}

fuzz_target!(|data: &[u8]| {
    let m = methodology();
    if let Ok(project) = Project::load(data, Arc::clone(&m)) {
        let mut bytes = Vec::new();
        project.save(&mut bytes).expect("a loaded project saves");
        let again = Project::load(bytes.as_slice(), m).expect("a saved project loads");
        assert!(again == project, "save/load round trip changed the project");
    }
});
