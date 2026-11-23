# ADR 0074 — Immutable Methodology Pack Pins

- **Status:** Accepted
- **Date:** 2026-11-23
- **Backlog:** `223-methodology-pins`

## Problem

Previously, snapshots, baselines, and workspace files recorded only
`methodology.key` and `methodology.version`. If a methodology pack's risk matrix,
feasibility bands, or impact ratings changed while keeping the same `key` and
`version`, reopening an old project or baseline could silently recompute different
risk levels.

## Decision

1. **Content-addressed pack digest:** `Methodology::new` computes an immutable
   `pack_digest: "sha256:<64 lowercase hex>"` over the canonical JSON written by
   `MethodologyDef::write_pack`. Any change to rules, matrices, or vocabularies
   produces a distinct digest.
2. **Persist pin across snapshots, baselines, and workspace files:** New projects
   (`Project::new`) record `pinned_digest = Some(methodology.pack_digest())`,
   which `Project::save` and `Project::to_workspace_files` (`aegis.json`) emit as
   `methodology.digest`. On load, if `methodology.digest` is present, it MUST
   match `methodology.pack_digest()` or load fails with
   `StoreError::MethodologyMismatch`.
3. **Retain historical packs by digest on the server:** `Methodologies` indexes
   active packs by `key` (for creating new projects) and retains immutable
   historical packs by `digest` (`Methodologies::add_pinned_bytes`,
   `get_by_digest`, `resolve_pin`). `Registry::load` inspects the snapshot's
   `MethodologyPin` and loads pinned projects strictly by digest—never falling
   back to today's pack for that key.
4. **No ordinary UI for changing a project's methodology (D42):** Methodology
   choice stays creation-only.

## Legacy Migration Policy

- Existing v1 files written without `methodology.digest` decode as
  `MethodologyPin::LegacyUnpinned { key, version }`, and `Project::load`
  preserves `pinned_digest = None` (re-saving byte-for-byte without fabricating
  a pin).
- `Methodologies::resolve_pin` returns `PinError::UnpinnedLegacy` for unpinned
  legacy files and `PinError::MissingPinnedPack` when a pinned digest is absent—
  never silently guessing a pack from `key`/`version`.
- Administrators explicitly verify a legacy project against its original pack
  and call `Project::pin_methodology()` to upgrade it to a pinned snapshot.
