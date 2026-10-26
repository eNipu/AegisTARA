# AegisTARA requirements (v2)

- Status: accepted as the plan of record; the owner can change any line.
- Date: 2026-10-26
- Sources: [brainstorm-product-v2.md](brainstorm-product-v2.md) (why),
  decisions D12 to D19 in [decisions.md](decisions.md),
  [direction-v1.md](direction-v1.md) section 11 (v1 scope),
  [security-and-architecture-review.md](security-and-architecture-review.md)
  (SEC-01 to SEC-12), [ai-backlog.md](ai-backlog.md).
- Jobs: every requirement names the `factory/backlog.md` job that delivers it.
  A requirement with no job is not planned yet.

Priority: **M** must (blocks the phase), **S** should, **C** could.
Phase: **4** harden and fill model gaps, **5** Japan-ready product, **6**
traceability and integrations, **7** AI. Phases 1 to 3 are done
([status-and-roadmap.md](status-and-roadmap.md)).

---

## 1. Gap analysis (checked against the code on 2026-10-26)

| # | Gap | Evidence | Why it matters | Closed by |
|---|---|---|---|---|
| G20 | Open security findings: 2 High, 5 Medium, 3 Low | `security-and-architecture-review.md` | SEC-02 (any user edits any project) blocks every customer deployment | 058, 060 to 063 |
| G21 | No description or notes text on any element; names only (256 chars) | `MAX_NAME_CHARS`, no `description` field in `aegis-model` | A TARA cannot hold its rationale; reports and AI have nothing to write into | 064 |
| G22 | Item definition (components, interfaces, data flows, trust boundaries) missing, though in v1 scope | `direction-v1.md` section 11; no such types in `aegis-model` | No architecture to trace from; assets float | 074 |
| G23 | Attack trees capped at 32 nodes; edits replace whole trees | `MAX_ATTACK_TREE_NODES = 32` (ADR 0014); no node-level ops among the 37 `Op` variants | Real technical trees and library fragments do not fit | 065 |
| G24 | No i18n: UI text hard-coded (about 226 `text:` lines in `app.slint`), no catalogs | no `@tr(` in the repo | No Japanese UI | 066, 068 |
| G25 | PDF font (Inter) has no Japanese glyphs | `crates/aegis-client/assets/fonts` | A Japanese report renders as empty boxes | 067 |
| G26 | No content language tag or translations | `ai-backlog.md` section 7 hook missing | No bilingual reports, no AI translation target | 069 |
| G27 | Report sections are fixed; HTML and PDF have separate models | `report.rs`, `pdf.rs`; `brainstorm-logs-and-reports.md` section 3 | Customers need submission-specific reports | 070, 071 |
| G28 | Excel export promised, only CSV built | `direction-v1.md` section 11; `ExportKind::RiskRegister` | Japanese and EU reviewers work in Excel | 072 |
| G29 | No figures or binary attachments | no attachment store | Architecture pictures cannot reach the report | 073 |
| G30 | No requirements, verification, generic trace links or external references | model has goals and claims only | No traceability to ALM tools | 076, 077, 078 |
| G31 | No ReqIF, no Jira, Polarion or JAMA connection | none | Customers' requirement flow stops at our border | 079, 094 to 096 |
| G32 | Suggestion branches (a D6 hook) not built | no suggestion type in `aegis-model` | AI output has no safe landing zone | 089 |
| G33 | No attack tree library | `brainstorm-attack-tree-library.md` | No reuse | 057, 087 |
| G34 | No role model or project membership | SEC-02; direction section 6 | Library publishing, AI policy and connectors all need roles | 058 |
| G35 | No migration path from Excel TARAs | none | The main sales objection | 080 |
| G36 | Power-user UI missing: command palette, resizable panel slots, history panel, bulk edit (an inspector panel exists) | `app.slint` | "Feels like a solid tool" needs these | 081 to 085 |
| G37 | Our own product's EU Cyber Resilience Act duties not planned (SBOM, vulnerability handling, support period) | none | Reporting duties apply since September 2026; full duties December 2027 | 101 |
| G38 | No signed installers, no production keys, no offline bundle | G13 in the roadmap; ADR 0034 | Cannot ship to a customer | 099, 100 |
| G39 | No Japanese or English user manual, no product security pack for vendor questionnaires | `docs/` holds engineering docs only | Japanese buyers ask for both before a PoC | 102, 103 |
| G40 | Reviews of jobs 027 to 056 still pending | `review-requirements.md` | Known unknowns before customers | Roadmap item 1 |
| G41 | D4 and D7 unconfirmed | `decisions.md` Open | D7 decides the download page and the Slint license cost | Owner |

---

## 2. Functional requirements

