# ADR 0041: The "numbers checked" mark

- Status: accepted (job 047-numbers-check-mark)
- Date: 2026-10-25
- Scope: `crates/aegis-client/src/{session,state}.rs`, `crates/aegis-shell/{ui/app.slint,src/controller.rs}`
- Rules: P1 (correctness before speed), A1 in `docs/engineering-standards.md`; gap G15
- Builds on: `0033-live-observers.md`

## Context

Two engines compute the risk numbers: the one in the app (for instant
feedback) and the server's (the official one). ADR 0033 compares them and
keeps the result in the view model. The window only appended "numbers checked
with the server" to the save text whenever a check had ever succeeded. That
had two faults:

1. The text stayed after later edits. At change 8 it still said "checked"
   although only change 5 had been compared.
2. An editor's own edits were never compared when the server's numbers for a
   change reached the app on the `Watch` stream a moment before the answer to
   its own `SubmitOps`. The comparison ran once, found the batch still
   pending, and was never tried again. The mark never appeared for the person
   who made the change.

## Decisions

### 1. The mark is a state, not a flag

`NumbersCheck` has four values: `Checked`, `Checking`, `ServerNumbersShown`
(this computer's numbers once differed from the server's and the server's are
shown) and `NotComparable` (another engine version on the server).
`Checked` needs three facts together: nothing is waiting to be saved, the
project is not reloading, and the last verified change is exactly the
confirmed change. Every edit therefore moves the mark to `Checking` until the
server has recomputed and agreed.

### 2. Compare again when our own batch is confirmed

The comparison is a function, `compare_server_risks`, of the newest server
numbers and the current state. It runs when the server's numbers arrive and
again when a batch is accepted. Whichever arrives second does the comparison.

### 3. It is a header status, with an icon and words

The status sits in the project header next to "Saved": a check badge for
`Checked`, a refresh arrow for `Checking`, a warning for the other two. The
colour is never the only signal; the words say the same, and the element has
an accessible label.

## Consequences

- The text of the save status no longer carries the check. "Saved" means the
  server has the change; the mark says whether the numbers agree.
- A very large project (over `risks_event_max_threats`) gets no server
  numbers, so it stays on `Checking`. The UX review should decide if that
  needs its own wording.
