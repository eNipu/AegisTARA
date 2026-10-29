# Security Review — 111-cra-pack (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## Security Evaluation
- `eu-cra.json` is derived strictly from public EU Regulation (EU) 2024/2847 (Cyber Resilience Act Annex I) text and verified by a pinned SHA-256 content hash at compile/test time (`built_in_eu_cra_pack_parses_and_verifies_hash`).