### 2.1 Model foundations (FND)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| FND-01 | Every element has a `description` (long text, capped, plain text) and the project has notes; both are versioned, diffed, exported | M | 4 | 064 |
| FND-02 | Attack trees are edited with node-level ops (add, link, unlink, remove, rename, set gate); a multi-node insert is one atomic batch and one undo step | M | 4 | 065 |
| FND-03 | The per-tree node cap is raised (target 4,096) with the 5 ms edit budget (A1) and the op-log line cap still met, measured by benchmarks | M | 4 | 065 |
| FND-04 | Text fields carry a language tag and optional translations (capped count) | S | 5 | 069 |
| FND-05 | A suggestion branch holds proposed ops (from AI or the library) apart from the project until accepted; accept and reject are ops with provenance | M | 7 | 089 |

### 2.2 Access (ACC)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| ACC-01 | Built-in roles (Admin, Methodology Owner, Project Owner, Analyst, Reviewer, Viewer) and project membership enforced on the server for every RPC (fixes SEC-02) | M | 4 | 058 |
| ACC-02 | Roles map from SSO groups when company login is used | S | 4 | 058 |
| ACC-03 | Library publishing, AI use and connector use are permissions in the same table | M | 4 | 058 |

### 2.3 Languages (L10N)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| L10N-01 | All UI text goes through translation (`@tr` in Slint, a Fluent catalog in `aegis-client`); catalogs are bundled in the binary | M | 5 | 066 |
| L10N-02 | The user picks the UI language (default from the OS); it switches without losing work | M | 5 | 066 |
| L10N-03 | A test fails when a shipped locale lacks a key; a pseudo-locale screenshot test catches clipped text | M | 5 | 066 |
| L10N-04 | Japanese text renders in the UI (system CJK fallback) and in the PDF (embedded, subset Noto Sans JP); Japanese IME works in every field | M | 5 | 067 |
| L10N-05 | Dates and numbers follow the locale; the date style is a user setting | S | 5 | 067 |
| L10N-06 | Full Japanese translation of the UI, reports and the built-in methodology pack labels | M | 5 | 068 |
| L10N-07 | Methodology packs carry labels per language | M | 5 | 068 |
| L10N-08 | Reports in English, Japanese or bilingual | S | 5 | 069, 071 |
| L10N-09 | German and simplified Chinese catalogs | C | later | none |

### 2.4 Reports (RPT)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| RPT-01 | HTML and PDF come from one document model (no drift) | M | 5 | 070 |
| RPT-02 | The user selects sections and filters (risk level, asset, component, baseline) before export | M | 5 | 071 |
| RPT-03 | Report profiles are saved per project and per company | S | 5 | 071 |
| RPT-04 | Cover, revision history, logo, document ID, approver names, classification mark on every page | M | 5 | 071 |
| RPT-05 | XLSX export: risk register and a full workbook (one sheet per element type) | M | 5 | 072 |
| RPT-06 | Figures (architecture images, architecture canvas) and the traceability matrix are report sections | M | 5, 6 | 073, 075, 078 |
| RPT-07 | Headless report generation from a command line tool for CI pipelines | C | 6 | 104 |
| RPT-08 | DOCX export | C | later | none |

### 2.5 Item definition and architecture (ARC)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| ARC-01 | Attach PNG, JPEG and sanitized SVG figures with captions to the project or an element; size capped; stored on the server | M | 5 | 073 |
| ARC-02 | Item definition elements: component, interface, data flow, trust boundary; assets bound to components; ops, store, diff, lint | M | 5 | 074 |
| ARC-03 | Architecture diagram on the canvas (data-flow style), user-placed positions plus auto-layout, click-through to threats | S | 6 | 075 |
| ARC-04 | Import from draw.io, SysML v2, ARXML | C | later | none |

### 2.6 Traceability and integrations (TRC, INT)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| TRC-01 | External references (system, key, URL, title, sync state) on every element | M | 6 | 076 |
| TRC-02 | Generic trace links for relations the engine does not use; suspect marking when a source changes; linter lists suspect links | M | 6 | 076 |
| TRC-03 | Requirement and verification elements linked to goals and controls | M | 6 | 077 |
| TRC-04 | Traceability matrix page with coverage numbers, also a report section | M | 6 | 078 |
| INT-01 | ReqIF export and import of goals, requirements, controls and claims with stable identifiers | M | 6 | 079 |
| INT-02 | Excel TARA import with a column mapping step and a dry-run preview | M | 5 | 080 |
| INT-03 | Server-side connector framework: admin-set encrypted credentials, per-project field mapping, push and link, status pull, audit events, bounded queues and timeouts | S | 6 | 094 |
| INT-04 | Jira (Cloud and Data Center) connector | S | 6 | 094 |
| INT-05 | Polarion connector | S | on order | 095 |
| INT-06 | JAMA connector | S | on order | 096 |

