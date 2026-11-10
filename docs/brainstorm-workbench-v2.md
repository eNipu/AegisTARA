# Brainstorm: the workbench (a Zed-like editor for TARA)

- Date: 2026-11-10
- Status: proposal for the owner. No code changed. Nothing here is decided until it has a D number in [decisions.md](decisions.md).
- Update 2026-11-10: the owner accepted the direction. [design/workbench-architecture.md](design/workbench-architecture.md) is the detailed architecture and replaces two points here: files are JSON, not TOML, and scripts use a scoped API instead of editing files.
- Trigger: the owner finds the current UI stiff, likes Zed, and wants Markdown item definitions, per-system TARAs with shared parts, templates, scripting, local AI, and plain file history.
- Reverses or changes, if accepted: D7 and D25 (Slint), D14 (library copies), D20 (no scripts), D31 (multi-file projects in the backlog), ADR 0009 and 0050 (op log as the saved history), ADR 0037 (History and Versions pages).

## TLDR

- **One idea carries most of the wishes: a project becomes a folder of text files**, like a code repository. Markdown for item definitions, TOML for everything else. The app is an editor for that folder.
- From that one change come: Markdown item definitions, per-system TARAs in one workspace, templates you inherit and override, normal file history (git), scripts in any language, and AI that proposes a diff.
- **The shell moves from Slint to GPUI with gpui-component**, behind a spike gate. Only `aegis-shell` is rewritten. `aegis-model`, `aegis-client` and `aegis-canvas` stay; this is the swap the architecture was built for.
- **The engine does not change.** Edits are still ops in memory (undo, validation, explain-why). Only what is written to disk changes.
- Do the file format first. It gives value under the current UI and it is needed whatever the toolkit is.
- Five decisions are the owner's (section 9).

## 1. Why the current UI feels stiff

Checked in the code on 2026-11-10.

| Cause | Where | What Zed does instead |
|---|---|---|
| One page per element type: 17 pages in a sidebar (`Page` in `aegis-client/src/view.rs`). You can look at one thing at a time. | D30, copied from the reference tool | Tabs and splits. A tree next to its threat next to the item definition. |
| Everything is a form or a table cell. Text is typed into small boxes. | `app.slint`, `widgets.slint` | Text is the first-class thing. Forms are a view of the text. |
| Settings are spread over pages: conditions, attacker profile, control status, systems. | D26 to D28 | One settings model with layers. Each value shows where it comes from. |
| The project is one opaque blob (snapshot plus `ops.jsonl`). History needs its own pages. | ADR 0009, 0037, 0050 | Files. History is the file history. |
| Slint widgets, fixed layouts, no docking. | D7 | Docks, resizable panels, command palette, keyboard for everything. |

The reference tool in the screenshot has the same shape: a long sidebar, one screen at a time, properties in a fixed right panel. Copying its navigation (D30) copied its stiffness. What is worth keeping from it is the content: the attack tree canvas, the rating badges, the properties.

## 2. The central decision: a workspace is a folder of text

```
vehicle-x/                          the workspace (one vehicle or product)
  aegis.toml                        methodology pack, systems, attacker profile, tasks
  shared/                           used by several systems
    controls.toml
    damage.toml
    goals.toml
  systems/
    gateway/
      system.toml                   extends a template; overrides; conditions
      item.md                       the item definition, in Markdown
      assets.toml
      threats.toml
      trees/
        th-1-bus-manipulation.toml  one attack tree per file
    brake-ecu/
      ...
  library/                          templates and fragments that belong to this workspace
  scripts/                          Python or anything else
  reports/                          report profiles; generated PDFs
```

Rules:

- **Files are the saved truth.** The app loads them into the same in-memory `Project` it has today.
- **Edits stay ops.** A click in the UI is still an `Op` with an exact inverse, so undo, validation, the evaluation cache and explain-why work as now. Save writes the changed elements back to their files.
- **Edits from outside** (a script, git pull, an AI proposal, a hand edit) are picked up by loading the files again and running `Project::diff` (ADR 0020) against memory. The diff is shown before it is applied.
- **Stable order and stable IDs** in every file, so a git diff shows only what changed. The visual IDs (`Th-1`, `Nd-19`) are the keys.
- **A system is a folder.** Its item definition, TARA and configuration live together. This is D26 (one system per component) made visible on disk, and it makes D31 (a project assembled from several TARAs) the normal case instead of a backlog item: a supplier's ECU TARA is a folder you add.
- **Numbers never depend on the view** (D26 unchanged). Choosing which systems you look at is a filter.

