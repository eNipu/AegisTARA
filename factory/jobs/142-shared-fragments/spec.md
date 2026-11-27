## Title
142 Shared fragments (BLK-02; non-canvas BLK-03)

## Design Reference
- `docs/requirements.md:216-220`: BLK-01 to BLK-05.
- `docs/decisions.md:228-235`: D29.
- `docs/architecture.md:2181-2207`: III.7 reference graph, cache and MakeShared/Unshare.
- `docs/adr/0002-dag-links-and-cycle-detection.md:83-96`: Kahn cycle extraction.
- `docs/design/workbench-breakdown.md:137-138,195-198`: GPUI job 187 and retained model/client scope.
- `docs/engineering-standards.md:23-66`: P7 and recompute budget.

## Problem & Goal
A consultant rates a technical attack subtree once rather than maintaining drifting copies
in twelve threat trees. References, transitive evaluation and navigable explanations provide
one reviewable source; extraction and value-preserving Unshare allow gradual adoption and
safe independence. Exact inverses and complete storage protect the consultant's saved work.
The owner authorizes all model/client workflows here, but defers fragment canvas acceptance
to GPUI job 187. This job neither extends Slint nor builds a GPUI canvas.

## Acceptance Criteria
1. Preserve delivered 142a at `7dd201c`, gate PASS (237/400 counted lines): immutable
   `TreeRole::{Threat, Fragment}`, shared AT/TT visual sequence, stable IDs, role persistence
   in snapshot/log/workspace, legacy default Threat and prefix/role rejection. Do not rebuild
   or revise its prompt, scope, commit or gate.
2. A leaf-only fragment slot is mutually exclusive with own feasibility/factors/via/attack
   type and shared-step reference. Local conditions/mitigations remain legal. Use/Unuse
   validate live Fragment targets, 256 references/tree, depth <=8 including threat entry,
   self/cross cycles with closed stable-ID paths, and parallel references. All rejected
   mutations are atomic; incoming users protect fragment deletion; outgoing references
   protect node/tree deletion until explicitly cleared. Only Threat trees link to threats.
3. Every independently runnable state that accepts references completely persists them:
   strict optional `step_fragments`, node-ID-sorted `{node, fragment}` stable IDs, omitted
   when empty, snapshot/operation-log/workspace forward-reference replay, malformed-input
   rejection, byte-stable legacy fixtures. Structural preparation 142b exposes no op or
   public mutation capable of installing references; 142c introduces ops and all persistence
   together. Test live Use -> snapshot/workspace reload -> Unuse -> undo, with non-default
   source provenance, full Project and serialized equality.
4. Fragment Use/Unuse validate and carry provenance but do NOT change the source tree's
   provenance, exactly like shared-step reference commits. Inverses retain the supplied
   provenance. Default-provenance second-pass replay therefore cannot overwrite saved
   provenance. Other metadata/shape ops retain their existing provenance-swapping inverses.
5. Both full and cached evaluation fold the requested dependency closure once/tree/mode,
   fragments first. Referencing-node applicability and mode-specific local ceilings apply
   after the referenced root. Excluded targets lift to Excluded; invalid/unrated targets
   never invent numbers; unrelated invalid trees do not poison a valid requested tree.
   Random DAG tests independently fold fixtures (OR=max; AND/SEQ=min); cover nested shared
   steps, diamonds, local controls and every node in both modes.
6. Cache reverse indexes use stable node IDs; root changes invalidate users transitively,
   batch shared-step fan-out and recompute each affected node at most once/mode. Metadata
   recomputes zero nodes; unrelated trees remain untouched. A 50-fragment edit benchmark
   reports <=5 ms, with deterministic counter bounds and independent full-result checks.
7. Explain-why records stable cross-tree node identities in the requested mode, including
   fragment boundaries and locally deciding control/condition causes. The existing GPUI
   step inspector reads inherent/residual values from the selected tree's cached Evaluation,
   not own/shared-only leaf accessors. Test different Before/After values, reference-local
   mitigation, excluded and unrated fragments, navigation and ceiling causes.
8. Client NewFragment, Use/Unuse, RenameFragment, linked-node/tree removal, shape replacement
   and branch/move edits are exact single batches with loaded-project undo/redo coverage.
   Used in enumerates actual direct referencing nodes through the single read-only
   FRAGMENT_USED_IN registry relation (Tree -> Step); no STEP_FRAGMENT relation exists.
   A Fragment tree has exactly one forward Used in row, and a referencing step exactly
   one reverse Fragment row with its navigable Tree target and TT ID/root shared_note.
   EN/JA/pseudo tests assert exact localized labels (never Needed by fallback), row
   direction/count, independently named direct stable-ID users, no duplicates/transitive
   users, ordinary/shared-step regressions and Unuse/undo/redo updates.
9. `Edit::MakeSharedFragment { tree, node, id }` is the fragment branch of the documented
   MakeShared workflow; existing shared-step MakeShared remains unchanged. Selection means
   a node plus its reachable descendant DAG, including a selected root or multiply-parented
   node. Create a NEW Fragment tree with fresh copied node IDs, preserve internal DAG
   sharing/order, ratings, conditions, mitigations, review marks and nested references.
   Replace the selected host node in place with a reference retaining its stable ID, label
   and all incoming edges. Descendants still reachable outside selection remain in the host;
   only unreachable originals are removed. Host tree role/visual ID/threat links/tree-level
   metadata never change. One undo restores all original IDs, ordering, provenance, values
   and references, removes the created fragment, and preserves pre-existing undo history.
