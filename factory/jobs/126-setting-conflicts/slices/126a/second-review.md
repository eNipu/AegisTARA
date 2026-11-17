VERDICT: CHANGES (tests only; the shipped code stays)
Reviewed: 0182a05..360c9ff (merged as aa6604a), against spec.md and prompts/126a.md.
Ran on master: control_kinds, setting_conflicts, pack_limits (46 passed), aegis-client pages (23 passed).

## Code: accepted

The source change (220 lines) is correct and as small as the spec allows.
- Pack field, cap, writer skip-when-empty, two-pass key resolution, the three refusals: all as specified.
- The lint is symmetric, deduplicated, order-independent, a warning, and counts direct applications only.
- No new op, no snapshot change, shipped packs untouched.

## Findings: the tests do not check what the spec and the prompt asked for

1. AC4 "kinds on different targets" has no test. The block commented "different components" in `no_findings_cases` puts `fused-a` on one component and `fused-c` on the other, a pair that does not conflict anyway. Put `fused-a` and `open-b` on two different components and expect no finding.
2. AC4 "custom setting without a preset" has no test. Add a control with no kind next to a preset one on the same target.
3. Prompt case "presets that do not conflict on one target" has no test. Put `fused-a` and a non-conflicting preset on the same component.
4. No test checks which presets a finding names. The prompt said to compare the whole collected list; the tests assert only the count and `first < second`. Compare against the exact expected `LintCode` values.
5. AC3 "names the preset": `conflicts_refused` matches `{ .. }`. Assert `control_kind` and `other`, so swapped fields fail.
6. AC6 "both preset labels": `problems_setting_conflicts_row` checks only the fixed phrase. Assert both labels are in the row text, so a "?" fallback fails.
7. AC1 forward reference is covered only by accident (the `setting_conflicts` fixture). Add the variant the prompt asked for in `control_kinds.rs`: the first preset lists the second.
8. A target with three presets where two pairs conflict is untested, and the spec is unclear on it. The spec says "exactly one warning for that target"; the code gives one per conflicting pair. Keep the code (each row names two presets, so one row per pair is the useful reading), correct the spec sentence, and add the test.

## Tidy while there

- `pack_limits.rs`, `control_kind_conflicts_cap`: delete the thinking-aloud comment before `over_cap`.
- `setting_conflicts.rs`: the `add_setting` helper exists but most tests repeat its three ops by hand. Use the helper; the file should shrink from 500 lines to about 200. Drop `mod common;` if nothing uses it.

## Not for this job

- The Problems row uses the default-language preset label, like the neighbouring `ControlRulesDifferFromKind` row. Fix both together when Problems rows get translated.

## Process note

The first review returned "Findings: none" and "Risk: none" on a slice the plan marked high risk, built by the free fallback model. It did not compare the test list in the prompt with the tests written. Suggested lesson: [reviewer] tick off every test case the prompt lists against a test that would fail without it; a green gate does not show a missing case.

NEXT: builder (follow-up 126b-setting-conflict-tests)
