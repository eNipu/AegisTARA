## Answers

The owner answered the spec's open questions on 2026-10-20:

1. **Hook:** confirmed. Use the provided `or_kept` / `and_kept` / `seq_and_kept` methods on `AttributeDomain`, with default bodies that return `Both`. No separate trait, no guessing by equality.
2. **When:** confirmed. Always record the trace in `evaluate`. No separate `evaluate_traced`.
3. **Ties:** confirmed. Name only the earlier child.
4. **Combined:** confirmed. When several children contribute, record `Combined` without tracking which ones. That is precise enough for v1.
5. **Text:** confirmed. Human-readable explanation text is out of scope and belongs to the UI and report jobs.
