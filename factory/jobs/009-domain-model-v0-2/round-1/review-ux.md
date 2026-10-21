VERDICT: PASS

No user-facing change in the UI, CLI, or API-endpoint sense. The diff is the `aegis-model` library crate (domain model, methodology data, typed ops, tests) and ADR 0008. There are no windows, menus, commands, or interactions, so keyboard reach, accessible labels, and the 50 ms UI budget do not apply here.

The only user-adjacent surface is the `ModelError` and `MethodologyError` Display text. The UI will eventually show or log these. I checked them against the spec: no entity names appear in any message (AC14, customer data), and existing variant texts are unchanged. Every failure is a typed error, and `validate` and `apply` leave the project unchanged on error, so nothing fails silently.

Advisory only (not blocking; a later UI job can wrap these):
- crates/aegis-model/src/error.rs:192 — "name is empty" does not say which name (asset, damage scenario, or threat scenario) — add the entity kind to the variant, or have the UI add context when it maps the error.
- crates/aegis-model/src/error.rs:225 — "catalog ID does not match the syntax of scheme index N" does not say what the syntax is, and the index is only meaningful to code — the UI should map the scheme index to the scheme's key and show its example or pattern.
- crates/aegis-model/src/error.rs:313 — "project capacity exceeded" does not say which limit was hit — name the entity kind, as `CapacityExceeded` does for the tree ("tree node capacity exceeded").
- crates/aegis-model/src/error.rs:292 and :298 — "this catalog ref" does not say which ref. Names are banned but the ref is not customer data, so the displayed form (for example `4.2`) could be included. The UI can do this through `display_catalog_ref`.
