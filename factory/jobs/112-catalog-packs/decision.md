# Decision — 112-catalog-packs

## Decision
Ship metadata-only catalog packs and a STRIDE methodology pack. See ADR 0062.

## License review (D5)
- CAPEC, MITRE ATT&CK, and MITRE EMB3D terms are permissive (non-exclusive,
  royalty-free) but impose attribution/trademark/notice obligations; EMB3D
  adds a notice to `EMB3D@mitre.org` and a U.S. Government pricing limit.
- The built-in packs therefore contain catalog-level metadata only, not
  dataset records, and retain the verified attribution and terms text.

## Key choices
- Format `aegis-catalog-pack` v1, `deny_unknown_fields`, HTTPS-only URLs.
- `stride-services-cloud` defines six STRIDE categories and three reference
  syntaxes; it is byte-canonical for deterministic reports and diffs (P4).

## UI decision (round 2)
Catalogs are searchable metadata, not insertable items (ADR 0062), so they
live in a **Catalogs (reference only)** section inside the attack tree library
panel, next to the insert flow but not part of it. The section is built inside
the existing library panel so it moves unchanged when job 117 turns the panel
into a right-panel slot. The STRIDE methodology pack is selectable in the
new-project methodology picker (and hosted by the server) wherever
methodologies are picked.
