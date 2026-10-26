# Brainstorm: an attack tree library (for a separate session)

- Status: **design questions answered** in decision D14 ([brainstorm-product-v2.md](brainstorm-product-v2.md) section 3); phase 0 is backlog job 057, the company library job 087.
- Date: 2026-10-26
- Asked for: a library of ready-made attack trees and leaves. While building a
  tree, a project searches the library, picks a relevant item, adds it, and
  customizes it. Items can be **templates** (to be filled in) or **complete**
  (ready to use).

## 1. What the app does today (checked)

| Question | Answer |
|---|---|
| Can I add an existing tree or leaf from somewhere? | **No.** There is no library, no import of a tree from another project, no "insert subtree". |
| What reuse exists? | (a) Inside one tree, **shared steps**: lines with identical text in the outline are one node (a DAG). (b) A tree is a separate object; reports list "Threat scenarios" per tree, so one tree can serve more than one threat scenario. (c) Copy and paste of outline text by hand. |
| Size limit | A tree holds at most **32 nodes** (`MAX_ATTACK_TREE_NODES`). A library item of any size would not fit. |
| Provenance | Every element records its source: human, import, **catalog**, AI. So "came from the library" has a place already. |
| Catalog IDs | Threat scenarios carry catalog references (R155 Annex 5 by default). The pack holds the ID **syntax** only, not any catalog **text**. |
| Where is it written down? | `docs/brainstorm.md` sketches "library reuse without copies" (pattern instances by reference) and "parametric attack patterns". `docs/ai-backlog.md` A4 says AI drafts trees "reusing library patterns". **Neither is in `factory/backlog.md`.** |

So the idea is new to the plan. This note turns it into a backlog item for a
design session (`factory/backlog.md`, section "Waiting for a design session").

## 2. What a library item could be

| Kind | What it is | Example |
|---|---|---|
| **Leaf** (step) | One attack step with defaults | "Reach the OBD port", typical attack potential factors, typical controls that mitigate it |
| **Fragment** | A gate with children, to graft under any gate | "Compromise the backend credentials" (AND of phish and bypass MFA) |
| **Complete tree** | A root goal and everything below, rated | "Spoofed brake request" |
| **Template** | Any of the above with **slots** | "Inject messages on bus `{bus}` to trigger `{function}` on `{ecu}`" |

"Complete" and "template" are a property, not separate kinds: a complete item
has no open slots. A template may ship with default ratings or with none.

## 3. How a project would use it

1. **Search** the library by words, tags, STRIDE category, catalog ID (R155
   Annex 5, CAPEC, ATT&CK, EMB3D), or asset type. Show a preview of the tree
   (the canvas already draws a display list from any tree).
2. **Insert** one of three ways:
   - as a **new tree** (then link it to a threat scenario);
   - **under the selected gate** of the open tree (graft a fragment);
   - **in place of a leaf** (expand a step into a fragment).
3. **Bind slots** (templates): a small form fills `{ecu}`, `{bus}`; node texts
   are filled in.
4. **Customize**: from then on it is the project's own tree; every edit works
   as today (outline editor, ratings, controls).

## 4. Design questions to decide

### 4.1 Copy or reference?

| | Copy with a remembered origin | Reference (pattern instance) |
|---|---|---|
| Customizing | Free, the tree is the project's own | Local overrides stored as a patch; more machinery |
| Library updates | "Update available" with a three way merge proposal, never silent | Arrive by design, same merge problem |
| Engine, evaluation, reports, diff | **Unchanged**: it is an ordinary tree | Every part must expand the reference |
| Storage | A tree per use | One small reference per use |
| Risk | Copies drift | Complexity in the engine, which must stay simple and deterministic |

**Recommendation:** copy, plus an **origin note** (library item ID, version,
which nodes came from it). The note supports "compare with the library" and
update proposals later without touching the engine. Revisit references only if
a customer has thousands of uses of one pattern.

### 4.2 Do ratings travel?

Feasibility depends on the product and its context, and the engine, not the
library, produces the numbers. Options: (a) insert with **no ratings**, only
structure; (b) insert with the library's **suggested ratings marked "from the
library, review"**; (c) let the user choose at insert time. **Recommendation:**
(c), default (b) for complete items from the company's own library, (a) for
public ones. The linter should flag unreviewed library ratings on the Problems
page so they cannot slip into a report unseen.

### 4.3 The 32 node cap

