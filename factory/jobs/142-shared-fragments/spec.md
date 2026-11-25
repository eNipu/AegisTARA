# 142 Shared fragments (BLK-02, BLK-03)

## Design Reference
- `docs/requirements.md` §3 (`BLK-02`, `BLK-03`)
- `docs/decisions.md` (`D29`)
- `docs/architecture.md` §III.7 (Shared building blocks)
- `docs/adr/0002-dag-links-and-cycle-detection.md`

## Problem & Goal
Technical attack trees repeat across threat trees. A fragment is an attack tree with the
`Fragment` role whose root value feeds a step of another tree, so a change in the fragment
shows in every tree that uses it.

## Acceptance Criteria
1. `TreeRole { Threat, Fragment }` is fixed by `Op::AddAttackTree { role, .. }`; fragments show
   `TT` visual IDs and threat trees `AT`, from one shared sequence so an ID never changes or
   repeats. The role survives undo, `Project::eq`, diff, snapshot (`role`, omitted for threat),
   op log (`role`, omitted for threat) and workspace files (`trees/ttNN.json`); legacy files load
   as threat trees, and a visual ID or file name whose prefix contradicts the role is refused
   (`shared_fragments` tests). (142a)
2. `UseFragment { tree, node, fragment }` / `UnuseFragment` with a node slot; the target must have
   role `Fragment`; the step must hold no ratings of its own and no shared step; a reference that
   closes a cycle is refused with its path (`FragmentCycle`, Kahn's check over the fragment
   graph); depth at most 8; a used fragment cannot be removed (`FragmentInUse`); removing a
   linked node or tree is refused until unlinked; snapshot, op log and workspace persistence;
   exact inverses. (142b)
3. Evaluation in topological order over the tree graph, fragments first, each once (A2);
   explain-why steps into the fragment; the cache keeps a reverse index fragment → using nodes
   and marks users dirty transitively; property test of random fragment DAGs against a naive
   evaluator; edit budget with 50 fragments. (142c)
4. Client: canvas reference badge, read-only in-place expansion, "Open fragment", a fragments
   tab with "Used in", undo-safe client edits. (142d)

## Decisions
- One visual sequence for both roles: `AT00`, `TT01`, `AT02`. Separate sequences would let
  `AT01` and `TT01` coexist and break the workspace file-number ordering and duplicate checks.
- No protocol bump in 142a: the new `role` field is optional, and an older server refuses it as
  an unknown field instead of misreading it. The protocol bump lands with the new op variants
  in 142b.
- The workspace reader checks the file prefix against the stored role itself, so the error
  names the tree file rather than `aegis.json`.

## Out of scope
- Bypass trees (143); `@TT02` outline syntax, reports and library save (144).
