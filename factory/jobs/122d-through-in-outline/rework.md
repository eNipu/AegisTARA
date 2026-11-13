## Orchestrator note 122db

From `slices/122da/review.md` and `slices/122da/build.md`: `to_outline_with` does not check the values it writes. A value that is empty, `-`, or contains ` | ` or a line break would not read back the same, and a line break would add a line to the outline. Slice 122db is the first to pass values in (project visual IDs and attack type keys). Planner: update `prompts/122db.md` (and `plan.json` only if the caps or allowed files must change) so that 122db refuses or never produces such values, with a test. Change nothing else.
