## Answers

The owner answered the spec's open questions on 2026-10-20:

1. **ADR:** confirmed. No ADR. Document the generator design in the test file's module docs.
2. **Bug found:** confirmed. If a property finds a real bug, fix it in `src/eval.rs` within this job, add a regression test, and record it in build.md.
3. **Runtime:** confirmed. The suite runs in under 60 s in a debug build. Longer runs are left for later.
4. **Regressions file:** confirmed. Commit `proptest-regressions/eval_properties.txt` if proptest writes one.
