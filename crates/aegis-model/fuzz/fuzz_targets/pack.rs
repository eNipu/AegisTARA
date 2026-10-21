//! Fuzzes `Methodology::read_pack` (the `aegis-methodology-pack` v1
//! decoder, ADR 0011).
//!
//! Property: any input either fails with a `PackError`, or loads a
//! methodology whose definition writes and reads back equal, with the same
//! bytes on a second write. `MethodologyDef::read_pack` and
//! `Methodology::read_pack` agree on whether the input is a valid pack. A
//! panic anywhere is a bug. Seeds: `seeds/pack/` (the built-in pack and the
//! second test methodology).

#![no_main]

use aegis_model::{Methodology, MethodologyDef};
use libfuzzer_sys::fuzz_target;

fn write(def: &MethodologyDef) -> Vec<u8> {
    let mut out = Vec::new();
    def.write_pack(&mut out).expect("write_pack into a Vec");
    out
}

fuzz_target!(|data: &[u8]| {
    let loaded = Methodology::read_pack(data);
    let Ok(def) = MethodologyDef::read_pack(data) else {
        assert!(loaded.is_err(), "read_pack disagrees with MethodologyDef::read_pack");
        return;
    };
    let valid = Methodology::new(def.clone());
    assert!(valid.is_ok() == loaded.is_ok(), "read_pack disagrees with new");
    if let Ok(m) = valid {
        // Every valid definition round-trips (the caps are at least the
        // model's limits).
        let bytes = write(&def);
        assert!(write(&def) == bytes, "write_pack is not deterministic");
        let again = MethodologyDef::read_pack(bytes.as_slice()).expect("a written pack reads");
        assert!(again == def, "write/read round trip changed the definition");
        let reloaded = Methodology::read_pack(bytes.as_slice()).expect("a written pack loads");
        assert!(reloaded == m, "write/read round trip changed the methodology");
    }
});
