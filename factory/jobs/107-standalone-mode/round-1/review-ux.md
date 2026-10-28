# UX Review — 107-standalone-mode (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## UX Evaluation
- Analysts can start working immediately via the startup launcher (`New local project`, `Open local project`, `Connect to server`) or CLI (`--open`, `--new`) without database or server setup.
- Standalone projects open directly in editing mode with the edit lock already held by the local analyst.
- A local folder is git-friendly (`project.json`, `ops.jsonl`, `ops.chain`, `snapshots/`) and can be promoted to a shared server at any time with `aegis-server import <folder>`.
