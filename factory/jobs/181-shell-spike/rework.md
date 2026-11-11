## Answers
Owner, 2026-11-10. All three answers match the planner's recommendations in spec.md, so the prompts stand unchanged.
1. Pass bar: the engineering-standards budgets on the Windows laptop (first frame <= 1.0 s, canvas frame p95 <= 16 ms panning 100,000 nodes, resident memory <= 300 MB); over RDP it starts, draws and pans with frame interval p95 <= 50 ms. Binary size is recorded only.
2. If GPUI fails: this job builds no second toolkit. Record the failure, Slint keeps shipping, a follow-up job spikes iced.
3. Windows and RDP: the owner runs measure.ps1 between 181c and 181d and commits the filled-in RESULTS.md.

## Answers, changed 2026-11-11
Owner: answer 3 is replaced. The owner has no Windows machine and must set up a VM first, so the Windows laptop and Windows-over-RDP measurements are deferred to a backlog job. Job 181 finishes on the macOS numbers alone:
- RESULTS.md: the two Windows rows and Windows checklist columns say "not measured (deferred to backlog: no Windows machine yet)" instead of TODO(owner). The macOS checklist cells that nobody reported stay "not checked".
- The decision record (181d) gives a provisional result: what macOS shows against the budgets, and states plainly that the pass bar in answer 1 is defined on the Windows laptop and RDP and is therefore not yet met or failed. D37's "Spike result" line says the same.
- No toolkit is finally chosen or rejected by this job. Job 182 may only start after the Windows follow-up, unless the owner says otherwise.
