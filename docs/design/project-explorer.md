# Project explorer: one project holds everything

- Date: 2026-11-18
- Status: Q1 to Q5 decided by the owner on 2026-11-18 (section 10). Q6 (missing properties) is open; the owner signs off the mockup after it.
- Asked for by the owner after the first GPUI mockup (job 206): systems under one project, an item definition per system, base attack trees that systems inherit, Markdown for all text, a database-explorer tree, a calmer palette.
- Reads with: [workbench-architecture.md](workbench-architecture.md) section 3 (scope levels), [workbench-gui-first.md](workbench-gui-first.md), [reference-ui-review.md](reference-ui-review.md) (ideas, never copied).
- Mockup: https://claude.ai/artifact/XrQjbEY2w4tXkgiumEN89m (screens 16 to 19 show this outline).

## TLDR

- **One project, one tree.** The left panel is an explorer like pgAdmin: Project → Project definition, Configurations, Base library, Shared analysis, **Systems**, Traceability, Reports, Versions. Every system has the **same folders in the same order**, so a user who knows one system knows all of them.
- **Each system has its own item definition**: description, scope, functions, schematic, components, interfaces, assumptions and TOE configuration. Then its own assets, damage scenarios, threats, attack trees, controls, circumvent trees, risk treatment, goals and claims.
- **Where an element lives (owner, Q1):** damage scenarios, security control definitions, goals and claims always live at project level in Shared analysis; a system links to them and shows them in its own folders with `⇢`. Everything about one item (item definition, assets, threats, trees, circumvent trees, control applications, risk treatment) lives in the system.
- **Base library = the "base directory".** System templates (ECU, gateway, telematics, back end) and base attack trees live there. A system is **derived from** a template and sets its **integration properties** (has OBD access, has OTA, bus types, external connectivity). Inherited trees switch branches on and off from those properties. A change in the system is an **override**, marked and resettable. A new base version shows each system what changed.
- **All text is Markdown**, written in one editor everywhere: formatting appears as you type, a toolbar for people who don't know Markdown, `@` to link an element, and a source view for those who do. The schematic is a block-diagram editor whose boxes are real components.
- **Palette:** quiet slate neutrals, one calm blue accent, six system colours that never look like risk colours. Risk colours only appear as labelled badges.

## 1. What the owner asked for, and the answer

| Asked | Answer |
|---|---|
| Systems organised under one project | Systems folder in the explorer, each with its own icon and colour (section 3) |
| Item definition belongs to each system | Item definition is the first folder of every system (section 3.2) |
| Database-explorer feel (pgAdmin) | Tree with folders and counts, right-click New, a folder opens a grid, a leaf opens an editor tab (section 2) |
| Project definitions for internal and external systems | A system is Internal (full TARA) or External (context only: item definition and interfaces) (section 3.3) |
| Damage scenarios linked to each system | Live at project level; each system links to the ones it uses and shows them in its own folder (section 4) |
| Base attack tree that every ECU system inherits | Base library with system templates and base trees, integration properties, overrides (section 5) |
| Markdown everywhere, smooth writing and drawing | One Markdown editor, one schematic editor (section 6) |
| Menus missing compared with the reference tool | Section 7 lists each idea and where it lives here |
| Simple, pleasant palette | Section 8 |

## 2. How the explorer behaves

Three products set the pattern. **pgAdmin** shows every object type as a folder with a count, even when empty, so the user always knows where a thing would be. **DBeaver** opens a folder as a grid and an object as an editor with tabs. **The JetBrains database tool window** gives every object kind its own icon, and right-click New always offers the kinds that can live under the selected node.

Rules:

1. **Same folder set in every system, in the TARA order.** Empty folders show `0` in grey; they are never hidden, so the structure never changes shape.
2. **A folder opens a grid tab**, and **an element opens an editor tab**. A second click on the same node focuses the existing tab.
3. **Right-click → New** offers only what can live there. **New** on the Systems folder starts the three-step guided step (template, name, integration properties).
4. **Counts and marks** on folders: the number of elements, plus a red dot for errors and an amber dot for warnings from the linter.
5. **Shared elements** appear in every system that uses them, with a link mark `⇢`. Opening one opens the single shared element.
6. **Inherited elements** carry `↳`, and **overridden** ones carry `↳•`, with "from ECU base 1.2" in the tooltip and the inspector.
7. **Filter box** at the top of the explorer: type to filter by ID or name across the whole tree.
8. **Drag and drop:** move a component between systems, drag a base tree onto a system's Attack trees folder, drag an attack leaf onto a tree node.
9. **Keyboard:** arrows move, Right opens a folder, Enter opens a tab, F2 renames, Delete removes with an undo toast, and `⌘K` searches everything.

