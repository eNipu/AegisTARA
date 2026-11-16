# Build 119c: Derive the systems of every element

Commit: 4a62bfc Derive the systems of every element (119c)

Files:
- crates/aegis-model/src/system.rs (new: `SystemSet`, `SystemUse`, `Project::component_system`, `Project::system_use`)
- crates/aegis-model/src/lib.rs (`mod system;`, `pub use system::{SystemSet, SystemUse};`)
- crates/aegis-model/src/element.rs (one sentence on `MAX_SYSTEMS` with the measured time)
- crates/aegis-model/Cargo.toml (`[[bench]] name = "systems"`, `harness = false`)
- crates/aegis-model/benches/systems.rs (new)
- crates/aegis-model/tests/systems_properties.rs (new)

Done when:
1. `cargo test --locked -p aegis-model --test systems_properties`: ok, 6 passed (3 proptests over both methodologies, 3 plain tests).
2. `cargo test --locked -p aegis-model --bench systems`: ok, bench ran once.
3. `cargo bench -p aegis-model --bench systems`: median 24.39 µs (fastest 22.49 µs, slowest 67.99 µs, 100 samples), well under 5 ms. Written in the `MAX_SYSTEMS` comment as "24 µs (median)".
4. `cargo test --locked -p aegis-model --doc`: ok, 707 passed.
Also: `cargo fmt --all` clean; `scripts/ci.sh crates/aegis-model`: fmt, clippy, test all ok; `cargo check --locked --workspace --all-targets` ok.

Notes:
- Changed source lines outside tests and benches: 356 (system.rs 349, lib.rs 2, element.rs 1, Cargo.toml 4), under the 400 cap.
- `control_targets` yields `ItemRef` (IDs), so `system_use` maps each target back through `component_idx` / `interface_idx`. Trees and goals are computed in the threat pass, before the control pass. A tree with an empty set is skipped in the mitigation pass (adding an empty set changes nothing).
- `component_system` returns `None` if the walk exceeds `MAX_COMPONENT_DEPTH + 1` steps or meets a missing index (cannot happen in a valid project).
- `SystemSet::rank` (private) gives an empty set for rank >= 64, so a project over the cap cannot shift out of range.
- The random generator rarely builds component hierarchies, so the property builder (`build`) adds two top-level components and four children of picked components after the random ops, then adds three systems and spreads them over the top-level components by the bits of a random `u64`. I checked the tests catch a wrong rule: with the component rule changed to ignore the parent, `rules_hold_with_systems` and `removing_a_system_touches_only_its_components` failed; the code was then restored and the regression file from that run deleted.
- The removal property checks the `diff`: only the system's components are modified, the system is the only element removed, every other kind's diff is empty, and `risk_deltas` is empty; the inverses in reverse give `==` the project before.
- `rules_hold_while_replaying` runs the checks after every op and every undo through `common::replay`, which covers "with no systems, every set is empty and `systems()` is empty".

NEXT: reviewer
