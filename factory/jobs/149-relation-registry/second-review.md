VERDICT: CHANGES (fixed on review/post-129-fixes, 95481c8)
Reviewed: cd26f5e..8157742 against spec.md, NFR-12 and D30.

Fine: the registry shape, the scan-based reverse lookup and the bidirectional test.

Fixed:
1. Wrong target kind. `control-targets` and `step-via` accept a component or an interface (`ItemRef`), but their rows said `to: Component`. `candidates()` (the picker from job 150) therefore never offered interfaces. `Relation::to` is now a slice of kinds.
2. Missing rows. The model has three links with no row: data flow to interface, interface to its owning component, and trust boundary to its member components. All three are added, with labels in en/ja/pseudo and with link/unlink edits where one exists.
3. The fixture never used an interface target, so the "one instance of every row" test could not catch fault 1. The fixture now covers interface targets, boundaries and a flow interface, and a new test checks that the picker offers interfaces.

Process note: the planner wrote the list of keys into the spec, and the builder and reviewer checked against that list instead of against the model's actual link fields.
