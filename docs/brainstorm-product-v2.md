# Brainstorm, round 2: AI, languages, library, UI, traceability, market

- Status: brainstorm with decisions. The decisions are recorded as D12 to D19
  in [decisions.md](decisions.md); the requirements are in
  [requirements.md](requirements.md); the work is in
  [`factory/backlog.md`](../factory/backlog.md) (jobs 060 onward); the planned
  system is Part III of [architecture.md](architecture.md).
- Date: 2026-10-26
- Builds on: [brainstorm.md](brainstorm.md) (round 1),
  [ai-backlog.md](ai-backlog.md), [brainstorm-attack-tree-library.md](brainstorm-attack-tree-library.md),
  [brainstorm-logs-and-reports.md](brainstorm-logs-and-reports.md),
  [security-and-architecture-review.md](security-and-architecture-review.md).

## TLDR

1. **AI: bring your own model first, our own small model later.** One AI
   gateway on the customer's server talks to any model the customer
   chooses (their API key for a cloud model, or an open-weight model they run
   themselves). A small tuned model of our own is an optional add-on once we
   have a public training set and a measured golden set. The rule stays: AI
   proposes, the engine computes, a human decides.
2. **Japan is the first export market.** That means a Japanese UI, Japanese
   PDF fonts, Japanese reports, a local partner, and a security check sheet
   answer pack. The UI language and the content language are two different
   things and are built separately.
3. **The attack tree library is the reuse engine.** Copy with a remembered
   origin; search by words, typos, topics and context without AI; meaning
   based search with a small local embedding model later. Trees need
   node-level operations and a much higher node cap first.
4. **Clean like Notion, complete like Photoshop**: quiet pages, a command
   palette, an inspector panel, a history panel, and every control one click
   or one key away. Progressive disclosure, not hidden features.
5. **Traceability is a first-class model**: item definition (components,
   interfaces, data flows, trust boundaries), architecture figures, trace
   links between every element, external references to Jira, Polarion and
   JAMA, and a traceability matrix in the report.
6. **Reports are built, not just exported**: pick sections, filters,
   language, branding and classification; save the choice as a profile.
7. **Integrations: ReqIF first, live connectors second.** ReqIF works air
   gapped with Polarion, JAMA, DOORS and codebeamer. Live connectors run on
   the server, never on the laptop, because the tokens are company secrets.
8. **Biggest gaps today**: no description text on any element, no item
   definition model (in v1 scope but not built), a 32-node tree cap, no i18n,
   no fonts for Japanese, no role model (SEC-02), and our own product's EU
   Cyber Resilience Act duties.

---

## 1. AI in TARA work

### 1.1 The two ideas from the owner

| | A. Our own small model (open weights, tuned) | B. Customer brings key and model |
|---|---|---|
| What it is | We pick an open-weight model (1 to 14 B parameters), tune it on TARA material, ship it as an optional download | The customer configures a provider: their cloud API key (Anthropic, OpenAI, Azure OpenAI, Bedrock, Vertex) or their own model server (vLLM, llama.cpp, Ollama) |
| Data sovereignty | Best: runs fully inside the customer site, even air gapped | Their choice: cloud keys send data out under their contract; a self-hosted model keeps it in |
| Quality | Limited by size. Small models invent facts easily; tuning helps style and format more than knowledge | Large frontier models are much better at reasoning and writing |
| Our cost | Training data, GPUs, evaluation, a model release process, model license review, support for GPU sizing at customer sites | An adapter per API family (two cover almost everything) |
| Customer cost | GPU hardware at their site | Their API bill, or their own GPU |
| Time to value | Months | Weeks |
| Training data | **Hard.** Customer TARAs never reach us (D3). Only public material (R155 Annex 5, CAPEC, ATT&CK, EMB3D, public example TARAs) and synthetic data reviewed by experts | None needed |
| Risk | A weak model hurts the product's name | Bad output from a provider we do not control |

### 1.2 Decision (D12): B first, A as an optional add-on, one gateway for both

- **Build B first.** The AI gateway from [ai-backlog.md](ai-backlog.md) is
  the single place where AI happens. It has two adapter families:
  1. **OpenAI-compatible HTTP** (covers OpenAI, Azure OpenAI, vLLM,
     llama.cpp server, Ollama, LM Studio, most Japanese and EU sovereign
     hosting offers).
  2. **Anthropic Messages API** (covers Anthropic directly, and Claude on
     Bedrock and Vertex through their endpoints).
  More adapters only when a customer order names one.
