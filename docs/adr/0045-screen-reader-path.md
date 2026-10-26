# ADR 0045: A screen reader path for the attack tree canvas

- Status: accepted (job 051-screen-reader-path)
- Date: 2026-10-26
- Scope: `crates/aegis-canvas/src/{access,scene}.rs`, `crates/aegis-shell/{ui/canvas.slint,ui/app.slint,src/controller.rs}`
- Rules: A5, A9 in `docs/engineering-standards.md`; gap G14
- Builds on: `0029-canvas-engine.md`, `0031-desktop-shell.md`

## Context

The attack tree is a drawing: rectangles, paths and text from a display list.
A screen reader sees one unlabeled area. A person who cannot see the screen
could not read the tree, find where they are in it, or select a node.
Keyboard navigation (arrow keys) existed, but nothing said what it landed on.

## Decisions

### 1. Describe nodes in the canvas crate, in plain words

`aegis_canvas::access::describe` gives the facts about a node (label, step or
gate kind, depth, position among siblings, number of children, rating text,
on the deciding path, reached from other gates). `sentence` words them:

> Relay to vehicle. Step. Child 2 of 2 under Keyless entry relay attack.
> Rating 14 · Medium. On the deciding path.

The words are in a library crate with unit tests, not in Slint, so they are
testable without a window and reusable (an export, a test) later.

### 2. One accessible element per node on screen

`visible(scene, viewport)` returns the nodes inside the viewport with their
screen boxes: O(log n + k x depth) for k visible nodes (A9, so a tree of a
million nodes costs what is on screen; a test asserts 5000 nodes in under
50 ms). The window lays an invisible `list-item` over each box with the
sentence as its label, its position and the count (so a reader can say
"item 3 of 12"), and a selected flag. The default action (Enter, "activate")
selects the node and brings it into view.

Nodes off screen are not offered. Keyboard navigation reveals the node it
moves to, so a reader following the arrow keys never loses it. A user who
wants the whole tree as text has the outline editor on the same page.

### 3. The canvas speaks the selection

The focusable canvas has the role `list`. Its label is "Attack tree with N
nodes." plus the selected node's full sentence, or how to select the root.
When the selection moves, the label changes, which readers announce for the
focused element. The canvas is on the Tab chain (a test reaches it with Tab
and arrow keys only).

### 4. What is not verified

The tests check what the window hands to Slint (entries, sentences, order,
flags) and that the canvas is reachable by keyboard. They cannot check what
Orca, NVDA or VoiceOver say, because no accessibility backend runs in the
headless harness. The UX review must do a hands-on pass with at least one
screen reader (listed in `docs/review-requirements.md`).

## Consequences

- No new dependency. `Scene` gained four O(1) accessors (`edges_from`,
  `owned_children`, `shared_in`, `owner_pos`).
- The accessible list is rebuilt on every redraw (pan, zoom, selection).
  That is O(visible nodes), well inside the 16 ms frame budget at the
  sizes in the budgets table; the benchmark for a large tree is a follow-up.
- Other custom-drawn parts of the window (colour swatches, risk bars) still
  rely on their neighbours' text. The UX review should check them.