## 3. The tree

```text
▾ ◆ E-platform 2028                          ISO/SAE 21434 + R155
  ▸ ◉ Overview                               process map, management summary
  ▾ ▤ Project definition
      Description                            Markdown
      Parties (responsible)            4     us, customer, suppliers
      Glossary                         12
      Related documents                6
      Methodology                            read-only: scales, matrices, CAL table
  ▾ ⚙ Configurations                   3     TOE variants of the whole product
      EU base
      EU premium
      Japan
    ⛉ Attacker profile                       attack filter
  ▾ ▣ Base library                           the "base directory"
    ▸ System templates                 4     ECU, Gateway, Telematics, Back end
    ▸ Base attack trees                9     ECU base, CAN node base, OTA client base …
    ▸ Technical attack trees           5     reusable sub-trees (fragments)
    ▸ Attack leaves                   31     shared steps, rated once
    ▸ Control catalogue               40
    ▸ Damage catalogue                18
  ▾ ⇢ Shared analysis                        project level, linked from systems
      Damage scenarios                 9
      Security controls               14
      Security goals                   6
      Security claims                  2
  ▾ ▦ Systems                          5
    ▾ ■ Gateway                              internal · from template Gateway 2.0
      ▾ Item definition
          Description · Scope · Out of scope           Markdown
          Functions                    6
          Schematic                            block diagram
          Components                   9
          Interfaces and data flows   14
          Assumptions                  4
          TOE configuration                    switches for this system
        Assets                        11
        Damage scenarios               5     linked from Shared analysis ⇢
        Misuse cases                   2
        Threat scenarios              14
        Attack trees                  14     8 ↳ from base, 2 ↳• overridden, 4 own
        Security controls              9     applications: status in this system
        Circumvent trees               3
        Risk treatment                14     one row per threat and damage pair
        Goals and claims               5     linked ⇢
        Problems                       2 ●
    ▸ ■ Brake domain                         internal · from template ECU 1.2
    ▸ ■ Telematics                           internal · from template Telematics 1.0
    ▸ ■ Infotainment                         internal
    ▸ □ OEM back end                         external · context only
  ▸ ⇄ External results                  1     another team's TARA, pinned
  ▸ ⋔ Traceability                           graph and matrix
  ▸ ⎙ Reports                           3     profiles
  ▸ ⟲ Versions                                baselines and history
```

### 3.1 Project level

| Folder | Holds | Notes |
|---|---|---|
| Overview | Process map, risk totals, top risks, what changed since the last baseline | Opens on project open |
| Project definition | Description (Markdown), parties, glossary, related documents, methodology (read-only) | |
| Configurations | Named TOE variants of the whole product | A system's TOE configuration sits below these as one more layer |
| Attacker profile | Which attack potential values count | One per project; a configuration may change it |
| Base library | Templates and base material (section 5) | Pinned versions from the company library |
| Shared analysis | Damage scenarios, control definitions, goals, claims | Section 4 |

### 3.2 System level (the same for every internal system)

| Folder | Holds | State in the code today |
|---|---|---|
| Item definition | Description, scope, out of scope (Markdown); functions; schematic; components; interfaces and flows; assumptions; TOE configuration | Components, interfaces and systems exist; functions, per-system text and the schematic editor are planned |
| Assets | Asset, bound element, security properties | Exists |
| Damage scenarios | The project-level ones this system's assets and threats link to (`⇢`) | Exists at project level; the per-system folder is a filtered view |
| Misuse cases | Optional; the pack may hide it | Planned (FND-13) |
| Threat scenarios | Asset, property, STRIDE, catalog reference | Exists |
| Attack trees | One per threat: own, inherited or overridden | Trees exist; inheritance is new |
| Security controls | Applications of controls in this system, with status Implemented, Planned or Off | Planned (140) |
| Circumvent trees | One per control: how the control is bypassed | Planned (BLK-04) |
| Risk treatment | The risk register: impact, feasibility, risk, decision, rationale, residual | Exists |
| Goals and claims | The project-level ones linked to this system's threats (`⇢`) | Exists at project level |
| Problems | Linter findings for this system | Exists |

### 3.3 Internal and external systems

