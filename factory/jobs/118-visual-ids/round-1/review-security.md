# Security Review — 118-visual-ids (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## Security Evaluation
- No new trust boundary, network path, or external input surface. Visual IDs
  are derived from the in-memory arena, never from user-supplied text.
- The persisted `visual_id` is decoded through `VisualPrefix::parse_num`,
  which is length-capped (prefix + 2..10 ASCII digits) and rejects non-digits
  and redundant leading zeros, so a crafted snapshot cannot allocate or
  overflow. `claim_visual` rejects duplicate visual numbers in a snapshot.
- The `next_visual` counter uses `saturating_add`, so a maliciously high
  stored number cannot wrap the counter and reuse IDs.
- CSV/XLSX output reuses the existing formula-guard and escaping paths
  (`csv_field`, `xlsx` guards); adding a leading `ID` column does not change
  those guards. HTML report cells are built through the existing escaping
  renderer, not string-concatenated into markup.
- Visual IDs are identifiers, not secrets; they never appear in logs, tokens,
  or key material. `scripts/deny.sh` passes (no new dependencies).
