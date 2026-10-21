VERDICT: PASS

Scope: no GUI or CLI. The user-facing surfaces are the library API, the `StoreError` messages, and `crates/aegis-model/fuzz/run.sh` with its README section. Nothing touches the UI thread, keyboard access or accessible labels. The 50 ms budget is unaffected this round: only the version check, docs and the memory bench changed (build.md, "No decoder or format behaviour changed this round except the version-0 classification").

Rework 2 UX item, checked on branch `factory/010-save-load` (commit 9948902):
- crates/aegis-model/src/store/wire.rs:646 (`FormatVersion::visit_u64`) — fixed. Only an integer above 1 gives `UnsupportedFormat { version: Some(n) }` ("a newer application may read it"). Version 0 goes through the `Less` arm, and negative or fractional versions go through `visit_i64` and `visit_f64`. All of them give `version: None`, the "not a supported AegisTARA file: wrong format name or format version" message. The next step is now right for each case: pick another file, or update the app.
- wire.rs:1327 — the unit test asserts that version 0 gives `None` and that its text never contains "newer". The doctest in `error.rs` still covers `None` and `Some(2)`.
- crates/aegis-model/src/error.rs:557 — the variant docs now describe version 0, negative and fractional as "the user picked the wrong file". They match the code.
- The "methodology key checked first" caveat is documented in the `Project::load` doc and in the ADR (build.md item 4a). A hand-edited file with the lists first may report `UnknownKey` instead of `MethodologyMismatch`. It is a typed error and no project is built.

Non-blocking:
1. crates/aegis-model/src/error.rs:631 — the `Some(v)` text says "this build reads version 1, a newer application may read it". The version number is hardcoded in the string, so it goes stale if the format reaches v2. Fix: build the text from `FORMAT_VERSION`. A test would catch the drift. Do this when v2 is introduced.
2. crates/aegis-model/src/error.rs:635 — the `None` text does not say what to do next ("choose an AegisTARA project file"). The app layer can add it. No change needed now.