| Kind | Has | Use |
|---|---|---|
| **Internal** | Every folder above | The systems we analyse |
| **External** | Item definition only (description, interfaces) | Back end, workshop tester, charging station: we draw their interfaces and trust boundary but do not analyse them. Their threats arrive through our interfaces. |

## 4. Where an element lives (decided by the owner, Q1)

The ownership table in [workbench-architecture.md](workbench-architecture.md) section 3.2 stands.

| Lives at project level (Shared analysis) | Lives in the system |
|---|---|
| Damage scenarios, security control definitions, security goals, security claims, parties, glossary, documents, configurations, attacker profile | Item definition (text, functions, schematic, components, interfaces, assumptions, TOE configuration), assets, misuse cases, threat scenarios, attack trees, circumvent trees, control applications (where a control applies and its status in this system), risk treatment |

- A system's Damage scenarios, Security controls and Goals and claims folders list the project-level elements its own elements link to, each with `⇢`. Opening one opens the single project-level element.
- **New** in such a system folder creates the element at project level and links it at once (one undo step), so the user can work inside a system without switching folders.
- A project-level element shows "Used in: Gateway, Brake domain" in its inspector. One no system uses gets a lint warning.

## 5. Base library and inheritance

### 5.1 What is in the base library

| Base item | Example | What a system gets from it |
|---|---|---|
| System template | "ECU 1.2" | An item-definition skeleton (typical components and interfaces), typical assets, the base trees to attach, the control catalogue entries, and its **integration properties** with defaults |
| Base attack tree | "ECU base: take control of an ECU" | A tree with branches that carry conditions ("only when OBD access", "only with OTA client") |
| Technical attack tree | "Exploit a CAN node" | A sub-tree that any tree may use (D29 fragment) |
| Attack leaf | "Read firmware via JTAG" | A step rated once and used in many trees (D29 shared step) |
| Control catalogue | "SecOC", "Secure boot" | Control definitions a system applies |
| Damage catalogue | "Loss of braking" | Damage scenarios to start from |

The base library is pinned from the company library (the existing library packs) at a version. Updating it is an explicit step: "Base library: ECU 1.3 available, 4 changes".

### 5.2 Integration properties

A template declares the questions that decide which base material applies, for example:

| Property | Kind | Example value |
|---|---|---|
| Physical diagnostic access (OBD) | yes / no | yes |
| Wireless interfaces | set | LTE, Bluetooth |
| In-vehicle buses | set | CAN-FD, Ethernet |
| Software update over the air | yes / no | yes |
| Safety relevance | pack scale | ASIL D |
| Debug ports closed in production | yes / no | yes |

A system answers them in a form on its TOE configuration page. These answers are **conditions** (the condition model of jobs 133 and 134). They switch base-tree branches on and off and change feasibility through rules. Nothing is copied: switching "OBD access" to "no" removes the OBD branch from every inherited tree in that system, with "Not applicable: no OBD access" as the reason.

### 5.3 Inherited, overridden, own

| Mark | Meaning | What editing does |
|---|---|---|
| `↳` inherited | Comes from the base, unchanged | Editing a value makes it an override |
| `↳•` overridden | Base element with local changes | The inspector lists each changed field with **Reset to base** |
| no mark | Made in this system | Normal editing |

When the base library moves to a new version, each system gets a **Base changed** review: unchanged inherited values update by themselves, overridden values whose base also changed are listed for a decision, and nothing changes silently.

This is new behaviour. Today a library item is a copy with an origin note (D14). Inheritance needs a derivation link and an override layer on trees, assets and the item skeleton. See Q2.

## 6. Writing and drawing

### 6.1 One Markdown editor for every text

Every description, scope, rationale, assumption, comment and report paragraph is Markdown, edited in one editor component:

- **Live mode (default):** formatting is shown as you type, as in Typora or Obsidian's live preview. The Markdown marks are visible only on the line being edited.
- **Toolbar** for people who never learned Markdown: bold, italic, heading, list, numbered list, table, link, `@` element, image, diagram.
- **`@` mention:** type `@C04` or `@brake` to link an element. The link shows as a chip and follows renames.
- **Source mode:** a toggle that shows the plain Markdown. This is the only place Markdown syntax is shown in full.
- Paste from Word or a web page becomes clean Markdown; pasted images become attachments.

This updates D40 ("no syntax is ever shown"), because the owner asked for Markdown editing. See Q3.

### 6.2 The schematic: a block diagram that means something

