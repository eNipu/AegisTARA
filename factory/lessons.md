# Factory lessons

What the factory learned from failed gates, reviewer findings and owner reworks. The
orchestrator writes this file; agents only read the lines with their tag. One line per
lesson: the cause, not the symptom. At most 25. `seen: 3` gets `PROMOTE`, and `/factory retro`
then proposes to turn it into a check or a rule.

- L01 [builder] Run `cargo fmt --all` before every commit; the gate fails on formatting first. Evidence: jobs 128, 138 (workspace fmt failed after the code was right). seen: 2
- L02 [builder] A new `Op` must be added to `common::record_every_op`, or the `undo_every_op` test in `aegis-client` fails. Evidence: job 138 (`SetThreatScaling`). seen: 1
- L03 [builder] A new symbol used across modules needs its import where it is used; build the whole workspace, not only the crate you edited. Evidence: job 128 (`ElementRef` in `aegis-client`). seen: 1
- L04 [planner] A removal op needs its guard decided in the spec: what blocks removal and how the inverse stays exact. Evidence: job 138 needed a follow-up fix commit. seen: 1
- L05 [reviewer] Ask of every test: would it fail if the change were removed? A wait or assertion on the wrong condition passes for the wrong reason. Evidence: job 128 (find-in-page test waited on the wrong error). seen: 1
- L06 [reviewer] `Project ==` does not compare visual IDs or order. A round-trip test must compare `save()` bytes, and must include more than 100 of an element whose file name or sort key carries a zero-padded number. Evidence: job 162 slice 162b (101 trees changed IDs on read-back). seen: 1
- L07 [builder] When asked to replace a literal with a constant, change only the use that was named; two formats must not share one constant by accident. Evidence: job 162 slice 162b (workspace envelope version tied to the snapshot version). seen: 1
- L08 [planner] Before a prompt says to derive a cap from an existing cap, check that the cap exists. Evidence: job 162 slice 162c (no per-project tree cap in the model). seen: 1
- L09 [builder] A test for a new guard needs an input that only that guard refuses, and must check which error comes back. Evidence: job 162 slice 162c (260-letter path was refused without the fix). seen: 1
- L10 [planner] A crate folder outside the main workspace needs an empty `[workspace]` table to build inside a factory worktree; and check on the registry that the allowed dependencies can actually open a window before writing the prompt. Evidence: job 181 slice 181a (builder stopped, GPUI platform crate missing). seen: 1
- L11 [planner] A "Done when" item that needs clicks or screenshots cannot be met by an agent without macOS Accessibility and Screen Recording permission; plan it as an owner check from the start. Evidence: job 181 slices 181a, 181b. seen: 2
- L12 [builder] Before committing, count the changed source lines (`git diff --numstat <base>`, without tests, docs and lock files) against the slice's `max_lines`. Evidence: job 181 slices 181a, 181b (490 lines, cap 400). seen: 2
- L13 [builder] A script that copies several user-given files into one folder by file name must reject a repeated name, or one file silently replaces another and the result still verifies. Evidence: job 100 slice 100b (`scripts/offline-bundle.sh`, two `--installer` paths with one name). seen: 1
