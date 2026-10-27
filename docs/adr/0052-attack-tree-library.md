# ADR 0052: Attack tree library, phase 0

- Status: accepted (job 057-attack-tree-library)
- Date: 2026-10-27
- Scope: `aegis-model` (`library.rs`, `packs/`, `op.rs`, `project.rs`, `lint.rs`, `store/`), `aegis-client` (`library.rs`, `commands.rs`, `tree_diff.rs`, `view.rs`), `aegis-shell` (`ui/`, `controller.rs`)
- Rules: R3, D10, D14, P4 in `docs/engineering-standards.md`
- Requirements: LIB-01, LIB-02, LIB-03, LIB-04
- Builds on: `0014-attack-tree-node-ops.md`, `0051-element-descriptions.md`, `0013-element-provenance.md`

## Context

Analysts rebuild the same attack steps (CAN denial of service, UDS misuse,
debug ports) in every project. D14 wants a library of reusable trees,
fragments and steps. Phase 0 is the smallest version that is useful and safe:
a pack format, one built-in demo pack from public sources, search, and a copy
into the project. No server, no shared library, no ISO text.

## Decisions

### 1. A pack is one versioned JSON file with a license and a hash

`{format: "aegis-library", version: 1, id, title, pack_version, license,
sources, hash, items}`. `hash` is the SHA-256 of the compact JSON of `items`;
a pack whose hash does not match is refused, so a damaged or edited file is
noticed. A pack without a license is refused. Unknown fields and unknown
versions are refused (`deny_unknown_fields`). Decode caps (R3): 4 MiB file,
5,000 items, 256 nodes per item, depth 32, text lengths. Tree walks are
iterative.

### 2. The demo pack is written from public sources

`crates/aegis-model/packs/library-demo.json` (CC0-1.0): 12 items written
from CAPEC titles and the UN R155 Annex 5 threat list. It quotes no ISO
text. Japanese aliases make the items findable in Japanese.

### 3. Search is character n-grams, deterministic, no dependency

Text is lower-cased, full-width ASCII made half-width, katakana folded to
hiragana, and cut at non-alphanumerics. Bigrams and trigrams are compared;
a query matches an item when at least half of its grams are found, scored by
the field weights (name 4, alias and tag 3, facet 2, notes and labels 1).
Equal scores keep pack order. Japanese works because n-grams need no word
splitting. Facets (STRIDE, interface, asset, attack, catalog) narrow the list
and show counts.

### 4. Insert is a copy, in one batch, in one undo step

`aegis_client::library::insert_ops` builds the ops: as a new tree, under a
gate (after its children), or in place of an unshared step (the step's
rating, factors, controls and mark are cleared first, then it is removed).
Every op carries `Provenance::catalog`; the tree's description gets an origin
note (pack, version, item, license). Whole-tree ops are capped, so the copy
is built node by node with `AddTreeNode`, at most 3 ops per node.

### 5. Ratings travel by choice and are marked for review

By default only structure is copied. With "ratings" ticked, each suggested
level is set and the step gets a review mark: `Op::SetReviewMark` on a side
set of `(tree, node)`. The linter reports `LibraryRatingNotReviewed`
(Warning) for every mark; "Reviewed" in the inspector clears it. A marked
step cannot be removed, and a tree with marks cannot be removed until they
are cleared; edits that remove steps clear the marks first in the same batch.
Marks are in the snapshot and the op log, optional and omitted when empty, so
old files load byte for byte.

### 6. Identical names are never merged silently

A copied node whose text the tree already has gets its own text (`name (2)`),
so the outline editor never sees one text twice. Sharing is the user's
choice: the panel's "Share steps the tree already has" links the existing
step (`LinkTreeNode`) instead of copying it.

## Consequences

- `Op` gets a 45th variant; snapshot and op log gain an optional field.
- The built-in pack is the only source in phase 0. Loading packs from disk,
  signing and a shared company library are later jobs; the pack format and
  the insert code do not change for them.
- Template items insert as plain text for now.
