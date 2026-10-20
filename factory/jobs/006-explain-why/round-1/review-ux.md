VERDICT: PASS

No end-user surface in this change (no UI, CLI, or user-visible error messages; the spec defers text, the "Why?" button, and report output to later jobs). The only surface is the Rust API of `aegis-model`, reviewed as a developer-facing interface:

- Flow: one call (`evaluate`) records the trace; `derivation`, `explain`, and `why_path` read it with no extra steps. Matches the goal.
- Failure signalling: out-of-range or mismatched input returns `None` / an empty iterator, never a panic (AC15 grep is clean). Documented on each method.
- Docs: every new public item has a compiling example and states its time and space cost.
- Perf: the trace adds about 4% to `evaluate` at large and stress (1.0% of the 200 ms budget). Reads are O(1) or O(depth), nothing runs on a UI thread.

Non-blocking notes for the later UI and report jobs:
- crates/aegis-model/src/eval.rs (`explain`) — `None` covers three causes: out of range, kind mismatch, and length mismatch. A UI cannot tell "stale tree" from "bad index". Fix when the UI job needs it: return a small error enum.
- crates/aegis-model/src/eval.rs (`Derivation::Combined`) — it also means "domain does not say" for domains without `*_kept` overrides. A "Why?" panel could show "combined" for a real selection. Fix: the UI should tell users that custom domains need the `*_kept` overrides to name a deciding child.
