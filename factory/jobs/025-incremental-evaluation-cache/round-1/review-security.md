# Security Review — 025-incremental-evaluation-cache (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## Security checklist
- **Cache poisoning / stale risk prevention**: Failed `Project::apply` calls never mutate `ProjectEvalCache`. Control updates (`SetControlStatus`, `SetStepControls`, `RemoveControl`) invalidate only residual caches while preserving inherent caches; structural or rating updates invalidate both inherent and residual caches for the affected tree and its linked threats.
- **Resource bounds (R3/R4)**: Cache entries are bounded by `MAX_ATTACK_TREES` and `MAX_THREAT_SCENARIOS`; no unbounded growth is possible.