10. `Edit::Unshare` dispatches shared-step or fragment unsharing. Fragment Unshare materializes
    the target's CURRENT DAG into ordinary editable host nodes, recursively copying nested
    fragments and shared-step rated fields. No fragment/shared-step slots remain in the
    copied region. Preserve both mode values and exclusion/unrated states, ordered DAG edges,
    ratings/factors/via/types, review marks, and all applicable tree/reference/leaf conditions
    and local mitigations. Reference overlays distribute over copied leaves; equal-control
    ceilings merge by the tighter value. Source fragments/shared blocks are NOT deleted or
    changed. Retain the selected host node ID/label and incoming edges even at the tree root.
    Later source rating/shape edits do not affect the copy. One undo restores the exact
    pre-Unshare Project/cache and one redo restores identical copied IDs. This differs from
    explicit UnuseFragment, which merely detaches to an unrated leaf.
11. Existing tree/node/op/condition/mitigation caps apply to workflows. Build large trees
    with bounded AddAttackTree plus node/link ops, not an oversized whole-tree op. Preflight
    expansion and merged metadata limits before excessive allocation; over-limit batches
    fail without partial mutation, pending submission or undo-history change. No arbitrary
    small-subtree limit may substitute for existing caps. Exercise >32-node workflows and
    high fan-out, outside-shared descendants, nested references, depth/limit rejection.
12. Fragments table/folder under project SharedAnalysis uses TT IDs, root names, cached
    feasibility and read-only Used in. Existing GPUI TableTab supports create/rename/delete,
    grid edits/paste, owner filtering and developer-only read-only source view. No normal
    editing via JSON. Full workspace CI closes the job; compatibility fixtures explicitly
    test historical v1 contracts separately from current headerless-write rejection in BOTH
    model compat.rs::tests::legacy_write_requests_succeed_on_v1_and_fail_after_bump and client
    engine_compatibility.rs::legacy_client_without_headers_is_accepted_on_v1_and_rejected_after_upgrade.
    Historical success uses explicit version "0.1.0", semantics1, protocol1, min1, never
    current(). Both future-semantics fixtures use ENGINE_SEMANTICS_VERSION + 1 with
    protocol/minimum1 and dynamic expected error values/comments, not hard-coded future2.
    Recheck both suites after protocol2 (142c), semantics2 (142d) and protocol3 (142h).

Checks: per-slice named tests in plan.json; final `scripts/ci.sh`, model shared_fragments,
ops_properties/store/workspace, client shared_fragments/undo_every_op/remove_loaded/relations/
engine_compatibility/inspector_view/table/grid/explorer, model `--lib compat::tests` and
workbench table_view. Workflow equality checks
compare independently captured pre/post snapshots and independently evaluated fixtures,
not a list with itself.

## Decisions
- Owner-authorized scope amendment: ALL fragment canvas badge, inline read-only expansion,
  Open-fragment canvas actions, selection gestures and accessibility acceptance move to
  GPUI job 187. Model explanation, inspector numbers, client commands and GPUI generic
  Fragments table remain here. No new Slint features or new GPUI canvas in 142.
- No expensive unresolved owner decision remains. Root-aware node-kind replacement is a
  narrowly guarded reversible op (142h), needed to retain identity when a selected root/gate
  becomes a reference or a reference becomes copied structure; it never changes tree roles.
- Snapshot/log/workspace FORMAT_VERSION remains 1. Reference ops raise protocol/minimum to
  2 in 142c; fragment evaluation raises semantics to 2 in 142d. Node-kind op raises
  protocol/minimum to 3 in 142h. Headerless requests always mean historical (1,1).
  Explicit historical contracts keep v1 compatibility tests honest after each bump.
- Relation presentation mirrors shared-step code: only FRAGMENT_USED_IN is registered.
  Its explicit inspector-rev-fragment-used-in label and existing shared_note convey the
  step-side target without duplicate rows; typed Use/Unuse edits handle attachment.
  This limited inspector wiring ships with142f, not deferred to142g; numeric/explanation
  inspector work remains142g and all canvas scope remains deferred.
- Unshare copies CURRENT editable input values and structure, not one flattened computed
  number. Reused controls/conditions/item IDs remain project references, so later control/
  applicability changes still apply; copied fragments/shared-step ratings are independent.
  Tree descriptions/translations remain on their original tree elements (there are no
  node-description fields). Extraction does not move host tree-level applicability.
- 12 slices total (11 remaining) are honestly sized for graph/storage, evaluation/cache,
  client safety/inspector, root operation, reusable bounded copier, both workflows and table.
  This exceeds the normal 3-5 target to keep every production slice <=600 changed lines.
- Fresh high-risk codebase-details and implementation-readiness audits are required before
  remaining builds. This revision is not an audit PASS or permission to skip those audits.

## Out of scope
- All fragment canvas presentation/UI acceptance: deferred to GPUI job 187 as above.
- Bypass trees (143), report/XLSX/library/@TT outline syntax (144), cross-project references,
  role conversion, new dependencies, new Slint features and a GPUI canvas.
