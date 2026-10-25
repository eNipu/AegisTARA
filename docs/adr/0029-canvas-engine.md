# ADR 0029: Canvas engine: linear layout, spatial index, display list

- Status: proposed (job 032-canvas-display-list)
- Date: 2026-10-25
- Scope: `crates/aegis-canvas`
- Rules: A5, A6, A8, A9, D1, D4, D10, P4, R2 in `docs/engineering-standards.md`; `direction-v1.md` section 5

## Context

The desktop client draws attack trees; the PDF report must draw the same
trees. Slint has no transforms (`spikes/RESULTS.md`), so zoom needs every
length scaled by hand. Layout, hit testing and culling must stay fast on big
graphs. Model trees inside a project are capped at 32 nodes
(`MAX_ATTACK_TREE_NODES`), but a standalone `AttackTree` can hold up to
`u32::MAX`, and the budgets name 100,000 and 1,000,000 node references.

## Decisions

### 1. The engine emits a display list and never draws

`render(scene, viewport, options)` returns a flat `DisplayList` of
rectangles, Bézier curves, polylines and single text lines in **screen**
coordinates (zoom and pan already applied), back to front. Adapters paint it:
the SVG adapter (here) for reports, the Slint adapter (job 033) for the
window. Because the list is plain data, tests compare lists and SVG text
with no window, and the toolkit is replaceable. Colors are *tokens*
(`Paint`, `Tone`); each adapter resolves them, so dark mode is a palette
swap and the rating colors live in one place (the mockup's Okabe-Ito set).

### 2. Layout: Buchheim et al. in O(n), iterative

| Option | Why not |
|---|---|
| Walker 1990 | O(n^2) worst case. |
| Layered DAG layout (Sugiyama) | Crossing minimisation is NP-hard and its heuristics cost far more; our trees are trees plus a few shared links. |
| **Buchheim, Jünger, Leipert 2002** | Linear, keeps Walker's four aesthetic properties, small. |

The DAG is drawn as its **owner tree** (each node has one owner in the
model); extra parent links are dashed edges. The implementation uses explicit
stacks in both walks, so a one-million-level chain does not overflow the
stack (test and benchmark).

Measured (release, 4 cores, divan): layout 10,000 nodes 0.35 ms; 100,000
nodes 4.2 ms; 1,000,000 nodes 51 ms; 1,000,000-level chain 73 ms. Growth is
linear.

### 3. Spatial index: R-tree (A5)

`rstar` bulk-loaded (OMT) over node boxes and edge bounding boxes. Hit test
and culling are O(log n + k). A 100,000 node hit test takes 33 ns. Query
results are sorted so drawing order is deterministic. The trees are derived
data (D10): rebuilt, never edited.

### 4. Frame cost is O(visible)

One frame at 1:1 over 100,000 nodes: 7.8 us (budget 16 ms). The worst case,
the smallest zoom with about 19,000 nodes in view, one rectangle each:
1.9 ms. Level of detail: text from zoom 0.5, bars and outlines from 0.2,
single rectangles below. A rating change is `Scene::restyle`, O(1), because
node boxes have a fixed size (no layout, no index rebuild); a structure change
rebuilds the scene (100,000 nodes: 46 ms).

### 5. Deterministic text without a font engine

`wrap` estimates width as `chars x size x 0.55`, breaks greedily, breaks
over-long words, adds an ellipsis. The estimate is identical on every
machine, so screen, SVG and PDF break lines at the same places. Cost: a
font with unusually wide glyphs can overflow a box a little, and adapters
may clip. The alternative (measuring with the real font) would make the
report differ between machines.

### 6. SVG adapter

Numbers with at most two decimals and no negative zero, fixed attribute
order, XML escaping, a `viewBox` so one drawing scales to any width.
`scene_to_svg` draws the whole tree with text always on. Snapshot tests
(`insta`) cover a light full tree with a shared node, selection and critical
path, and a dark zoomed-out frame; the light one was also rasterised with
Chromium and compared by eye with the design mockup.

### 7. Keyboard navigation and accessibility hooks

`Scene::navigate` moves left (owner), right (the child nearest in height),
up and down (the neighbour in the same column). Properties: left undoes right
and up undoes down. `Viewport::reveal` pans minimally to show the selection.

## Verification

Seven property tests on random trees with shared links (no overlaps, owner
centred over children and left of them, determinism, index equals linear
scan, culling equals "visible set", restyle moves nothing, navigation is
consistent); layout unit tests (mirror symmetry, identical subtrees, 1M-level
chain); wrap, viewport, and tone tests.

## Not in this job

Text measurement with real fonts; edge hit testing and drag-to-link;
incremental layout on structure change (a full 100,000 node rebuild is 46 ms
and project trees have at most 32 nodes); PNG export; the Slint adapter.