- **Keys live on the server**, encrypted at rest, set by an admin, never on
  the laptop, never in a project file, never in logs. Each provider entry
  shows its data policy ("may leave the company: yes/no") and the model
  license.
- **A is a product option, later.** When we have (a) the golden set, (b) a
  public and synthetic training set, and (c) wave 1 features measured on B,
  we tune a small Apache-2.0 model with LoRA and ship it as an "AegisTARA
  Assist" image the customer runs with vLLM or llama.cpp. It plugs into the
  same OpenAI-compatible adapter, so no product code changes.
- **Customers may tune their own adapter** on their own approved TARAs, on
  their own hardware (wave 4). We supply the export of training pairs and a
  recipe, never their data.

Why not A first: tuning a small model teaches it our output format and tone,
but not facts it lacks; it still needs retrieval for facts. Retrieval plus a
strict output schema plus the engine as a tool gives most of the value with
any model, including a small one. So the first investment is the gateway,
retrieval and output checks, and the model is replaceable.

### 1.3 What makes small models good enough here

- **Constrained output.** Every AI feature asks for JSON that matches a
  schema (llama.cpp grammars, vLLM guided decoding, OpenAI structured outputs,
  Anthropic tool input). Output that does not parse is dropped, not repaired.
- **Grounding.** Retrieval over the catalogs, the library and (with
  permission) the company's approved projects. Every suggestion cites catalog
  IDs or element IDs; a citation that does not exist rejects the suggestion.
- **Engine as a tool.** The model never computes a feasibility or a risk. It
  asks the engine, or it proposes a rating with reasons and the engine
  computes the rest.
- **Small task size.** "Propose damage scenarios for this asset" is a small
  task. "Write the whole TARA" is not offered.

### 1.4 Where AI helps (mapped to the TARA flow)

| TARA step | AI feature | Ships in |
|---|---|---|
| Item definition | Read an architecture document or figure caption, propose components, interfaces, data flows (A7) | Wave 3 |
| Assets | Propose assets and properties for a component | Wave 2 |
| Damage scenarios | Draft wording and an impact proposal with reasons (A3) | Wave 1 |
| Threat scenarios | Propose threats per asset with STRIDE and catalog citations (A2) | Wave 1 |
| Attack trees | Draft a tree from library items into a suggestion branch (A4); find similar library items (A1) | Wave 1 (search), wave 2 (draft) |
| Feasibility | Propose attack potential factors with reasons, show how similar steps were rated (A5) | Wave 2 |
| Treatment, goals, claims | Propose goals and controls mapped to R155 Annex 5 mitigations (A6) | Wave 3 |
| Review | AI reviewer next to the linter (B1); duplicate finder (B2) | Wave 2 |
| Documentation | Report summary wording with a number check (D1); plain-language explain-why (D2); **evaluation rationale text** for each rating | Wave 1 |
| Language | Translate element text EN to JA and back (B4) | Wave 2 (moved up for Japan) |
| Outside the app | MCP server so the company's own assistants can read and propose (E4) | Wave 3 |

"Writing the evaluation and related documentation" (owner request) is the
documentation row: rationale text per rating, summary wording, and
translation. All of it lands as text proposals the user accepts.

### 1.5 Hooks that are still missing in the model

[ai-backlog.md](ai-backlog.md) section 7 lists hooks v1 should have. Checked
against the code on 2026-10-26:

| Hook | State |
|---|---|
| Provenance with `Ai` source | Built |
| Typed operation API | Built |
| Catalog IDs on threats | Built (threats only; not on steps or controls) |
| Engine callable as a tool | Built (library API; no RPC for "evaluate this proposal") |
| Derivation traces | Built |
| Audit log | Built |
| **Suggestion branches** | **Missing** |
| **Language tag on text fields** | **Missing** (and there are no long text fields at all) |

### 1.6 Model licenses and data

- Every model is reviewed under D5 before it is listed. Apache-2.0 and MIT
  weights pass. Custom licenses (Llama community license, Gemma terms) need a
  review per model. Japanese-capable open models exist in several license
  families; pick by license first, quality second.
- The embedding model for search must be **multilingual** (an English query
  must find a Japanese library item). Candidates with permissive licenses
  exist (for example the multilingual E5 and BGE-M3 families); measure first.
- Embeddings run in the server process (pure Rust inference, for example
  `candle`) or in the customer's model server. Store vectors in memory first
  (brute force cosine is fast enough up to tens of thousands of items);
  `pgvector` only when a library outgrows that, after a D5 review of the
  extension.