A library item with 60 nodes cannot be inserted today. Either the cap goes up
(canvas layout and the PDF tiling already handle 1,000,000 boxes; the cap
exists for the wire format and early limits, see ADR 0014 and rule R3) or items
are split into fragments. **Decision needed**; it changes decode caps.

### 4.4 Same text means the same node

The outline rule "identical text is one node" would silently **merge** an
inserted step with an existing step of the same name. That is good when the
step really is the same ("Reach the OBD port" used in two trees' branches) and
wrong when it only sounds the same. Options: always ask; merge by default and
show what merged; never merge on insert (suffix the name) and offer a "merge
duplicates" tool. **Recommendation:** never merge silently; show a list of the
matches and let the user tick the ones to share.

### 4.5 Who owns the library?

| Tier | Source | Edited by | Notes |
|---|---|---|---|
| Built in | Ships with the app or a methodology pack | Vendor | Signed, versioned, read only. Public catalogs (CAPEC, ATT&CK, EMB3D) are permissive with attribution; ISO text is **not** shipped (direction section 11) |
| Company | On the server, in the database | Role: methodology owner | Draft, approved, deprecated states; audit event on publish |
| Project | A tree saved from another project | Any editor | "Save selection as library item" |
| Personal | Local file | The user | Probably not needed in v1 |

The pack idea already exists for methodologies (`aegis-model` pack files, size
caps, stable string keys). A **library pack** can follow the same pattern:
JSON, versioned, license field, hash, optional signature, decode caps (R3).
Offline sites import packs from a file.

### 4.6 Search

- **Now, no AI:** lexical search in memory (names, tags, notes, catalog IDs),
  deterministic order, instant at library sizes of thousands of items. No new
  service, works air gapped.
- **Later:** meaning based search (embeddings in the server's PostgreSQL) is
  AI wave 1, feature A1 (`docs/ai-backlog.md`). The library gives A1 and A4
  (tree drafting) their material: the AI proposes from approved items, the
  engine computes, a human decides.
- Useful filters: asset type, threat category, "used in N projects", rating
  status (rated or structure only), approved only.

### 4.7 One undo step, one audit event

An insert is a **batch of ordinary operations** (add nodes, links), applied
atomically, undone with one click, logged as one event (`library_inserted`,
with item ID and version). No new low level operation is needed if the batch is
built from the existing ones; whether to add a single `InsertFragment` op for
the audit trail is a design choice.

### 4.8 Reports and diff

Reports can print "Based on library item X, version N" under a tree.
Baseline compare shows an inserted fragment as added nodes, as for any edit.

## 5. Risks

- **Stale content.** Mitigate with versions, deprecation, and an "update
  available" badge (never an automatic change).
- **Poisoned or oversized items.** A shared library is an input from other
  people: same decode caps and text limits as any pack; plain text only; no
  active content in descriptions.
- **Looks authoritative.** A "complete" tree with ratings can be taken as
  analysis done. The "review needed" mark and the linter guard against that.
- **Copyright.** Library text from standards must not be shipped; users enter
  or import what they are licensed to use.
- **Merge noise** from section 4.4.

## 6. A phased plan to discuss

| Phase | What | Needs a decision on |
|---|---|---|
| 0 | Search and insert from a **built-in demo pack** (a few trees and leaves as a JSON file in the repo); insert as a copy, one undo step; lexical search; origin note | The pack format; the 32 node cap |
| 1 | **Company library on the server**: save a selection as an item, draft/approved states, audit events, roles | Who may publish (ties to the role model, see the roadmap) |
| 2 | **Templates with slots** and a binding form | Slot syntax; escaping rules |
| 3 | **Update proposals**: compare an inserted fragment with its library item, three way merge | Merge rules |
| 4 | **AI assisted**: meaning based search, draft a tree from library items into a suggestion branch | The AI gateway design |

Phase 0 is small and answers the user's first question end to end (find, add,
customize), without touching the engine.

## 7. Questions for the session

1. Copy with origin (recommended) or references?
2. Raise the 32 node cap, and to what?
3. Do ratings travel, and with what mark?
4. Merge identical step names on insert? (Recommended: ask, never silent.)
5. Who may publish company items: only admins, a methodology owner role, anyone?
6. First library content: what do we have the right to ship?
7. Is a separate **Library page** right, or a panel next to the attack tree
   canvas (the direction paper lists "docked side panels: properties,
   library")?
8. Is phase 0 enough for the first customer demo?