What goes away in standalone mode: `ops.jsonl` as the saved history, the audit hash chain, the baseline store, the History and Versions pages. Git replaces them: a commit is a history entry, a tag is a baseline, a signed commit is the audit proof. The app shows the *meaning* of a commit with the existing project diff ("Th-1 risk went from Medium to High because Nd-19 changed"), which plain git cannot.

Server mode (D3) is not designed here. Consultancy-first (D21, D22) means standalone is what is used. When a customer orders a server, the simplest answer is that the server hosts the same folder as a git repository.

## 3. Item definition: Markdown you write, structure the engine reads

The owner wants to write the item definition as text, with diagrams, and still have interfaces, functions and trust boundaries known to the tool. Both come from one file.

````markdown
# Gateway ECU

The gateway routes diagnostic and powertrain traffic between the
telematics unit and the body network. [[IF-2]] is the only external entry.

```aegis-item
component gw   "Gateway ECU"
component tcu  "Telematics unit"
component brk  "Brake ECU"          system=brake-ecu

function  route   "Route CAN frames"         on gw
function  diag    "Forward diagnostic jobs"  on gw

interface IF-1 "CAN powertrain"  gw -- brk
interface IF-2 "Ethernet DoIP"   tcu -- gw      when ethernet

flow  "Diagnostic request"  tcu -> gw  via IF-2
boundary "Vehicle internal" { gw brk }
```

## Out of scope
The backend is analysed in its own system.
````

- Prose is plain Markdown. `[[IF-2]]` is a live link to an element: hover shows it, rename updates it.
- The fenced `aegis-item` block **is** the model. It is parsed into the item elements `aegis-model` already has (component, interface, data flow, trust boundary; D15 step 2). Functions are one new element type.
- The same block **is** the diagram. `aegis-canvas` lays it out and draws it (D15 step 3), in the editor preview and in the PDF. Dragging a box in the preview only changes layout hints, never the meaning.
- `when ethernet` ties a part to a condition (D27).
- The editor is a split: source left, rendered right, like Zed's Markdown preview. A form-only user can stay on the right side and never see the block.

Not proposed: Mermaid. It needs a JavaScript engine or a partial Rust port, and its boxes mean nothing to the engine. Pictures from the customer still go in as images (D15 step 1).

**PDF engine: Typst** (Apache-2.0, a Rust library, no external program). Markdown is converted to Typst markup, diagrams go in as SVG from the adapter `aegis-canvas` already has (`svg.rs`), and a report template is a Typst file a customer can brand. This replaces hand layout on `krilla` and gives the report builder (D17) its template language. Japanese text works with the Noto font already planned (D13). Cost: a large dependency; measure the binary size in the spike.

## 4. Configuration: layers, like Zed settings

A value is looked up through layers, the nearest wins:

```
methodology pack  →  template (library)  →  workspace (vehicle)  →  system
```

```toml
# systems/gateway/system.toml
name    = "Gateway"
extends = "library:gateway-base@2"      # pinned version

[conditions]
ethernet = true
oem_diag = false

[controls.secure-boot]
status = "off"                          # D28: implemented | planned | off

[[controls.add]]
id   = "CTL-40"
name = "Locked enclosure"
kind = "physical"
caps = { physical-access = "low" }      # D26: attack type → feasibility ceiling
```

- This is D26 to D28 with a file format. The engine rules do not change.
- The settings screen shows the **effective** value and its source: "Secure boot: Off. Set in gateway/system.toml. Template says Implemented." One click removes the override.
- The same screen design serves app settings, workspace settings and system settings. One thing to learn.
- **Vehicle view:** the workspace overview adds up the systems. Switching a control off in one system shows at once which vehicle-level risks move, with the explain-why chain.
- **Compare:** two configurations side by side (with and without Ethernet), changed numbers highlighted. Today that means copying the project.

## 5. Templates and the library