---

## 2. Languages and markets

### 2.1 Three different language problems

| Problem | Example | Where it lives |
|---|---|---|
| **UI language** | Menus, buttons, messages, page titles | Compiled into the app. Slint `@tr()` with bundled gettext catalogs for `.slint` markup; a small catalog in `aegis-client` for text made in Rust (pages, errors, reports) |
| **Methodology language** | "Severe", "Major", impact category names, risk levels | The methodology pack: each label gets a per-language form (`label: {en, ja}`) |
| **Content language** | The user's own asset names, threat descriptions | The project. Each text field carries a language tag and may carry translations, so a report can be English, Japanese or bilingual |

The "language backend" the owner mentioned is the third row plus an optional
translation service: the AI gateway (feature B4) or a machine translation
service the customer already licenses, behind the same gateway policy. UI
catalogs never need a server.

### 2.2 Decision (D13): i18n design

- UI: Slint `@tr()` everywhere in `.slint` files, catalogs bundled into the
  binary (no files to lose on air-gapped machines). English and Japanese
  first; German and Chinese (simplified) next by demand.
- Rust-side text (`aegis-client` pages, error messages, report text): one
  catalog per locale in `aegis-client`, Fluent format (`fluent-bundle`,
  Apache-2.0/MIT) because it handles plural rules for EU languages. A test
  fails when a key is missing in any shipped locale.
- Server error messages stay codes plus English; the client translates by
  code (rule R6 already keeps entity names out of messages).
- Locale-aware dates (`2026年10月26日` or `2026-10-26`, user choice),
  numbers, and sort order.
- Fonts: the UI falls back to the system CJK font (Yu Gothic UI, Meiryo,
  Hiragino, Noto); the PDF embeds a subset of Noto Sans JP (OFL-1.1, allowed
  by D5; krilla subsets fonts). Inter has no Japanese glyphs, so **a Japanese
  PDF is broken today**.
- Text input: Japanese IME must work in every text field. Slint's winit
  backend has IME support; it must be tested on Windows with Microsoft IME
  and Google Japanese Input.
- Layout: German text runs about 30 percent longer than English; Japanese is
  shorter but needs more line height. Every page gets a "pseudo-locale" test
  (long strings) and a Japanese screenshot test.

### 2.3 Japan specifics (first market)

- **Regulation:** Japan applies UN R155 and R156 through MLIT type approval,
  so the R155 Annex 5 pack applies as is. JASPAR publishes industry
  cybersecurity guidance used by Japanese OEMs and suppliers; check what can
  be referenced without copying text (direction section 11 copyright rule).
- **Buyers:** OEMs, Tier 1 and Tier 2 suppliers, and engineering service
  companies. Sovereign, on-premise deployment (D3) fits well.
- **How they buy:** through a local partner (a distributor or a systems
  integrator), with a proof of concept first, quotes and invoices in JPY,
  Japanese contract terms, and support in Japanese during JST hours.
- **What they ask for before buying:** a security check sheet (vendor
  security questionnaire), often ISO 27001 status of the vendor, product
  security documentation, an SBOM, a vulnerability handling policy, and
  Japanese manuals.
- **Report culture:** cover page, revision history table, approval stamps
  area, classification marks (for example 社外秘), and Excel. Many
  existing TARAs are Excel workbooks, so **Excel import** is a sales feature.