### 2.7 Attack tree library (LIB)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| LIB-01 | Library packs (versioned JSON, license field, hash, decode caps) for leaves, fragments, complete trees and templates | M | 5 | 057 |
| LIB-02 | Library panel next to the canvas: fuzzy n-gram search (works for Japanese), facet chips, preview | M | 5 | 057 |
| LIB-03 | Insert as a new tree, under a gate, or in place of a leaf; copy with an origin note; one undo step; ratings travel by user choice and are flagged for review | M | 5 | 057 |
| LIB-04 | Never merge identical step names silently | M | 5 | 057 |
| LIB-05 | Company library on the server with draft, approved, deprecated states; publishing needs the Methodology Owner role | S | 6 | 087 |
| LIB-06 | Context suggestions ranked by the selected element's facets, with a "why suggested" line | S | 6 | 087 |
| LIB-07 | "Save as library item" offered after an insert or large edit | S | 6 | 087 |
| LIB-08 | Templates with slots and attack loop fragments (repeat count, unrolled at insert) | C | 6 | 087 |
| LIB-09 | Meaning based multilingual search | S | 7 | 092 |

### 2.8 UI (UI)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| UI-01 | Command palette (Ctrl+K / Cmd+K) over pages, elements and commands, with shortcuts shown | M | 5 | 081 |
| UI-02 | The existing inspector shows every property of the selection with explain-why; resizable panel slots (left, right, bottom) remember their size; the library and AI panels use the same slots | M | 5 | 082 |
| UI-03 | History panel listing ops, with undo to a point | S | 5 | 083 |
| UI-04 | Multi-select and bulk edit in tables | S | 5 | 084 |
| UI-05 | Design system pass: tokens, compact density, empty states, Japanese layout check, screenshots in both languages | M | 5 | 085 |
| UI-06 | Every action under 100 ms or with visible progress (A1 budgets unchanged) | M | all | every job |

### 2.9 AI (AI)

| ID | Requirement | Pri | Phase | Job |
|---|---|---|---|---|
| AI-01 | One AI gateway in the server; off by default; admin enables per feature and per project classification | M | 7 | 088 |
| AI-02 | Two adapter families: OpenAI-compatible HTTP and Anthropic Messages; keys stored on the server, encrypted, never sent to clients or logs | M | 7 | 088 |
| AI-03 | Each provider shows whether data leaves the company and the model license; restricted projects may use only providers marked "stays inside" | M | 7 | 088 |
| AI-04 | Structured output checked against a schema, citations checked, ops checked by the engine and linter, results land in a suggestion branch | M | 7 | 088, 089 |
| AI-05 | AI audit log: user, feature, provider, model, context references, response, accept or reject | M | 7 | 088 |
| AI-06 | Golden set and evaluation harness; a feature cannot be enabled until it passes its threshold | M | 7 | 090 |
| AI-07 | Wave 1: damage writer, threat suggestions, rating rationale text, report summary wording with a number check | M | 7 | 091 |
| AI-08 | Multilingual embeddings for library and catalog search, computed on the server | S | 7 | 092 |
| AI-09 | Element text translation through the gateway | S | 7 | 093 |
| AI-10 | MCP server exposing read and propose tools with the user's permissions | C | later | 097 |
| AI-11 | Optional tuned small model (Apache-2.0 or MIT weights) served through the OpenAI-compatible adapter | C | later | 098 |

---

## 3. Non-functional requirements

| ID | Requirement | Pri | Job |
|---|---|---|---|
| NFR-01 | Sovereignty: every feature works air gapped except live connectors and cloud AI providers, which are opt-in | M | all |
| NFR-02 | Engineering standards (D8) apply to every new part: caps (R3), bounded queues (R4), timeouts (R6), determinism (P4), no entity names in errors (R6) | M | all |
| NFR-03 | All open SEC findings closed before the first customer deployment | M | 058, 060 to 063 |
| NFR-04 | New dependencies pass D5 and `cargo-deny`; model weights and fonts get a license review | M | all |
| NFR-05 | CRA readiness for our own product: SBOM per release, vulnerability disclosure policy, security update and support period statement | M | 101 |
| NFR-06 | Signed installers (Windows MSI first), production signing keys, an offline bundle | M | 099, 100 |
| NFR-07 | Product security pack and user manual in English and Japanese | M | 102, 103 |
| NFR-08 | Tool qualification evidence document (determinism, property tests, explain-why) | S | 105 |
| NFR-09 | Personal data handling statement for GDPR and APPI (accounts, event log, AI audit log retention) | M | 102 |
