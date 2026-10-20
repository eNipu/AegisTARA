VERDICT: PASS

No UI, CLI, or keyboard surface in this change. The only user-facing parts are the library API and two error messages, which I reviewed for the future UI that will show them.

What works:
- The flow matches the spec goal. One call, `evaluate(tree, domain, leaf)`, returns a value for every node. The doctests show the minimal use.
- Both new errors name the node by public ID and never by label (crates/aegis-model/src/error.rs:121-163):
  - "gate node {id} has no children, so it has no value"
  - "no value given for step node {id}"
- Neither error fails silently. Both are pinned by doctests.
- Nothing runs on a UI thread. `evaluate` is a single pass with no recursion. Per build.md, the medians are 1.9 ms at `large` and 20.6 ms at `stress`, well inside the 50 ms interaction budget for `large`.
- The caller supplies leaf values through a closure and gets a hard error on a missing one. This follows the owner's answers on empty gates and missing leaf values.

Minor findings, none blocking:
- crates/aegis-model/src/error.rs:140,162 — The messages say what went wrong, not what to do next. The existing `ModelError` messages are also descriptive only, so this is consistent. When the GUI shows these, it should add the action: "add a child or remove the gate" and "rate this step". — Fix: handle this in the UI layer, or add the action to the text if wanted before then.
- crates/aegis-model/src/eval.rs (the `evaluate` loop) — `evaluate` stops at the first error. An analyst with several unrated steps would fix them one at a time. This is the spec's deterministic first-error behaviour, and it is the right trade now. — Fix: later, a `validate`-style call that collects all missing leaves and empty gates, before the UI ships.
- crates/aegis-model/src/eval.rs (invariant paths) — The impossible cases return `UnknownNode(idx)`, whose message is "node index N is not in the tree". That would confuse a user if it ever surfaced, though it should be unreachable. — Fix: optional. Keep it, since the `INVARIANT` comments explain it.
- crates/aegis-model/src/eval.rs (`Evaluation::iter`) — There is no `IntoIterator for &Evaluation`, so `for (i, v) in &eval` does not compile. — Fix: optional ergonomic addition.
