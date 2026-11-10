# GUI first: files are the storage, never the interface

- Date: 2026-11-10
- Status: proposal from the owner's steer; recorded as D40. No code changed.
- Changes: [workbench-architecture.md](workbench-architecture.md) section 9 ("text twin") and D38 (how the item definition is edited). The storage (D32) does not change.

## TLDR

- A customer's analyst never sees JSON, Markdown, a file path or git. They see a tree of things, grids, forms, a canvas and plain-word versions.
- The model is a database navigator: objects on the left, a grid or a diagram in the middle, properties on the right. The files are what the database files are there: something you back up, not something you edit.
- "Not too heavy" means three things we do **not** build: a word processor, a free drawing tool, a formula or script language in the UI.
- The source view stays, as a developer option that is off by default.
- Attack trees grow from a right-click menu and the keyboard: add a step, use an existing shared step or fragment, or take one from the library.

## 1. Who uses what

| User | Sees | Never needs |
|---|---|---|
| Analyst (the customer's engineer) | Project tree, grids, forms, canvas, reports, Versions | Files, JSON, Markdown, git, API |
| Lead or consultant | The same, plus configurations, templates, compare, the command palette, bulk edits | Files |
| Integrator (IT, tool owner) | Automation settings, tasks, API tokens, the developer view | Nothing hidden |

The first row is the product a customer buys. The third row is what makes it fit into their company. The design must never make the first user pay for the third.

## 2. The navigator model

| In a database navigator | In AegisTARA |
|---|---|
| Object tree: servers, schemas, tables | Project tree: workspace, systems, element kinds, attack trees |
| Data grid with inline editing | Element tables: assets, threats, the risk register, controls |
| Row detail and properties panel | Inspector |
| Entity-relationship diagram | Item diagram, trace graph |
| Filter bar and saved filters | Filter chips and saved queries, built by clicking |
| "View DDL" | Developer view of the stored file, read-only unless switched on |
| Backup and restore | Save version, bundle export and import |

## 3. How each kind of information is edited

| Information | The user's tool | Stored as | Notes |
|---|---|---|---|
| Lists of elements | **Grid**: type in a cell, pick a rating from a coloured dropdown, select many and edit at once, fill down, paste rows from Excel, filter per column | JSON | The most used tool; it must feel like a spreadsheet |
| One element in depth | **Inspector form** with link pickers | JSON | Link pickers search by ID or name |
| Attack trees | **Canvas** with the menu in section 4, and a table mode | JSON | |
| Item definition, text | **Sections with light text**: Description, Scope, Out of scope, Functions and so on, each a text box with bold, lists, and `@` to mention an element | Markdown | No syntax is ever shown. Not a page layout tool. |
| Item definition, structure | **Diagram canvas**: drop a component, drag to connect, draw a boundary around boxes; the same data as three small grids (components, interfaces, flows) | The structure block in the Markdown file | Diagram and grids are two views of one model |
| Pictures | Paste or drop an image into a section; caption | An attachment file | The customer's own architecture pictures (D15) |
| Settings and configuration | **Form with layers**: each value shows where it comes from and a reset | JSON | As in the Figma mock-up |
| Methodology | Read-only tables of the scales and matrices | Pack | Editing a pack is an integrator's job |
| Versions | **Versions timeline** in plain words: "Saved 14:02, 3 changes", Compare, Mark as baseline, Restore | git inside the app | See section 6 |
| Reports | A profile form (sections, scope, configuration, language) and a preview | JSON profile, Typst template | Templates are for integrators |

Guided steps are used where a blank screen would stop a new user: new workspace (methodology, template), new system (template, name), new configuration (copy one, change switches), import from Excel. Each is at most three steps and ends in the normal screens.

## 4. Growing an attack tree

Three ways to do the same things: the right-click menu, the keyboard, and dragging from the Library panel. The menu opens on a node, on the `+` handle at its edge, with the Menu key or Shift+F10.

```
Add step                         Tab
Add gate                    ▸    AND · OR
Add existing                ▸    Shared step…      search by ID or name; shows rating and "used in 4 trees"
                                 Fragment…
From library                ▸    Suggested for IF-2 Ethernet DoIP   (by what the node goes through and its attack type)
                                 Recently used
                                 Search the library…
Paste                            ⌘V
────────────────────────────
Make shared…                     this step or subtree becomes a shared step or fragment
Save to library…
Make a local copy                turn a reference into an editable copy
────────────────────────────
Rate                        ▸    one submenu per factor, from the pack
Goes through                ▸    component or interface
Applies when                ▸    condition
Delete                           ⌫
```

Keyboard, as in a mind-map tool: **Tab** adds a child, **Enter** adds a sibling, **F2** renames, typing in a new node offers matching shared steps and library items as you type, so reuse happens without opening a menu.

Rules:

| Action | What it creates | Rule |
|---|---|---|
| Add existing | A **reference** to a shared step or fragment in this workspace | Rated once; a change shows in every tree that uses it (D29). The node carries a link mark and "used in N". |
| From library | A **copy** with an origin note | The copy is yours to change (D14). Ratings come along only if you tick it, and are then marked for review. |
| Make shared | A shared step or fragment, and a reference in place of the node | One undo step; exact inverse |
| Any insert | | One undo step, one history entry. A cycle is refused with its path. The node cap is checked before the insert, not after. |

- Suggestions are ranked without AI: same interface kind, same attack type, same catalog entry, then text match. AI ranking can come later through the same list.
- Dragging an item from the Library panel onto a node does the same as "From library"; the node under the pointer lights up when the drop is allowed.
- The same menu works on a row of the tree's table mode.

What exists in the model: tree node ops, the library pack and its search, origin notes. What is still open in the backlog: shared steps, shared fragments, bypass trees (BLK-01 to BLK-04). The menu ships with "Add step", "Add gate", "From library" first; "Add existing" and "Make shared" light up when those jobs land.

## 5. Keeping it light

| We build | We do not build | Why |
|---|---|---|
| Sections with light text | A word processor with fonts, columns, page layout | The report template does layout; the user writes content |
| A diagram of typed boxes and links | Free drawing, shapes, colours | Every box means something to the engine; pictures cover the rest |
| Filters by clicking | A query language in the UI | The API is there for integrators |
| Three diagram kinds on one canvas engine | A diagram tool per view | One set of gestures to learn, one engine to maintain |
| Pack tables shown read-only | A methodology editor | Rare, risky, an integrator's job |

One canvas engine (`aegis-canvas`) draws the attack tree, the item diagram and the trace graph, with the same select, drag, connect, menu and zoom everywhere.

## 5a. Icons and tooltips

| Rule | Detail |
|---|---|
| Every menu item has an icon before its label | Also toolbar buttons, tabs, tree rows (one icon per element kind) and status bar items |
| One icon family | A single permissively licensed set (license checked under D5), one stroke weight, drawn at 16 px; an element kind keeps the same icon everywhere |
| Every icon has a tooltip | The name, one line that says what it does, and the shortcut: "Add existing — use a shared step that is already rated (⌘E)" |
| A disabled item says why | "Make shared: select a step or a gate first" |
| Icons never stand alone for meaning | Menus show icon and label. Icon-only buttons are allowed in toolbars, always with a tooltip and a name for screen readers |
| Colour is not the icon's job | Icons are one colour; only rating badges carry colour |
| Text is translated | Tooltip text goes through the same English and Japanese catalogs as labels (D13) |

One table in the client core lists each command once: id, label, icon, tooltip, shortcut. Menus, the toolbar, the palette and the shortcut list all read it, so an icon or tooltip cannot be missing in one place and present in another. A test fails for a command with no icon or no tooltip.

## 6. Versions without git knowledge

D32 keeps git as the history. For a customer it must be invisible and need no installation:

- The app carries a git library (a permissive Rust one; to be chosen and license-checked in the job) and keeps the repository inside the workspace folder.
- **Save version** makes a commit with an automatic summary ("3 threats changed, 1 control switched off") that the user may edit. **Mark as baseline** makes a tag. **Compare** shows the meaning diff. **Restore** brings an old version back as a new version; nothing is ever rewritten.
- No branches, remotes or merges in the UI. A user who knows git can use it on the folder; the app copes with what it finds.
- Sending a project to someone is **Export bundle**, not "push".

## 7. What changes in the plan

| Job | Change |
|---|---|
| 187-tree-tab | Includes the menu, keyboard and drag rules of section 4 |
| 189-table-tabs | Includes multi-cell edit, fill down, paste from Excel, column filters |
| 193-item-block | Unchanged: the block is the storage form |
| 195-item-tab | Becomes the **item editor**: sections with light text, pictures, and editing on the diagram canvas. Size L. |
| 168-drop-history-pages | Becomes the **Versions timeline** of section 6 |
| new 203-embedded-versions | The git library inside the app: save version, baseline, restore; license check; no git installation needed. Needs 164. |
| new 204-guided-steps | New workspace, new system, new configuration. Needs 182, 200. |
| WBX-07 "a source view of every tab" | Becomes a developer option, off by default, read-only until switched on |
