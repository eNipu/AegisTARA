# Spec — 089-suggestion-branches

## Title
Suggestion branches, canvas ghost nodes, and Aegis-Core Gear 1 desktop integration (`ADR 0055`, `FND-05`)

## Design Reference
- Reference: `docs/adr/0055-suggestion-branches-and-gear1-ghost-nodes.md`
- Affected crates/paths: `crates/aegis-model/src/{id,op,suggest}.rs`, `crates/aegis-canvas/src/{scene,render}.rs`, `crates/aegis-client/src/{suggest,workspace,view,commands,state,session}.rs`, `crates/aegis-shell/ui/{widgets,app}.slint`, `crates/aegis-shell/src/controller.rs`

## Problem & Goal
Implement `SuggestionId` and `SuggestionBranch` (`FND-05`) in `aegis-model` so proposed AI operations are kept strictly apart from `Project` until accepted, render pending suggestions as dashed-accent ghost nodes (`NodeStyle::ghost`) on the attack-tree canvas in `aegis-canvas`, and integrate the pure-Rust Aegis-Core Gear 1 multi-head attack-tree predictor (`aegis-client::suggest`) and inspector panel into `aegis-client` and `aegis-shell`.

## Acceptance Criteria
1. `SuggestionBranch` holds up to `MAX_SUGGESTIONS_PER_BRANCH = 64` items without mutating `Project`, overlays ghost nodes on a cloned `AttackTree` via `preview_tree`, and rebases/stamps accepted items with `ProvenanceSource::Ai` in any acceptance order (`accept_one` / `accept_all`).
2. `aegis-canvas` renders ghost nodes (`NodeStyle.ghost = true`) with `Paint::AccentWash` fill, dashed `Paint::Accent` border and incoming edge, and `"ghost · ..."` badges.
3. `aegis-client::suggest` runs Aegis-Core Gear 1 inference (`tools/aegis-ai/artifacts/gear1_model.json` or built-in compact weights), verifies candidates against a cloned `Project`, and populates a `SuggestionBranch`.
4. `aegis-shell` provides `"Suggest steps"` on the Attack Trees page and an `"AI suggestions (Gear 1)"` inspector panel with `Accept all`, `Dismiss all`, and per-item `Accept` / `Dismiss` actions.