- **Tools:** component, external entity, trust boundary, connector (interface or data flow), label, group. Snap to grid, align and distribute, auto-layout, zoom, minimap.
- Every box **is** a component and every line **is** an interface or flow: drawing one creates the element in the grid, and deleting it removes it (one undo step).
- Colours come from the system colour, never from a free colour picker.
- **Free figures** inside a Markdown text (an illustration that the engine does not read) come later as a figure block. Pasted images work from the start. See Q4.

## 7. Ideas from the reference tool, in our structure

Ideas only. No layout, icon, wording or colour is copied.

| Idea seen | Where it lives here | State |
|---|---|---|
| TOE description, scope, assumptions as separate pages | System → Item definition → Description, Scope, Assumptions | Text exists for the project; per system is new |
| TOE configuration | Project → Configurations (product variants), plus System → TOE configuration (integration properties) | Conditions in progress (133, 134) |
| Attack filter | Project → Attacker profile | Exists (D27) |
| Misuse cases | System → Misuse cases | Planned (FND-13) |
| Threats and threat scenarios as two lists | Threat catalogue in Base library; threat scenarios per system | Catalog packs exist |
| Technical attack trees | Base library → Technical attack trees | Planned (BLK-02) |
| Circumvent trees, one per control | System → Circumvent trees | Planned (BLK-04) |
| Attack leaves | Base library → Attack leaves | Planned (BLK-01) |
| Tree toolbox panel to drag from | Library panel on the right of the tree tab: base trees, technical trees, leaves | New (job 187) |
| Two "active" switches per control | One status: Implemented, Planned, Off (D28) | Planned (140) |
| Properties panel says why an element is inactive | Inspector banner: "Not applicable: control SC17 is Off" | Planned (135) |
| External results | Project → External results | Design session (MTA) |
| Security claims, security goals, risk treatment | System → Goals and claims, Risk treatment | Exists |
| Traceability graph | Project → Traceability | Planned (TRC-05) |
| Status bar error and warning counts | Status bar | Exists |
| "Auto evaluation" switch | Not needed: evaluation is always live and incremental (5 ms budget) | |

## 8. Palette

Calm and readable for long sessions. Colour means something; it never decorates.

| Token | Light | Use |
|---|---|---|
| `surface` | `#FFFFFF` | Editors, grids |
| `sidebar` | `#F4F6F8` | Explorer, inspector background (one clear step from surface) |
| `raised` | `#EDF0F3` | Headers, toolbars, status bar |
| `line` | `#DCE2E8` | Grid lines, separators |
| `line-strong` | `#9AA5B1` | Field borders |
| `ink` | `#1E2A36` | Text |
| `ink-muted` | `#4F5D6B` | Secondary text (6.5:1 on white) |
| `accent` | `#2F6DB5` | Selection, focus ring, primary button, links (5.3:1 on white) |
| `accent-wash` | `#E7EFF9` | Selected row, selected tree node |

**System colours.** Six identity colours, shown as a small square and a tinted icon next to the system name, never as a cell background:

| Slot | Colour | Example |
|---|---|---|
| 1 | Indigo `#5560C8` | Gateway |
| 2 | Plum `#94508F` | Brake domain |
| 3 | Petrol `#2A7C8C` | Telematics |
| 4 | Rose `#B8577A` | Infotainment |
| 5 | Slate `#5A6F86` | System 5 |
| 6 | Umber `#8A6748` | System 6 |
| External | Outline only, no fill | OEM back end |

**Risk colours** keep the colour-blind-safe set the product already uses, and appear **only** as a badge with its word ("High", "Medium"…): orange, amber, sky, green. System colours avoid that hue family, so a system square is never read as a risk.

Dark theme: the same tokens, with surfaces at about 12 to 20 percent lightness and the accent lightened to `#7FA8E0`.

Typeface: Atkinson Hyperlegible (chosen for legibility; already used), with Noto Sans JP for Japanese.

## 9. What changes in the model and the plan

| Change | Size | Where it goes |
|---|---|---|
| Per-system item text, assumptions, functions, TOE configuration | M | Extends 195 and ARC-06 (functions) |
| System folders: system-level elements plus linked project-level ones; New in a system folder creates and links in one undo step; system kind Internal or External | M | Job 210, before 189 |
| System templates, integration properties as conditions | L | New; builds on 133 and 134 |
| Base derivation link and override layer for trees, assets and the item skeleton; Base changed review | L | New; needs Q2 |
| Markdown fields everywhere, one editor component, reports render Markdown | M | 195 becomes the editor component; reports in 209 |
| Schematic editor (typed block diagram) | L | 194 and 195 |
| Explorer tree with counts, marks, drag and drop | M | 182 |
| Library panel beside the tree tab | S | 187 |

