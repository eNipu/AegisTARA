VERDICT: PASS

No product user-facing change. Developer-facing surface (bench commands) checked: two documented commands (`cargo bench --bench parents_memory`, `--bench parents_time`) in the bench doc comments and ADR 0004; memory bench prints a Markdown table and on failure prints "fact failed: ..." to stderr and exits non-zero. No UI, no accessibility or 50 ms concerns.

Minor, non-blocking: crates/aegis-model/benches/parents_memory.rs:159 — "fact failed: {f}" is terse; ensure f names the expected relation and the cases (fix: append "see ADR 0004" for next step).
