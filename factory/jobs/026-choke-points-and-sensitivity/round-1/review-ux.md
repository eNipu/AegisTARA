# UX Review — 026-choke-points-and-sensitivity (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## Workflow check
- Gives analysts two direct decision-support rankings: which leaf steps appear in the most minimal attack paths (`ChokePoint` with `cuts_hit`, `total_cuts`, `fraction_bps`), and which unverified controls deliver the largest risk reduction across the project (`ControlEffect`).
