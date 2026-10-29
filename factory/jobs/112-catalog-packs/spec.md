# Spec — 112-catalog-packs

## Title
Catalog packs and a STRIDE plus MITRE pack (DOM-04, DOM-05)

## Design Reference
- Reference: docs/adr/0062-catalog-packs-and-stride-methodology.md, docs/decisions.md D5
- Affected crates/paths: crates/aegis-model/src/catalog_pack.rs, crates/aegis-model/packs/catalogs/{capec,mitre-attack,emb3d}.json, crates/aegis-model/packs/stride-services-cloud.json, crates/aegis-model/src/methodology/stride.rs, crates/aegis-model/tests/{catalog_pack,stride_pack}.rs

## Problem & Goal
Provide catalog packs with names, descriptions, and attribution for CAPEC, ATT&CK, and EMB3D after a D5 license review, and a STRIDE methodology pack for services and cloud that cites them.

## Acceptance Criteria
1. Metadata-only catalog packs (no source-catalog records) with verified attribution and terms; 2. a bounded, validated reader (`aegis-catalog-pack` v1); 3. a canonical `stride-services-cloud` methodology pack citing CAPEC/ATT&CK/EMB3D reference syntaxes; 4. tests for metadata, canonical serialization, and catalog-ID parsing.