- A template is a system folder in a library: `ecu-base`, `gateway-base`, `network-device-base`, `sensor-base`, `backend-base`. It holds a starter item definition, assets, threats, trees and controls.
- Libraries are folders in three places, searched in this order: the workspace (`./library`), the user (`~/…/AegisTARA/library`), built in (shipped with the app). All are plain files, so a consultancy keeps its library in its own git repository.
- **Inherit by pinned reference plus local changes.** `extends = "library:gateway-base@2"` loads the template; the system folder holds only what differs (added, changed, removed elements). Numbers cannot change silently because the version is pinned. "Update to @3" shows the diff first. "Detach" copies everything in and cuts the link.
- This changes D14 (copy with an origin note) for whole-system templates. Copying stays the rule for single fragments dragged from the library panel.
- **Composable trees.** A tree file may include a fragment with parameters; D29 already has fragments by reference.

```toml
# trees/th-1-bus-manipulation.toml
id = "Th-1"
name = "Manipulation of bus communication"
gate = "or"

[[node]]
id = "Nd-19"
name = "Spoof messages"
gate = "and"
children = ["Lf-8", { include = "library:fragments/bypass-secure-onboard-comm" }]

[[node]]
id = "Lf-8"
name = "Inject frames on the bus"
via  = "IF-1"
when = "oem_diag"
potential = { time = 13, expertise = 3, knowledge = 7, window = 0, equipment = 0 }
```

## 6. Scripting, AI and outside APIs

**Scripts.** D20 said no loaded code, and the reason still holds (loaded code bypasses every check). Files make the rule easy to keep:

