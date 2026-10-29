# 0062 — Catalog packs and a STRIDE methodology for services and cloud

- **Date:** 2026-10-30
- **Status:** accepted
- **Decision:** ship metadata-only catalog packs for CAPEC, MITRE ATT&CK, and
  MITRE EMB3D, plus a STRIDE methodology pack for services and cloud that
  cites their catalog IDs. This is job 112 (`DOM-04`, `DOM-05`) after the D5
  license review.

## Context

`DOM-04` wants catalog packs with names and descriptions for CAPEC, ATT&CK,
and EMB3D "where the catalog license allows redistribution, with
attribution". `DOM-05` wants a STRIDE plus MITRE pack for services and cloud.

## Decision

- The three catalogs' terms are verifiable but each carries obligations
  (trademark treatment, notice, no endorsement, and for EMB3D a notice to
  `EMB3D@mitre.org` and a U.S. Government pricing restriction). Rather than
  copy source-catalog records, the built-in packs hold **catalog-level
  metadata only**: original names, original short descriptions, official
  links, and the verified attribution/license text. No CAPEC/ATT&CK/EMB3D
  dataset records ship, so `DOM-04`'s redistribution condition is met without
  taking on record-level obligations.
- A bounded, `deny_unknown_fields` reader
  (`crates/aegis-model/src/catalog_pack.rs`, format `aegis-catalog-pack`
  v1) validates and round-trips these packs.
- A `stride-services-cloud` methodology pack
  (`crates/aegis-model/packs/stride-services-cloud.json`) defines the six
  STRIDE categories mapped to security properties and three catalog reference
  syntaxes: `CAPEC-<n>`, MITRE ATT&CK Enterprise `T<n>[.<n>]`, and EMB3D
  `TID-<n>`. It cites the catalog IDs without embedding catalog records.
- The STRIDE pack is canonical (`write_pack` byte-for-byte matches the
  checked-in file), so signed reports and diffs stay deterministic (P4).

## Consequences

- Catalog packs are searchable metadata, not insertable attack-tree items;
  the library panel's insert flow remains attack-tree-only until the company
  library (job 087) adds catalog browsing.
- Future catalog record redistribution needs a per-source record-level review
  against the terms recorded in each pack.
