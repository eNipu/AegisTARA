//! Property tests for reading workspace files back: a random project
//! written as workspace files and read again saves to the same bytes.

mod common;

use std::sync::Arc;

use aegis_model::{Methodology, Project};
use common::{accepted, builtin, save, second, seeds};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

fn workspace_round_trip(m: &Arc<Methodology>, seeds: &[common::Seed]) -> Result<(), TestCaseError> {
    let p = accepted(m, seeds, |_, _, _| Ok(()))?;
    let files = p
        .to_workspace_files()
        .map_err(|e| TestCaseError::fail(format!("write: {e}")))?;
    let back = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(m),
    )
    .map_err(|e| TestCaseError::fail(format!("read: {e}")))?;
    prop_assert_eq!(save(&back), save(&p));
    prop_assert_eq!(&back, &p);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn workspace_round_trip_first_methodology(seeds in seeds()) {
        workspace_round_trip(&builtin(), &seeds)?;
    }

    #[test]
    fn workspace_round_trip_second_methodology(seeds in seeds()) {
        workspace_round_trip(&second(), &seeds)?;
    }
}