- **Privacy:** APPI (Japan's personal information law) for the few personal
  data we hold (user names, logins).

### 2.4 EU and US

- **EU:** GDPR for account data; the **Cyber Resilience Act** applies to
  AegisTARA itself as a product with digital elements: vulnerability
  reporting duties from September 2026, full duties (CE marking, SBOM,
  security updates for the support period) from December 2027. TISAX matters
  only if we ever host customer data (a SaaS option).
- **US:** ISO/SAE 21434 plus NHTSA best practice guidance; FedRAMP only if a
  government customer appears (not planned).
- **All markets:** export control classification of the product (it contains
  cryptography); ask a trade lawyer once, before the first export sale.

---

## 3. Attack tree reuse: library, loops, fuzzy search, suggestions

The library note ([brainstorm-attack-tree-library.md](brainstorm-attack-tree-library.md))
asked eight questions. Decisions (D14):

| # | Question | Decision |
|---|---|---|
| 1 | Copy or reference | **Copy with an origin note** (library item ID, version, node mapping). The engine never sees a reference |
| 2 | 32-node cap | **Raise it** after adding node-level tree operations (see below). Target 4,096 nodes per tree; the job measures the 5 ms edit budget and op-log size and may pick a lower number with numbers |
| 3 | Do ratings travel | User picks at insert; default **with ratings marked "from library, review"** for company items, **structure only** for public items. The linter flags unreviewed library ratings |
| 4 | Merge identical step names | **Never silently.** Show the matches, the user ticks the ones to share |
| 5 | Who publishes company items | The **Methodology Owner** role (needs the role model, job 058) |
| 6 | First content | A small built-in demo pack written by us from public sources (R155 Annex 5, CAPEC); no ISO text |
| 7 | Page or panel | A **Library panel** docked next to the canvas, plus a full Library page for curators |
| 8 | Phase 0 enough for a demo | Yes, with fuzzy search and preview |

### 3.1 Why the cap is 32, and how to lift it

ADR 0014 caps project trees at 32 nodes so that one `AddAttackTree` op (a
whole tree) fits in a 64 KiB op-log line. The fix is **node-level
operations** (`AddTreeNode`, `LinkTreeNode`, `UnlinkTreeNode`,
`RemoveTreeNode`, `RenameTreeNode`, `SetGate`), each small, with a batch for
"insert fragment" applied atomically and undone in one step. Then the per-tree
cap is no longer tied to the line size. This is also what the outline editor,
the library and AI drafting all need.

### 3.2 "Technical attack trees and attack loops"

Interpretation to confirm with the owner:

- **Technical attack trees:** trees whose leaves are concrete technical steps
  (interfaces, protocols, tools), as opposed to goal-level trees. They are
  the most reusable library content because the same ECU or bus attack
  recurs across products. Tag them by interface type (CAN, Ethernet, OBD,
  Bluetooth, cellular, USB, debug port) and asset type.
- **Attack loops:** repeated sequences (recon, foothold, escalate, pivot,
  repeat on the next ECU). The engine stays a DAG (rule A3, cycle detection),
  so a loop is stored as a **template fragment with a repeat count**, unrolled
  into a `SeqAnd` chain at insert time, with the origin note remembering the
  loop. The report can show it folded.

### 3.3 Fuzzy and topic search without AI (works air gapped)

- **Fuzzy text:** character n-grams (bigrams and trigrams) over name, tags,
  notes and catalog IDs, scored and sorted deterministically (P4). N-grams
  work for Japanese without a word splitter. Typos and partial words match.
- **Topics:** every item carries facets: STRIDE category, interface type,
  asset type, catalog IDs (R155 Annex 5, CAPEC, ATT&CK, EMB3D), lifecycle
  phase. The panel shows facet chips; picking one narrows the list.
- **Context suggestions:** when the user is on a threat for an asset of type
  "gateway ECU" with interface "CAN", the panel ranks items whose facets
  match, with a short "why suggested" line. Rule-based, explainable.
- **Usage signals (company library only):** "used in N projects", "often used
  together with X". Counted on the server; never leaves the company.
- **Saved on use:** the owner asked that every use is saved to the library.
  Decision: not automatic. After an insert or a large edit, the app offers
  "Save as library item" with the origin filled in. Automatic saving fills
  the library with near-duplicates and leaks project wording into a shared
  space.

### 3.4 Meaning based search and drafting (AI, later)

Embeddings (multilingual) add "find items that mean the same" (A1). AI
drafting (A4) proposes a tree assembled from approved library items into a
suggestion branch. Both use the same library, so library quality is the
lever.

---

## 4. UI: clean like Notion, complete like Photoshop

The two goals conflict only if every control is visible at once. The answer
is **progressive disclosure with fast reach**.

| Pattern | Notion-like (clean) | Photoshop-like (complete) |
|---|---|---|
| Command palette (Ctrl+K / Cmd+K) | One search box for every page, element and command | Every command, with its shortcut shown |
| Inspector panel (right) | Hidden until something is selected | Every property of the selection, editable, grouped, with explain-why |
| Context toolbar | Appears over the selection only | Gate type, rating, mitigation, library, AI actions |
| History panel | Collapsed by default | Every op, click to preview, undo to a point (the op log already exists) |
| Layers / outline panel | | Tree outline, item definition outline |
| Split view | | Canvas next to table, two trees side by side, baseline compare |
| Density | Comfortable default | Compact mode for power users |
| Keyboard | Arrow keys, Enter to edit, Esc to leave | A shortcut for every command, a cheat sheet (?), remappable later |
| Empty states | One sentence and one button | A link to the library and to the sample project |
| Status bar | | Connection, lease, "numbers checked", lint count, language |

Polish details that make it feel solid:

- One spacing grid (4 px), one type scale, one icon set (Lucide, already in
  use), tokens in `theme.slint`, light and dark themes.
- Every list virtualized; every action under 100 ms or a visible progress
  state.
- Undo for everything, including library inserts and AI accepts.
- Error text says what happened and what to do, in the user's language.
- Resizable panels remember their size per user.
- Slint has no docking framework. Build resizable split panes with a fixed
  set of panel slots (left, right, bottom) instead of free docking; free
  floating windows are not worth the cost.

---

## 5. System architecture diagrams and the item definition

### 5.1 Today

The direction paper puts the item definition (functions, components,
interfaces, data flows, trust boundaries) in v1 scope. **None of it is
built**: assets are a flat list with no component they belong to.

### 5.2 Decision (D15): three steps

1. **Figures:** attach images (PNG, JPEG; SVG after sanitizing to static
   shapes) with a caption to the project or to any element; include them in
   reports. Binary storage on the server with size caps (R3). Small job,
   immediate value: customers already have architecture pictures.
2. **Item definition model** in `aegis-model`: `Component`, `Interface`,
   `DataFlow` (from, to, interface, protocol), `TrustBoundary`, plus
   `Asset.component`. Neutral names; packs can rename them (D1, D8). Ops,
   store, diff, lint ("asset not bound to a component", "data flow crosses a
   trust boundary with no threat").
3. **Architecture canvas:** a data-flow diagram drawn by `aegis-canvas`
   (same display list, same PDF and SVG pipeline). Positions are stored as
   layout data in the project (user-placed), with an auto-layout button. The
   diagram is a figure in the report and a navigator: click a data flow, see
   its threats.

Imports later, by customer demand: draw.io XML, SysML v2, AUTOSAR ARXML,
Microsoft Threat Modeling Tool files.

---

## 6. Traceability

### 6.1 What auditors and customers trace

```
Item definition -> Asset -> Damage scenario -> Threat scenario -> Attack tree / steps
      -> Risk -> Treatment -> Cybersecurity goal -> Requirement -> Control -> Verification
                                   \-> Claim (retain, share)
```

Most of the middle exists today as typed links. Missing: the item definition
at the start, **requirements** and **verification** at the end, and links
out to other tools.

### 6.2 Decision (D16): trace model

- **Typed internal links stay typed** (the engine needs them). A generic
  `TraceLink { from, to, kind }` is added only for links the engine does not
  use (for example "requirement refines goal", "test verifies requirement").
- **External references** on every element: `ExternalRef { system, key,
  url, title, synced_at, status }` where system is Jira, Polarion, JAMA,
  ReqIF or URL. Stored in the project, versioned with it.
- **Suspect links:** when the source of a link changes after the link was
  last confirmed, the link is marked suspect until someone confirms it (the
  same idea as in Polarion and JAMA). The linter lists suspect links.
- **Traceability matrix page and report section**, with coverage numbers
  ("27 of 30 goals have a requirement", "2 requirements have no
  verification").
- **Requirement** and **Verification** become elements (neutral names;
  packs can rename them).

---

## 7. Report generation

Today: HTML, PDF (with tree pictures) and a CSV risk register, with fixed
sections.

Decision (D17), the report builder:

- **Sections** are picked with check boxes from a list (cover, revision
  history, scope and item definition, architecture figures, assets, damage
  scenarios, threats, attack trees, risks, treatment, goals and claims,
  requirements, traceability matrix, open lint findings, baseline diff,
  appendix with methodology and explain-why).
- **Filters:** by risk level, by asset, by component, by baseline.
- **Language:** English, Japanese, or bilingual (side by side text where a
  translation exists).
- **Branding and marks:** logo, company name, classification mark on every
  page, document ID, revision, approver names.
- **Profiles:** a set of choices saved per project or per company ("OEM
  submission", "internal review", "supplier CIA").
- **Formats:** PDF and HTML from one document model (ADR 0044 follow-up),
  plus **XLSX** (the direction paper promised Excel; CSV is not Excel for a
  Japanese reviewer). Word (DOCX) only when a customer asks.
- The report records which profile, baseline and app version made it, and
  the export audit event stores the profile.

---

## 8. Integrations: Jira, Polarion, JAMA

### 8.1 Decision (D18): ReqIF first, then server-side connectors

| Step | What | Why first |
|---|---|---|
| 1 | **ReqIF export and import** of goals, requirements, controls, claims, with stable identifiers | One standard file works with Polarion, JAMA, DOORS, codebeamer; air gapped; no credentials |
| 2 | **Jira connector**: create or link an issue for a requirement or a control; pull its status | Jira is everywhere, including at Japanese suppliers |
| 3 | **Polarion connector** (REST on current versions) | Common at EU OEMs and Tier 1s |
| 4 | **JAMA connector** (REST) | Common in US and some EU programs |

Connector rules:

- Runs in the **server**, with credentials stored there (admin set,
  encrypted), because the server sits inside the company network and the
  tokens are company secrets.
- **Push and link, not full two-way sync.** AegisTARA owns the analysis; the
  ALM tool owns the requirement lifecycle. We push new items, store the
  external key, and pull status for display. Field mapping is configured per
  project.
- Every push is an audit event. A failed push never loses the local edit.
- Rate limits and timeouts per remote (R6), bounded queues (R4).

---

## 9. Marketing and distribution

### 9.1 Packaging

| Edition | Content | License features (already in the license token) |
|---|---|---|
| Team | Core TARA, reports, built-in library, password or one SSO | `core` |
| Professional | Company library, report profiles, ReqIF, XLSX, item definition | `core`, `library`, `reqif`, `reports-pro` |
| Enterprise | Live connectors, AI gateway, SSO of choice, priority support | all, plus `ai`, `connectors` |

Named-user annual subscription; the server is free to install. Trials use a
30-day license file. Academic and partner licenses are cheap feature sets.

### 9.2 Channels

- **Direct** in the EU (German-speaking market first).
- **Japan through a partner** (distributor or systems integrator) with a
  Japanese website, manuals and support in JST hours.
- **Cloud marketplaces** (AWS, Azure) list the server container for
  customers who host there; still single tenant, in their account.
- **Events:** escar Europe, escar Asia (Tokyo), the JSAE automotive
  engineering exposition in Japan, and supplier days at OEMs.
- **Content:** public sample TARA, a free library pack, short videos in
  English and Japanese, a "move from Excel TARA" guide with the import tool.

### 9.3 Distribution mechanics

- Windows first (D2): signed MSI (EV code signing certificate, so SmartScreen
  does not warn), then macOS (notarized), then Linux (AppImage or deb).
- Offline bundle for air-gapped sites: installer, server image tarball,
  license file, update manifest, checksums.
- Updates: the signed manifest and installer download already exist (ADR
  0042, 0048); production keys are still missing (G13).

### 9.4 Optional SaaS

A vendor-hosted single-tenant instance (EU region and Tokyo region) for
small customers and trials would shorten sales a lot. It changes D3 (we
would hold customer data), brings TISAX and ISO 27001 into scope, and needs
the horizontal scaling work (SEC-12). **Not decided; owner call.**

---

## 10. Other ideas worth keeping

- **Excel TARA import** with a column mapping wizard (most customers start
  from Excel).
- **Command line tool** (`aegis report --profile ...`) for CI pipelines that
  publish the TARA report on every release.
- **Tool qualification kit**: customers working under ISO 26262 and ISO/SAE
  21434 ask for evidence that the tool computes correctly. Our property tests,
  explain-why traces and determinism are the evidence; package them as a
  document.
- **Sample projects** in English and Japanese (an ECU, a gateway, a
  telematics unit).
- **Variants**: one item, several markets or trims, shared analysis with
  deltas (direction "later" list).
- **Living TARA**: vulnerability feeds against attack steps (AI wave 3).
- **Co-editing** beyond the one-editor lease (D4), once module-level locks
  are needed by a customer.

---

## 11. Questions for the owner

1. Is "attack loop" the repeated recon-to-pivot sequence (section 3.2), or
   something else?
2. Which partner model for Japan: a distributor, a systems integrator, or a
   Japanese subsidiary later?
3. Is a vendor-hosted SaaS option (section 9.4) wanted, given it changes D3?
4. Which ALM tool does the first customer use (decides the first live
   connector)?
5. Who translates: a professional translator per release (recommended for
   the UI and manuals), or machine translation with review?
6. D4 and D7 are still open (decisions.md). D7 matters for distribution: the
   Slint royalty-free license needs an attribution badge on the download
   page; a commercial Slint license removes that.