- An `aegis` command-line tool: `aegis check`, `aegis eval --json`, `aegis report`. It is the same engine with no window.
- A small Python package that reads and writes the workspace files and calls `aegis eval`. No Python is embedded in the app; the user's own Python runs it.
- **Tasks** in `aegis.toml`, run from the command palette, output in the bottom panel (Zed's tasks):

```toml
[tasks.import-can-matrix]
run = "python scripts/import_dbc.py data/powertrain.dbc"
```

- Whatever a script changes arrives as a diff to review, like any outside edit. A workspace from someone else asks "trust this folder?" before any task runs.

**AI.** D6 and D12 stay: AI proposes, the engine computes, a person decides. In standalone mode the gateway is a settings block instead of a server:

```toml
[ai.providers.local]
kind     = "openai-compatible"          # Ollama, llama.cpp, vLLM, LM Studio, a tuned model
base_url = "http://localhost:11434/v1"
model    = "tara-tuned-8b"
```

- Two adapter kinds as in D12: OpenAI-compatible and Anthropic. A fine-tuned model is just another OpenAI-compatible endpoint.
- Keys go in the OS keychain (the keyring crates are already in the workspace), never in a file.
- A proposal is a diff against the files, shown in the same review view as a script's change. Nothing leaves the machine unless the user configured an endpoint that is not local.

**User-provided APIs.** Later, and only as tools the AI may call through MCP servers listed in settings (what Zed does). Not before the first two work.

## 7. The workbench

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ vehicle-x ▾   Scope: Gateway ▾   Config: with Ethernet ▾        ⌘P   main ⎇  │
├───────────────┬──────────────────────────────────────────┬───────────────────┤
│ PROJECT       │ item.md │ Th-1 Manipulation… × │ Risks   │ INSPECTOR         │
│ ▾ systems     ├──────────────────────┬───────────────────┤ Lf-7              │
│  ▾ gateway    │                      │                   │ Medium  14        │
│     item.md   │   attack tree        │  item.md preview  │ time 0 · exp 6 …  │
│     assets    │   canvas             │  (diagram + text) │                   │
│     threats   │                      │                   │ Why Medium?       │
│   ▾ trees     │                      │                   │  └ Nd-20 AND      │
│      Th-1     │                      │                   │     └ Lf-7 ←      │
│  ▸ brake-ecu  │                      │                   │ Capped by         │
│ ▸ shared      │                      │                   │  Secure boot (off │
│ ▸ library     │                      │                   │  in this system)  │
├───────────────┴──────────────────────┴───────────────────┴───────────────────┤
│ Problems 3 │ Task output │ Changes                                           │
├──────────────────────────────────────────────────────────────────────────────┤
│ ✓ numbers checked   High 2 · Medium 7 · Low 12      Gateway    3 problems    │
└──────────────────────────────────────────────────────────────────────────────┘
```

**Structure**

- **Left dock:** Project (the folder as a tree, with element counts and risk dots), Library, Search, Changes (git).
- **Centre:** tabs and splits. Anything opens as a tab: a tree on the canvas, a table of assets, the Markdown item definition, a settings layer, a report preview.
- **Right dock:** Inspector for the selection, and the AI assistant.
- **Bottom dock:** Problems (the linter, ADR 0018), task output, pending changes to review.
- **Status bar:** the numbers-checked mark, the risk totals, the scope, the problem count. Every item is a button.
- Every panel can be moved, hidden, and resized; the layout is saved per workspace.

**Five ideas that make it its own tool, not a Zed skin**

1. **Text twin.** Every tab has a "source" toggle. The tree canvas and its TOML, the settings form and its TOML, the rendered item definition and its Markdown. Edit either side. Experts type; reviewers click.
2. **Go to anything.** `⌘P` then `Th-1`, `secure boot`, or `Lf-7` opens it. `⌘⇧P` runs any command. No one hunts through a sidebar.
3. **Go to cause.** Hold `⌘` and click any rating: the app jumps to the node, control or condition that produces that number, the way an editor jumps to a definition. The explain-why trace (ADR 0006) already has the data.
4. **Query tabs.** A saved query ("all leaves rated Expert that go via IF-2", "every threat whose risk changed since tag v1") opens as an editable table gathered from many files, like Zed's multibuffer. The risk register is just the built-in query.
5. **Scope and config in the title bar.** Two pickers decide which systems and which configuration you look at. They are always visible, so a number is never read without knowing its context.

**Look and feel**

- One UI typeface and one mono typeface with Japanese coverage: IBM Plex Sans, IBM Plex Mono, Noto Sans JP (all OFL-1.1, allowed by D5).
- Dense rows (28 px), flat surfaces separated by tone and not by borders, one accent colour. **Rating colours are the only saturated colours on screen**, so risk is what the eye finds first.
- Motion only where it explains: a panel sliding, a node moving to its new place after a change. 120 to 150 ms. No decoration.
- Light and dark themes as TOML token files. A user theme is a file in the settings folder.
- Canvas nodes keep the reference tool's good part (ID, name, level, the five factor values) and drop the icon rows that repeat on every node; factors show on hover and in the inspector.

**What happens to the 17 pages:** they become tabs you open, not places you go. The clause 15 phase order (D30) stays as the order of the Project tree and as a "Process" overview tab. The relation registry (D30) is kept; the inspector and query tabs read it. History and Versions go (section 2). Admin stays a server-only tab.

## 8. Toolkit: GPUI, with a gate

The October spike rated GPUI best on look (4.5 of 5) and rejected it for three reasons. Where they stand now:

| Reason then | Now | Answer |
|---|---|---|
| No CPU renderer; many automotive engineers use Citrix or RDP | Still no CPU renderer. Windows uses DirectX 11, which may run on the system's software adapter over RDP; not verified here. | Spike gate. Consultancy-first means the first user is the owner's laptop. |
| Unofficial pre-1.0 snapshots | The official `gpui` on crates.io is 0.2.2 and reported to be about 11 months behind Zed's main branch; several forks exist. gpui-component (Apache-2.0) says its API still changes often. | Pin one git revision of GPUI and of gpui-component. Update on purpose, not by accident. |
| Accessibility partial | Forks advertise AccessKit; the official crate's state was not confirmed. | The screen-reader path (ADR 0045) will regress at first. The owner accepts that or it is a gate. |

For it: Apache-2.0 removes the one license exception the product has today (the Slint attribution notice and badge). gpui-component already has docks with draggable tabs, virtual tables, a code editor with tree-sitter highlighting, and Markdown rendering, which is most of section 7.

One trap: **Zed's own editor and workspace crates are GPL** and cannot be linked (D5). Only GPUI and gpui-component may be used. The text editor is gpui-component's.

What the rewrite costs: `aegis-shell` only (about 6,300 lines of `.slint` and 6,000 of Rust). `aegis-client` and `aegis-canvas` were kept toolkit-free for this (D9). The canvas needs one new adapter that draws the display list with GPUI.

How to do it without a dark period: build a second shell crate next to `aegis-shell`. Slint stays the shipped app until the new one covers the consultancy's daily work, then `aegis-shell` and its license exception are deleted.

## 9. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| 1 | Files as the saved truth, git as history, in standalone mode? Drops the op log on disk, the audit chain and the baseline store there. | Yes. It is the base for sections 3 to 6. |
| 2 | Move the shell to GPUI, reversing D7 and D25? | Yes, if the spike passes on a real Windows laptop and over RDP. |
| 3 | Accept a weaker screen-reader path and no CPU-only rendering for a while? | Yes for the consultancy; revisit at the first customer deployment. |
| 4 | Scripts as outside programs over files and a CLI, no embedded Python (keeps D20)? | Yes. |
| 5 | Templates by pinned reference with local changes (changes D14 for whole systems)? | Yes. |

## 10. Plan

Each phase ships alone and has a test that says it is done.

| Phase | What | Done when |
|---|---|---|
| 0. Spike (1 to 2 weeks) | GPUI window with a dock, the tree canvas from `aegis-canvas`, an inspector, a Markdown editor tab, Typst PDF of one page. Run on macOS, a Windows laptop, and Windows over RDP. | Frame time, memory, binary size and the RDP result are written down; decisions 1 to 5 get D numbers. |
| 1. Workspace format | Reader and writer for the folder in `aegis-model`; the `aegis` CLI (`check`, `eval`, `convert`). No UI change. | An existing project converts to a folder and back with identical numbers; a property test round-trips random projects. |
| 2. Workbench skeleton | New shell crate: docks, Project tree, tabs and splits, command palette, go to anything, settings with layers, themes. Tree canvas tab and inspector. | A tree can be opened, edited, undone and saved with no mouse. |
| 3. Item definition | Markdown editor with preview, the `aegis-item` block, `[[links]]`, diagram from `aegis-canvas`, Typst PDF. | The sample project's item definition renders to a PDF whose diagram matches the screen. |
| 4. Tables and parity | Table tabs, query tabs, Problems, Changes with the meaning diff, reports. Then delete `aegis-shell`. | The owner does one real TARA end to end in the new shell. |
| 5. Templates and configuration | Library folders, `extends` with local changes, the five base templates, the vehicle view, compare. | A new gateway system from the template, with one control off, changes the vehicle totals and says why. |
| 6. Tasks and AI | Tasks, the Python package, AI providers, proposals as diffs. MCP tools after that. | A local model proposes three threats for an interface and they land as a reviewable diff. |

Phase 1 comes before the toolkit work on purpose. It is needed whichever toolkit wins, it is testable without a window, and it gives git history and scripting to the current app.

## 11. Not proposed

- Embedded Python, a plugin marketplace, a built-in terminal, a full git client (history, diff and commit are enough), Mermaid, real-time co-editing.
- Server mode changes. Frozen until a customer orders one.
- Porting the Admin, license and session pages early.

## 12. Risks

| Risk | Limit it by |
|---|---|
| GPUI breaks between revisions | Pinned revision; the shell is thin; the display list and client core do not depend on it. |
| Two shells for some months | Freeze Slint features on the day phase 2 starts; fix only bugs there. |
| Hand-edited files that are wrong | The loader reports the file and line; a broken file never loses the last good state in memory. |
| Merge conflicts in TOML | One element per table, one tree per file, stable order. The meaning diff shows what a merge did to the numbers. |
| Template update moves numbers | Pinned versions; an update is always a reviewed diff. |
| Scripts from a received workspace | Nothing runs before the user trusts the folder. |

## Sources checked for section 8

- gpui-component repository and docs: <https://github.com/longbridge/gpui-component>, <https://longbridge.github.io/gpui-component/docs/>
- State of the official `gpui` crate versus Zed main (a single secondary source; confirm on crates.io in the spike): <https://baeseokjae.github.io/posts/native-app-for-coding-agents-rust-gpui/>
- Zed on Windows and DirectX 11: <https://www.theregister.com/2025/08/22/everything_is_different_on_windows/>
- Our own spike: [spikes/RESULTS.md](../spikes/RESULTS.md)
