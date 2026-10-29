# ADR 0055: Suggestion branches, ghost nodes, and Aegis-Core Gear 1 desktop integration

- Status: accepted (job 089-suggestion-branches)
- Date: 2026-10-28
- Scope: `aegis-model` (`id.rs`, `op.rs`, `suggest.rs`), `aegis-canvas` (`scene.rs`, `render.rs`), `aegis-client` (`suggest.rs`, `workspace.rs`, `view.rs`, `commands.rs`, `state.rs`, `session.rs`), `aegis-shell` (`ui/widgets.slint`, `ui/app.slint`, `controller.rs`)
- Rules: R3, R4, R6, D5, D10, P1, P4 in `docs/engineering-standards.md`
- Requirements: FND-05
- Builds on: `0013-element-provenance.md`, `0014-attack-tree-node-ops.md`, `0052-attack-tree-library.md`

## Context

Requirement `FND-05` specifies that AI-generated or assistive TARA suggestions must never mutate a `Project` directly. Instead, proposed operations must live in a separate `SuggestionBranch` with citations, rationale, and confidence, render as ghost previews on the attack-tree canvas so the analyst can inspect them in context, and enter the project's op log with `ProvenanceSource::Ai` only when the analyst explicitly accepts an individual suggestion or accepts all pending suggestions.

In parallel, the in-house `tools/aegis-ai` subproject trains the compact **Aegis-Core Gear 1** multi-head attack-tree predictor (`artifacts/gear1_model.json`) paired with a deterministic safety/schema verifier. We want `aegis-client` and `aegis-shell` to run Gear 1 inference locally with zero external runtime dependencies (`D5`), stage verified completions into a `SuggestionBranch`, render them as ghost nodes on the attack-tree canvas, and let the analyst accept or dismiss them individually or in bulk.

## Decisions

### 1. `SuggestionId` and `SuggestionBranch` in `aegis-model::suggest` (`FND-05`)

- `SuggestionId` is a typed RFC 9562 UUIDv7 newtype in `crates/aegis-model/src/id.rs`.
- `SuggestionItem` holds `id: SuggestionId`, `label: String`, `rationale: String`, `confidence_bps: u16` (`0..=10_000`), `citations: Vec<Citation>` (capped at `MAX_CITATIONS_PER_ITEM = 16`), and `ops: Vec<Op>` (capped at `MAX_OPS_PER_SUGGESTION = 64`).
- `SuggestionBranch` holds up to `MAX_SUGGESTIONS_PER_BRANCH = 64` items targeting a single `AttackTreeId` and records `base_op_count` and `source: ProvenanceSource`. While items remain in `SuggestionBranch`, the underlying `Project` is untouched.
- `SuggestionBranch::preview_tree(&self, project: &Project) -> Result<GhostPreview, SuggestionError>` clones the target `AttackTree` and applies each pending item's ops in memory, collecting `GhostTreeNode` metadata (`node_id`, `suggestion_id`, `confidence_bps`, `rationale`, `citations`) without mutating `project`.
- `SuggestionBranch::accept_one` and `accept_all` rebase `Op::AddTreeNode` child `index` and canvas `position` against the live `AttackTree` before validating against `project`, so suggestions can be accepted in any order (e.g., accepting the second suggestion before the first). Every accepted op is stamped via `Op::with_provenance(Provenance::new(actor, timestamp_ms, self.source))` (`ProvenanceSource::Ai`).
- `reject_one` and `reject_all` discard pending items without touching `project` or the undo stack.

### 2. Ghost node rendering in `aegis-canvas`

- `NodeStyle` in `crates/aegis-canvas/src/scene.rs` gains `pub ghost: bool` (defaulting to `false`).
- In `crates/aegis-canvas/src/render.rs`, incoming edges targeting a ghost node render with `Stroke::dashed(Paint::Accent, width(1.5))`, and ghost node cards render with `Paint::AccentWash` background fill, dashed `Paint::Accent` border (`Stroke::dashed(Paint::Accent, width)`), and `"ghost · step"` / `"ghost · OR"` / `"ghost · AND"` / `"ghost · SAND"` badges.
- Clicking a ghost node on the canvas selects its corresponding `SuggestionId` in `UiState::selected_suggestion` without changing the real project selection.

### 3. Pure-Rust Aegis-Core Gear 1 predictor in `aegis-client::suggest`

- `crates/aegis-client/src/suggest.rs` implements `Gear1Predictor`: a pure-Rust inference engine for the Aegis-Core Gear 1 multi-head linear model (`gate_head`, `step_head`, `factor_heads`).
- `Gear1Predictor::load_default()` loads `AEGIS_AI_MODEL_PATH` or `tools/aegis-ai/artifacts/gear1_model.json` when present on disk (enforcing `MAX_MODEL_ARTIFACT_BYTES = 16 MiB`, `R3`) and falls back seamlessly to `Gear1Predictor::builtin()` (an embedded compact Gear 1 weight artifact), keeping `aegis-client` self-contained and license-clean (`D5`).
- `suggest_tree_nodes(ws, ui)` extracts features from the selected attack tree (root goal label, target gate label, asset/threat context, and existing child labels to prevent duplicates), scores candidate attack steps from the built-in library catalog plus Gear 1's step/factor heads, verifies each candidate structurally against a cloned `Project`, and returns a populated `SuggestionBranch`.

### 4. Desktop workflow in `aegis-client` and `aegis-shell`

- `Workspace` stores `suggestions: Option<SuggestionBranch>` and automatically clears stale suggestions if the target tree is deleted or if an undo/redo invalidates the branch.
- `Command` gains `SuggestTreeNodes`, `AcceptSuggestion { id }`, `AcceptAllSuggestions`, `RejectSuggestion { id }`, and `RejectAllSuggestions`.
- `aegis-shell` adds a `"Suggest steps"` toolbar button on the Attack Trees page and an `"AI suggestions (Gear 1)"` panel in the right-hand inspector showing each pending ghost node's label, confidence percentage, catalog citation badge, rationale, and per-item `Accept` / `Dismiss` buttons alongside `Accept all` and `Dismiss all`.

## Consequences

- AI-assisted attack-tree completion works offline and deterministically in the desktop app with zero new external crate dependencies.
- Every accepted AI suggestion is recorded in the project's op log with `ProvenanceSource::Ai` and can be undone/redone as a single batch on the client's undo stack.
