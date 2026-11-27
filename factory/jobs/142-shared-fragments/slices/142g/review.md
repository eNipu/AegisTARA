VERDICT: PASS
## For the owner
What changed: Reference Before/After ratings now come independently from cached host-node evaluations.
Cross-tree explanations retain stable-ID boundaries, local ceilings, conditions, and nested deciding causes.
Localized fragment-reference labels were added while 142f's note and single reverse relation remain intact.
Look at: crates/aegis-client/src/inspector.rs:288 — cross-tree paths resolve each node in its own tree and compare same-mode boundary values before adding local causes.
Look at: crates/aegis-client/src/inspector.rs:865 — inherent and residual fields separately read the host reference node without leaf-API or cross-mode fallback.
Look at: crates/aegis-client/tests/inspector_view.rs:1371 — exact values, tones, labels, and navigation identities cover Planned, Implemented, Off, and nested references.
Risk: A mode or tree-index mix-up could display incorrect ratings or causes, and a boundary cap could disappear; exact independent-value and identity assertions exercise those paths, including unrated/excluded fragments and the preserved 142f relation behavior. The supplied scoped CI and focused test gate passed.
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff e4882cf..HEAD
## Findings
- none
## Lesson
none
NEXT: human