## 9a. The combined workbench (v3, owner 2026-11-18)

The owner liked the v2 explorer and asked to blend it with the v1 screens. The canvas page "v3 Combined workbench" shows the result.

| Part | From | How it is combined |
|---|---|---|
| Window frame | v2 | Top bar (project, configuration, Go to anything, undo, Save version, account menu), explorer on the left, tabs in the middle, inspector on the right, bottom dock, status bar. The same frame as Zed: docks that collapse, everything on the keyboard. |
| Explorer | v2 | Two modes at the top: **Project** (the hierarchy in section 3) and **TARA steps** (v1's process order: item definition, assets and damage, threats and feasibility, risk and treatment, goals and claims, each listing all systems with counts). One selection; switching modes keeps it. |
| Where you are | new | Every tab starts with a breadcrumb (`E-platform 2028 › Gateway › Threat scenarios`). Opened from Shared analysis, a grid gets a **System** filter chip instead. |
| Grids, inspector, explain-why, multi-link picker, rating dropdowns, hatch for not applicable | v1 | Unchanged in behaviour, restyled. Project-level grids add a **Used in** column with system squares. |
| Controls | v1 + v2 | One project-level grid of control definitions with one **status column per system** (Implemented, Planned, Off), so the per-system application is visible at a glance. |
| Overview | v1 | Opens with the explorer in TARA steps mode; process map boxes show a small bar per system. |
| Settings that change numbers | v1 "Settings with layers" | Moved into the tree: Configurations (product variants) and each system's TOE configuration. |
| **Project settings** | new | Its own node at the bottom of the Project tree (section 9b). |
| Administration, accounts, preferences | v1 "Administration" | Moved out of the project into **App settings** (section 9c). |

Rule that keeps the two kinds of settings apart: **anything that can change a risk number lives in the tree** (configurations, TOE configuration, attacker profile). **Project settings never change a risk number.** **App settings never belong to a project.**

## 9b. Project settings (one node per project)

| Page | Holds |
|---|---|
| General | Name, product, phase, description link, project owner |
| Methodology and scales | The pinned pack and version, its scales, matrices and CAL table, read-only |
| Risk acceptance | The level at or below which a risk may be retained without a claim, and who may accept |
| Identifiers | Prefixes and number width per element kind (`TS`, `DS`, `SC` …) |
| Languages | Content languages and the default report language |
| Base library versions | Pinned template and catalogue versions, with "update available" |
| Members and roles | Who may open, edit or review this project (server mode) |
| Checks | Which linter rules run and at what severity |
| Report defaults | Default profile, branding, cover fields |

## 9c. App settings (draft note, later)

The owner asked to note this for later. It is a separate settings window opened with `⌘,` or from the account menu, like Zed's settings: a category list on the left, a search box on top, and each setting shown with its current value and a reset. Two layers only: the app default and this user. In server mode, an Administration category appears for administrators.

| Category | Holds |
|---|---|
| Appearance | Theme light or dark, density, font size |
| Language | UI language (English, 日本語) |
| Keymap | Shortcuts, with conflicts shown |
| Editor | Markdown editor defaults, spell check, autosave |
| Accounts and servers | Signed-in account, saved servers, transport, sign out |
| License | Edition, seats, expiry, offline token |
| Administration (server, admins only) | Users, roles and **permissions**, sign-in methods, API tokens, audit log, backups |
| AI providers | Later (Phase 7) |
| Developer | Source views, logs, diagnostics; off by default |

Mostly later. One part cannot wait: administration exists only in the Slint app today, so the **Administration** category (with Accounts and servers, and License) must work in GPUI **before Slint is removed (job 192)**. The other categories can follow.

## 10. Owner decisions (2026-11-18)

1. **Q1, where elements live:** damage scenarios, controls, goals and claims **always at project level**; systems link to them (section 4).
2. **Q2, inheritance:** **live link with overrides**, and a review list on each new base version (section 5.3).
3. **Q3, Markdown:** **live editor with a source switch** (section 6.1). This updates D40.
4. **Q4, drawing:** **typed schematic and pasted images first**; free figures later (section 6.2).
5. **Q5, "responder":** not relevant. Responsible parties stay as they are (FND-08); no new element.
6. **Q6, missing properties:** still open. The owner will upload the menus from their machine; they go into section 3.2.
