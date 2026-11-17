VERDICT: CHANGES (data loss on undo; fixed on review/post-129-fixes, 6f48e6a)
Reviewed: 276be4b..8837af1 against spec.md and FND-09.

Fine: the v1 snapshot bytes are unchanged, the object form round-trips, IDs are compared by public ID, and the op log handles the new op.

Fixed:
1. Data loss on undo. `UnlinkDamage` quietly deleted the link's rationale, and its inverse `LinkDamage` could not restore it. `RemoveThreatScenario` lost every link rationale the same way. The same applied to `Edit::RemoveDamage`. The model now refuses these while a rationale exists (`DamageLinkHasRationale`). The client edits clear the rationale first, in the same undo step. Tests cover the model guard and undo for all three edits.

Process note: the same fault as in job 145 (an inverse that cannot carry the new field). Lesson candidate: when a field is added to an element, check every removal and unlink inverse, not only the new op's inverse.
