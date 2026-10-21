//! Fuzzes `OpLogReader` (the `aegis-oplog` v1 header and record decoder).
//!
//! Property: the reader ends after finitely many items and never panics;
//! every op it yields encodes again with `OpLogWriter` and reads back
//! equal; replaying the ops onto a project never panics (the model rejects
//! bad ones). Seeds: `seeds/op_log/` (the golden v1 log).

#![no_main]

use std::sync::{Arc, OnceLock};

use aegis_model::{Methodology, OpLogReader, OpLogWriter, Project};
use libfuzzer_sys::fuzz_target;

fn methodology() -> Arc<Methodology> {
    static M: OnceLock<Arc<Methodology>> = OnceLock::new();
    Arc::clone(M.get_or_init(|| {
        Arc::new(Methodology::iso21434_r155().expect("built-in methodology is valid"))
    }))
}

fuzz_target!(|data: &[u8]| {
    let m = methodology();
    let Ok(reader) = OpLogReader::open(data, Arc::clone(&m)) else {
        return;
    };
    let mut project = Project::new(Arc::clone(&m));
    let mut ops = Vec::new();
    for (n, record) in reader.enumerate() {
        assert!(n <= data.len(), "reader does not end");
        let Ok(record) = record else { continue };
        let _ = project.apply(record.op.clone());
        ops.push(record.op);
    }
    let mut writer = OpLogWriter::create(Vec::new(), Arc::clone(&m)).expect("header");
    for op in &ops {
        writer.append(op).expect("a decoded op encodes again");
    }
    let bytes = writer.into_inner().expect("flush");
    let again: Vec<_> = OpLogReader::open(bytes.as_slice(), m)
        .expect("header reads back")
        .map(|r| r.expect("record reads back").op)
        .collect();
    assert!(again == ops, "op round trip changed an op");
});
