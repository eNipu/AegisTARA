//! Fuzzes `Project::from_workspace_files` (the workspace file reader, ADR 0065).
//!
//! The input is a file set: it is split on the byte `0x00` into chunks, and in
//! each chunk the text up to the first `\n` is the path and the rest is the
//! file. A chunk whose path is not UTF-8 is skipped.
//!
//! Property: any input either fails with a `StoreError` or returns a project
//! whose workspace files read back to an equal project, with the same
//! `save()` bytes (`Project ==` does not compare visual IDs or order, rule
//! L6). A panic anywhere is a bug. Seeds: `seeds/workspace/` (the fixture
//! `project-v1.json`, written as workspace files in two orders).

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

/// Splits the input into `(path, file)` pairs, as described above.
fn file_set(data: &[u8]) -> Vec<(&str, &[u8])> {
    data.split(|b| *b == 0)
        .filter_map(|chunk| {
            let (path, rest) = match chunk.iter().position(|b| *b == b'\n') {
                Some(at) => (chunk.get(..at)?, chunk.get(at + 1..).unwrap_or_default()),
                None => (chunk, &[][..]),
            };
            Some((std::str::from_utf8(path).ok()?, rest))
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let m = methodology();
    let files = file_set(data);
    if let Ok(project) = Project::from_workspace_files(files, Arc::clone(&m)) {
        let written = project
            .to_workspace_files()
            .expect("a loaded project writes its workspace files");
        let again = Project::from_workspace_files(
            written
                .iter()
                .map(|f| (f.path.as_str(), f.bytes.as_slice())),
            Arc::clone(&m),
        )
        .expect("written workspace files read back");
        assert!(again == project, "workspace round trip changed the project");
        let (mut a, mut b) = (Vec::new(), Vec::new());
        project.save(&mut a).expect("a loaded project saves");
        again.save(&mut b).expect("a read-back project saves");
        assert!(a == b, "workspace round trip changed the saved bytes");
    }
});
