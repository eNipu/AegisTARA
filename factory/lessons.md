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
