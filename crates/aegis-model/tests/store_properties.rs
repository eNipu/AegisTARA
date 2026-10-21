//! Property tests for save and load over both methodologies under test:
//! snapshot round trips after every accepted op (AC2), op round trips
//! (AC7), replay from a log and from a snapshot plus a log tail (AC8), and
//! hostile input that must never panic (AC13).

mod common;

use std::sync::Arc;

use aegis_model::{Methodology, Op, OpLogReader, Project};
use common::{accepted, builtin, read_log, save, second, seeds, write_log};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

/// AC2: after every accepted op, the project loads back equal from its
/// snapshot, and saving the loaded project gives the same bytes.
fn snapshot_round_trip(m: &Arc<Methodology>, seeds: &[common::Seed]) -> Result<(), TestCaseError> {
    accepted(m, seeds, |p, _, _| {
        let bytes = save(p);
        let loaded = Project::load(bytes.as_slice(), Arc::clone(m))
            .map_err(|e| TestCaseError::fail(format!("load: {e}")))?;
        prop_assert_eq!(&loaded, p);
        prop_assert_eq!(save(&loaded), bytes);
        Ok(())
    })?;
    Ok(())
}

/// AC7: every accepted op and its inverse decode to themselves.
fn op_round_trip(m: &Arc<Methodology>, seeds: &[common::Seed]) -> Result<(), TestCaseError> {
    let mut ops = Vec::new();
    accepted(m, seeds, |_, op, inverse| {
        ops.push(op.clone());
        ops.push(inverse.clone());
        Ok(())
    })?;
    let log = write_log(m, &ops);
    let read: Vec<Op> = read_log(m, &log).into_iter().map(|r| r.op).collect();
    prop_assert_eq!(read, ops);
    Ok(())
}

/// AC8: (a) replaying the whole log onto an empty project gives the final
/// project; (b) a snapshot saved after op `cut` plus the records after it
/// gives the same project.
fn replay(m: &Arc<Methodology>, seeds: &[common::Seed], cut: usize) -> Result<(), TestCaseError> {
    let mut ops = Vec::new();
    let mut snapshots = vec![save(&Project::new(Arc::clone(m)))];
    let last = accepted(m, seeds, |p, op, _| {
        ops.push(op.clone());
        snapshots.push(save(p));
        Ok(())
    })?;
    let log = write_log(m, &ops);

    let mut replayed = Project::new(Arc::clone(m));
    for r in read_log(m, &log) {
        replayed
            .apply(r.op)
            .map_err(|e| TestCaseError::fail(format!("replay: {e}")))?;
    }
    prop_assert_eq!(&replayed, &last);

    let cut = cut % snapshots.len();
    let mut resumed = Project::load(snapshots[cut].as_slice(), Arc::clone(m))
        .map_err(|e| TestCaseError::fail(format!("load: {e}")))?;
    for r in read_log(m, &log).into_iter().skip(cut) {
        resumed
            .apply(r.op)
            .map_err(|e| TestCaseError::fail(format!("tail: {e}")))?;
    }
    prop_assert_eq!(&resumed, &last);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn snapshot_round_trip_first_methodology(seeds in seeds()) {
        snapshot_round_trip(&builtin(), &seeds)?;
    }

    #[test]
    fn snapshot_round_trip_second_methodology(seeds in seeds()) {
        snapshot_round_trip(&second(), &seeds)?;
    }

    #[test]
    fn op_round_trip_first_methodology(seeds in seeds()) {
        op_round_trip(&builtin(), &seeds)?;
    }

    #[test]
    fn op_round_trip_second_methodology(seeds in seeds()) {
        op_round_trip(&second(), &seeds)?;
    }

    #[test]
    fn replay_first_methodology(seeds in seeds(), cut in any::<usize>()) {
        replay(&builtin(), &seeds, cut)?;
    }

    #[test]
    fn replay_second_methodology(seeds in seeds(), cut in any::<usize>()) {
        replay(&second(), &seeds, cut)?;
    }
}

// ---------------------------------------------------------------------------
// AC13: hostile input.
// ---------------------------------------------------------------------------

const PROJECT_V1: &[u8] = include_bytes!("fixtures/project-v1.json");
const OPLOG_V1: &[u8] = include_bytes!("fixtures/oplog-v1.jsonl");

/// One change to a valid file.
#[derive(Clone, Debug)]
enum Edit {
    Flip { at: usize, bit: u8 },
    Insert { at: usize, byte: u8 },
    Delete { at: usize },
    Truncate { at: usize },
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<usize>(), 0u8..8).prop_map(|(at, bit)| Edit::Flip { at, bit }),
        (any::<usize>(), any::<u8>()).prop_map(|(at, byte)| Edit::Insert { at, byte }),
        any::<usize>().prop_map(|at| Edit::Delete { at }),
        any::<usize>().prop_map(|at| Edit::Truncate { at }),
    ]
}

/// Bytes worth decoding: random bytes, or a golden file with 1 to 8 edits.
fn hostile() -> impl Strategy<Value = Vec<u8>> {
    let mutated = (any::<bool>(), prop::collection::vec(edit(), 1..=8)).prop_map(|(log, edits)| {
        let mut bytes = if log { OPLOG_V1 } else { PROJECT_V1 }.to_vec();
        for e in edits {
            let n = bytes.len().max(1);
            match e {
                Edit::Flip { at, bit } => {
                    if let Some(b) = bytes.get_mut(at % n) {
                        *b ^= 1 << bit;
                    }
                }
                Edit::Insert { at, byte } => bytes.insert(at % (bytes.len() + 1), byte),
                Edit::Delete { at } => {
                    if !bytes.is_empty() {
                        bytes.remove(at % n);
                    }
                }
                Edit::Truncate { at } => bytes.truncate(at % n),
            }
        }
        bytes
    });
    prop_oneof![
        1 => prop::collection::vec(any::<u8>(), 0..512),
        3 => mutated,
    ]
}

/// Feeds `bytes` to both decoders. Neither may panic; a project that loads
/// must save and load again as an equal project; the reader must end.
fn decode_everything(bytes: &[u8]) -> Result<(), TestCaseError> {
    for m in [builtin(), second()] {
        if let Ok(p) = Project::load(bytes, Arc::clone(&m)) {
            let again = Project::load(save(&p).as_slice(), Arc::clone(&m))
                .map_err(|e| TestCaseError::fail(format!("reload: {e}")))?;
            prop_assert_eq!(again, p);
        }
        if let Ok(reader) = OpLogReader::open(bytes, Arc::clone(&m)) {
            let mut project = Project::new(Arc::clone(&m));
            let mut records = 0usize;
            for r in reader {
                records += 1;
                prop_assert!(records <= bytes.len(), "reader does not end");
                if let Ok(r) = r {
                    // Replaying hostile ops is checked by the model.
                    let _ = project.apply(r.op);
                }
            }
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn hostile_input_never_panics(bytes in hostile()) {
        decode_everything(&bytes)?;
    }
}

#[test]
fn hostile_input_never_panics_on_the_golden_files() {
    for bytes in [PROJECT_V1, OPLOG_V1] {
        decode_everything(bytes).expect("golden files decode");
    }
    assert!(Project::load(PROJECT_V1, builtin()).is_ok());
    assert!(OpLogReader::open(OPLOG_V1, builtin()).is_ok());
}
