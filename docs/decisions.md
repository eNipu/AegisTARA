# Decision Log

Short record of product decisions. Newest at the bottom.
Details live in [direction-v1.md](direction-v1.md) and [ai-backlog.md](ai-backlog.md).

## TLDR

- Automotive first (ISO/SAE 21434, UN R155).
- Native desktop client. Not a browser app.
- The customer company hosts the server in its own cloud. Users log in with their own company credentials.
- Many users per company. One editor per project at a time; others read a saved version (D4, D23).
- Proprietary product. Open source libraries are welcome, but only with permissive licenses.
- AI features are in the backlog. The v1 architecture gets ready for them now.
- Password login for testing. Company SSO (Entra ID, Google, other OIDC) when a customer orders it.
- The test server runs on Railway.
- AI: the customer's own model through one server gateway first; our own small model later (D12).
- Japan is the first export market; English and Japanese ship first (D13).
- Library, item definition, traceability, report builder, ReqIF-first integrations and a built-in role table are decided (D14 to D19).
- Extensions through data files, a public API and built-in connectors (D20). No vendor-hosted service for now; consultancy first in Japan (D21).
- A standalone app with no server (D22). No live view: read-only users see a saved version (D23). EU CRA and other domains through packs (D24). Stay on Slint, with a GPU renderer and modeless editing (D25). Systems group components in one project (one system per component), and settings are controls with pack-defined rules (D26).
- Reference review (2026-11-07): applicability switches (conditions, assumptions, attacker profile) are ops and exclusions are derived with reasons (D27); a control has one status, Implemented, Planned or Off (D28); shared steps, shared fragments and bypass trees by reference inside a project (D29); pages grouped by TARA phase with one relation registry (D30); projects assembled from several TARA files stay in the backlog (D31).
- Workbench rethink (2026-11-10): a workspace is a folder of JSON files with git as the history (D32); three scope levels with one home per element kind (D33); one methodology per workspace and packs as pure data (D34); MITRE and R155 Annex 5 as catalogs (D35); one API with scopes (D36); a workbench UI with the toolkit decided by a spike (D37); Markdown item definitions and Typst reports (D38); templates by pinned reference (D39); GUI first, files are storage only (D40); a sequential factory with small slices and an owner gate (D41).

---

## D1. Automotive is the first domain

- **Date:** 2026-10-10
- **Decision:** v1 targets automotive: ISO/SAE 21434 TARA flow and UN R155 Annex 5.
- **Why:** The most structured method. Strong regulatory demand. Clear work products.
- **Consequence:** The core stays methodology-neutral. Automotive is the first methodology pack, not hard-coded logic.

## D2. Native desktop client

- **Date:** 2026-10-10
- **Decision:** Users work in a native desktop application written in Rust. No web browser UI.
- **Why:** A rich canvas, fast interaction, a feel like other engineering tools, and control over local data.
- **Consequence:** Choose a native Rust UI toolkit (proposal: iced, with GPUI as challenger, decided by a short spike). Need installers, code signing, and auto-update. Windows comes first, because most automotive engineers use it.
- **Note:** Login opens the system browser once for company single sign-on. This is the standard, secure way for native apps (RFC 8252). The app itself stays native.

## D3. Customer-hosted server, client-server model

- **Date:** 2026-10-10
- **Decision:** Each customer company runs the AegisTARA server in its own cloud account or data center. Desktop clients connect to it. Users log in with company credentials.
- **Why:** TARA data is highly sensitive. The company keeps full control. No data passes through us.
- **Consequence:** One deployment per company (single tenant). Must be easy to install and run: one server binary in a container, PostgreSQL, and the company's identity provider. The license is a signed file checked offline, so no phone-home is needed.

## D4. Many users, one editor per project at a time (v1)

- **Date:** 2026-10-10
- **Interpretation of "single user":** many people in a company use the same server. In v1 only one person edits a given project at a time. Others can open it read-only and see changes live.
- **Why:** Much simpler than real-time co-editing. Fits engineering review culture (check out, edit, check in).
- **Consequence:** Store every change as an operation from day one. This keeps the path open to real-time co-editing later without a rewrite.
- **Confirmed by the owner (2026-10-26), with one change:** others get **read-only access to a saved version**, not a live view. Nobody needs to watch edits as they happen, and the live view costs a lot of machinery. See D23.

## D5. Proprietary product, permissive open source dependencies

- **Date:** 2026-10-10
- **Decision:** AegisTARA is closed source for now. It may use open source libraries with permissive licenses.
- **Allowed:** MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, BSL-1.0, CC0-1.0, MIT-0, OFL-1.1 (fonts).
- **Review first:** MPL-2.0, Slint royalty-free license (chosen in D7, needs your approval), any AI model license, any data catalog license.
- **Not allowed:** GPL, LGPL, AGPL, SSPL, Business Source License, Commons Clause, non-commercial licenses.
- **Non-shipped tooling (added 2026-10-21):** crates used only to build, test, fuzz or benchmark (dev-dependencies and fuzz crates, never linked into a shipped binary) may also use NCSA (the LLVM license, permissive). Each such crate needs a per-crate exception in `deny.toml` that names the crate and version and points to D5. Everything that ships keeps the allow list above.
- **Development tools, widened by the owner (2026-10-26):** tools used only during development (editors, test and benchmark crates, CI tools, paid tools under their own license) may have any license, including copyleft, paid or with other obligations, as long as nothing from them is linked into, copied into or shipped with the product (binaries, installers, container images, packs, reports). The allow list above is unchanged for everything that ships. `deny.toml` keeps checking the shipped dependency graph. Job 110 (done 2026-10-27) found that cargo-deny already ignores dev-dependencies in the license check, pinned that with two fixtures (a copyleft dev-dependency passes; a copyleft crate that is also a normal dependency fails), and made `scripts/deny.sh` skip the license check for the non-shipped fuzz crate, which let the NCSA exception for `libfuzzer-sys` go. Advisories, bans and sources still cover everything.
- **Enforcement:** `cargo-deny` in CI. `cargo-about` generates the third-party notices shown in the About dialog.
- **Correction to the brainstorm:** Garage and MinIO are AGPL. Do not use them. Use the cloud provider's object storage or PostgreSQL.

## D6. AI features go to the backlog, but v1 is "AI-ready"

- **Date:** 2026-10-10
- **Decision:** No AI in v1. Build the hooks AI will need: provenance on every element, suggestion branches, a typed operation API, catalog IDs everywhere, and an engine that can be called as a tool.
- **Principle:** AI proposes. The engine computes. A human decides.
- **Details:** [ai-backlog.md](ai-backlog.md)

## D7. Desktop UI toolkit: Slint

- **Date:** 2026-10-11
- **Decision:** Build the desktop client with Slint (1.x, Rust API, `.slint` markup).
- **Why:** In a five-way spike, all toolkits could do right-click editing of feasibility values. Slint did it with built-in menus and widgets, has a stable 1.x API, and renders fully on a CPU-only renderer for users on remote desktops. Details: [spikes/RESULTS.md](../spikes/RESULTS.md).
- **Condition:** use Slint under its Royalty-free License 2.0. This requires an "About Slint" notice in the app and a Slint badge on the download page. If that is not acceptable, buy the commercial license or switch to iced (MIT), the tested fallback.
- **Before locking:** run the Slint spike on a real Windows laptop and over RDP or Citrix.
- **Accepted by the owner (2026-10-26):** use the free royalty-free license with the attribution; buy a commercial Slint license when a customer asks for it or the business grows.
- **Replaces:** the earlier lead pick of iced in direction-v1.md section 4.

## D8. Engineering standards are requirements

- **Date:** 2026-10-15
- **Decision:** All production code follows [engineering-standards.md](engineering-standards.md): performance budgets, data structure and algorithm rules, resource limits, concurrency rules, testing and measurement, and learning practices.
- **Why:** AegisTARA is both a product and a way to learn software engineering. Large attack graphs need efficient data layout and incremental algorithms from the start, and every choice should be measured and explained.
- **Consequence:** Spike code is not promoted to production. Budgets are checked by benchmarks in CI. Every non-trivial choice gets a decision record with numbers.

## D9. Working assumptions while D4 and D7 await the owner

- **Date:** 2026-10-25
- **Context:** the roadmap (docs/status-and-roadmap.md, G4 and G5) lists D4 and D7 as open. Phases 1 and 2 cannot start without an answer, so the build proceeds on the safest reading of each, and each one is cheap to reverse.
- **D4 (edit lock):** the interpretation is built as written (confirmed 2026-10-26; the live view is removed by D23). Many users share one server, one editor holds a project's lease at a time, others observe read-only. The lease is 2 minutes and is renewed by the client. The lock lives behind one small trait in `aegis-server`, so per-module locks or co-editing replace it without touching the op path.
- **D7 (Slint):** the shell is built on Slint under the Royalty-free License 2.0. The condition is met by default: the app has an "About Slint" notice, and `docs/release-checklist.md` lists the download-page badge. The Slint crates are the only entry in `deny.toml` `[licenses] exceptions` that is not tooling, it names `slint` and cites this decision, and it applies to `crates/aegis-shell` only. All UI-independent logic lives in `aegis-client`, so switching to iced or buying the commercial license rewrites only `aegis-shell`.
- **Owner answers:** D4 confirmed and D7 accepted on 2026-10-26.
- **Housekeeping (G3):** the branches `claude/pipeline-tasks-3ca78e`, `factory/016-store-trees-and-provenance`, `factory/jobs-013-015` and `factory/jobs-017-019` are fully merged into `master` (0 commits ahead, checked 2026-10-25). The mockup content is in `docs/design/`. No local worktree exists. Deleting remote branches is left to the owner.

## D10. Password login for testing; company SSO when a customer orders it

- **Date:** 2026-10-25
- **Decision:** the server has a simple user name and password login (`AEGIS_AUTH=password`). It is for the test server, demos and small trials. A customer project can turn it off: with `AEGIS_AUTH=oidc` the `LogIn` call answers that password login is off and the app shows only the company login.
- **Backlog:** single sign-on with Microsoft Entra ID, Google, or the customer's own OIDC provider is added when a customer order names the provider (`factory/backlog.md`). The OIDC and DPoP code from job 031 is already in place; each provider needs its own setup, test and guide.
- **Limits of password login:** Argon2id hashes, opaque session tokens stored as SHA-256, a per-name throttle, an event log. No password reset by e-mail, no MFA. These are reasons not to use it for customer production data.

## D11. Test server on Railway

- **Date:** 2026-10-25
- **Decision:** the shared test server runs on the owner's Railway account (`docs/railway.md`), built from the branch with `deploy/Dockerfile` and the `dev-license` feature, with Railway PostgreSQL. Railway's edge forwards HTTP/1.1, so the app reaches it with gRPC-Web.
- **Not for customers:** the `dev-license` build skips the license check. Release builds never enable that feature (`docs/release-checklist.md`).

## D12. AI: customer's own model first, our own small model later

- **Date:** 2026-10-26
- **Decision:** one AI gateway in the server. It speaks to the model the customer chooses through two adapter families: OpenAI-compatible HTTP (OpenAI, Azure OpenAI, vLLM, llama.cpp, Ollama) and the Anthropic Messages API (Anthropic, Claude on Bedrock and Vertex). Keys are set by an admin, stored encrypted on the server, never on laptops. A tuned small open-weight model of our own is an optional add-on later, served through the same OpenAI-compatible adapter.
- **Why:** customer TARAs never reach us (D3), so we have little training data; retrieval, strict output schemas and the engine as a tool give most of the value with any model. Building the gateway first makes the model replaceable.
- **Unchanged:** D6. AI proposes, the engine computes, a human decides. Off by default.
- **Details:** [brainstorm-product-v2.md](brainstorm-product-v2.md) section 1, [ai-backlog.md](ai-backlog.md).

## D13. Japan is the first export market; UI, methodology and content languages are separate

- **Date:** 2026-10-26
- **Decision:** English and Japanese ship first. UI text uses Slint `@tr()` with catalogs bundled in the binary, and a Fluent catalog in `aegis-client` for text made in Rust (pages, errors, reports). Methodology packs carry labels per language. Project text carries a language tag and optional translations. PDFs embed a subset of Noto Sans JP (OFL-1.1). Translation of user content goes through the AI gateway or a customer's licensed service, never a hidden network call.
- **Why:** Japanese OEMs and suppliers apply UN R155 through MLIT type approval and prefer on-premise tools, which matches D3. A Japanese PDF is broken today (Inter has no Japanese glyphs).
- **Details:** [brainstorm-product-v2.md](brainstorm-product-v2.md) section 2.

## D14. Attack tree library answers

- **Date:** 2026-10-26
- **Decision:** the eight questions in [brainstorm-attack-tree-library.md](brainstorm-attack-tree-library.md) section 7 are answered in [brainstorm-product-v2.md](brainstorm-product-v2.md) section 3: copy with an origin note; node-level tree ops, then raise the per-tree cap (target 4,096, measured); ratings travel by user choice and are flagged for review; never merge identical names silently; the Methodology Owner role publishes; a built-in demo pack from public sources; a Library panel next to the canvas; phase 0 is the first demo. Search is n-gram fuzzy plus facets without AI; multilingual embeddings come later. Saving to the library is offered, not automatic.
- **Moves:** backlog item 057 out of "waiting for a design session".

## D15. Item definition and architecture figures

- **Date:** 2026-10-26
- **Decision:** three steps. (1) Figures: image attachments with captions, stored on the server, in reports. (2) Item definition elements in `aegis-model` (component, interface, data flow, trust boundary; assets bound to components), with neutral names packs can rename. (3) A data-flow diagram drawn by `aegis-canvas`.
- **Why:** direction section 11 put the item definition in v1 scope and it was not built; customers already have architecture pictures, so figures come first.

## D16. Traceability model

- **Date:** 2026-10-26
- **Decision:** typed links the engine uses stay typed. New: requirement and verification elements, a generic trace link for relations the engine does not use, external references (Jira, Polarion, JAMA, ReqIF, URL) on every element, and suspect marking when a link's source changes. A traceability matrix page and report section show coverage.

## D17. Report builder

- **Date:** 2026-10-26
- **Decision:** HTML and PDF move to one document model. Users pick sections, filters, language and branding, and save the choice as a profile. XLSX joins PDF and HTML; DOCX only on a customer order.

## D18. Integrations: ReqIF first, server-side connectors second

- **Date:** 2026-10-26
- **Decision:** ReqIF export and import first (air gapped, works with Polarion, JAMA, DOORS, codebeamer). Then live connectors in the server, never in the desktop app: Jira first, Polarion and JAMA when a customer order names them. Push and link with status pull, not full two-way sync.
- **Why:** connector tokens are company secrets and the server already sits inside the company network. Jira comes first also because its free Cloud plan has the full REST API, so it is built and tested without a paid license; Polarion and JAMA have no free tier.

## D19. Project access: a built-in role table, not Cedar

- **Date:** 2026-10-26
- **Decision:** the six roles from direction section 6 (Admin, Methodology Owner, Project Owner, Analyst, Reviewer, Viewer) and per-project membership, enforced on the server for every RPC. Roles map from SSO groups. Cedar policies only if a customer needs rules the table cannot express.
- **Why:** fixes SEC-02 with the least new machinery; the library, AI and connectors need these permissions.
- **Moves:** backlog item 058 out of "waiting for a design session".

## D20. Extensions: data files, a public API, built-in connectors; no loaded code

- **Date:** 2026-10-26
- **Decision:** AegisTARA is extended in three layers: (1) data plugins, files in published formats (methodology packs, library packs, ReqIF, XLSX); (2) external plugins, separate programs in any language that call the public gRPC API with a service account token and follow changes through `Watch`; (3) built-in connectors in the server behind one Rust trait (Jira first). No native code is loaded into the server or the app. A WebAssembly plugin host is the upgrade path if a customer needs custom logic inside the server.
- **Why:** modular and open without a plugin ABI to maintain, and every external plugin passes the same access checks as a user.
- **Details:** [brainstorm-product-v2.md](brainstorm-product-v2.md) section 8.2; job 106.

## D21. No vendor-hosted service for now; consultancy first in Japan

- **Date:** 2026-10-26
- **Decision:** D3 stays: customers host their own server, and their data never passes through us. The owner's consultancy works in the standalone app (D22) under NDA and hands the project file over with the report. A vendor-hosted service is revisited when trial demand outgrows that.
- **Details:** [brainstorm-product-v2.md](brainstorm-product-v2.md) sections 9.4 and 9.5.

## D22. A standalone app with no server, for consultancy and small projects

- **Date:** 2026-10-26
- **Decision:** the desktop app can run alone. It starts the server library inside its own process, bound to the loopback address only, with a project-folder store (the snapshot and op log formats `aegis-model` already has) instead of PostgreSQL, and no login. The client code path stays the same, so every feature works in both modes. A standalone project folder can be imported into a customer server later. The license file is checked by the app in this mode.
- **Why:** the consultancy (D21) works alone, and small customers should not have to run a server for one project. Reusing the server library keeps one code path instead of two.

## D23. Remove the live observer view; viewers read a saved version

- **Date:** 2026-10-26
- **Decision:** read-only users open the project as last saved by the server (or a baseline). They see a "newer version available, reload" notice instead of a live stream. The editor keeps its lease and "numbers checked" through ordinary calls. Streaming ops and risk events to observers (ADR 0033), and the `Watch` heartbeat and silence watchdog where nothing else needs them, are removed. Job 108 (done 2026-10-27, ADR 0033 update) cut the whole `Watch` stream and added `GetProjectStatus` for the "newer version available" notice.
- **Rule from the owner:** features that add technical complexity without business value are reduced or removed. Added to [engineering-standards.md](engineering-standards.md) section 1.

## D24. Beyond automotive: EU CRA and other domains through packs

- **Date:** 2026-10-26
- **Decision:** the core stays methodology-neutral (D1). After the ISO/SAE 21434 pack comes a product-security pack for EU CRA (any product with digital elements), then STRIDE with MITRE catalogs (CAPEC, ATT&CK, EMB3D) for services, cloud and small electronics. Medical (IEC 81001-5-1, FDA) and industrial (IEC 62443) packs follow customer demand. Packs may rename the element types ("damage scenario" becomes "harm"), and catalog packs may carry catalog text where the catalog's license allows it (D5 review).
- **Why:** many companies must do TARA-like risk assessment under the CRA, including Japanese makers that export to the EU.
- **Details:** [brainstorm-product-v2.md](brainstorm-product-v2.md) section 2.5, [brainstorm.md](brainstorm.md) section 2.

## D25. Stay on Slint; fix how it renders and remove modal editing

- **Date:** 2026-10-30
- **Decision:** no native rewrite (SwiftUI on macOS, WinUI or WPF on Windows). The app keeps Slint (D7) and gets a GPU renderer with a software fallback, the Cupertino style on macOS, the system appearance and ⌘ shortcuts (job 113). Undo and redo, which the client core has but the app hides, become visible top-bar actions that name what they undo (job 116, first part). Editing becomes modeless: elements are created as a draft in the inspector, removals offer undo instead of a confirmation, and editors move into panels (jobs 114 to 117). A dialog is kept only for actions undo cannot reverse, security tasks, file pickers and About.
- **Why:** the "not solid on macOS" feel comes from the forced software renderer and the Windows-style widgets, not from Slint. A native rewrite would build every screen and the canvas twice and add two bindings to the Rust core, for no new capability. Reconsider only if a paying customer requires a native Mac app after jobs 113 to 117.
- **Details:** [ui-ux-requirements.md](ui-ux-requirements.md) (UI-07 to UI-12).

## D26. Systems group components in one project; settings are controls with rules

- **Date:** 2026-11-02
- **Status:** accepted by the owner on 2026-11-02, with the change that a component is in at most one system ([brainstorm-systems-config.md](brainstorm-systems-config.md) sections 5.2 and 10).
- **Decision:** (1) Several systems (Brake, ADAS, Backend) live inside one project, not in a program of projects. A system is a named group of components with its own boundary: each component is in at most one system, and a component several systems need (the gateway) becomes a system of its own, linked to the others by boundary data flows. Interfaces, assets, threats and trees belong to a system through the links they already have; damage scenarios, controls, goals and claims may be used by several systems. Viewing one system, a subset or all is a per-user filter, never an op, and a risk number does not depend on the view. One op log, lease, undo history, audit chain and baseline per project, as today. (2) ECU and interface settings are security controls applied to components and interfaces, with rules "attack type → feasibility ceiling". An attack step names the component or interface it goes through and its attack type; the engine derives the ceilings and the explain-why names them ("capped by: Secure boot on ADAS ECU"). Attack types and setting presets come from the methodology pack; a custom setting is a control with rules the user types. (3) Settings ship before systems.
- **Why:** one system per component gives each TARA a clear boundary, as an item definition has, and totals that add up; one project keeps undo, baselines, reports and Excel import whole with no server change. Cross-system attacks need no tree links: a step goes via a component of another system, whose settings cap it. Settings replace the consultant's hand re-rating, which helps every project, and pack data keeps the core methodology-neutral (D1, D24).
- **Details:** [brainstorm-systems-config.md](brainstorm-systems-config.md), [design/systems-config.md](design/systems-config.md); jobs 119 to 126.

## D27. Applicability: conditions, assumptions and an attacker profile switch elements off; the engine derives what is excluded and why

- **Date:** 2026-11-07
- **Status:** decided by the architect review ([design/reference-ui-review.md](design/reference-ui-review.md) sections 3.2 to 3.4); the owner can change it.
- **Decision:** three new project elements decide which parts of the analysis apply. (1) **Conditions** (default pack name "Configuration"; a pack may say "TOE configuration" or "variant"), each present or not; assets, components, interfaces, threats, trees, steps, goals, claims and assumptions may need conditions, and an element is excluded while any condition it needs is off. (2) **Assumptions** that hold or not; while one holds, the steps and trees it names are excluded, and an assumption should lead to a claim (linter warning when it has none). (3) An **attacker profile** on the project: allowed values per attack potential factor, a threat scalability choice and later attacker types; a step whose rated value is not allowed is excluded, an unrated factor never excludes. Every switch is an op with an exact inverse, so history, baselines, the diff and the report cover show the variant. Exclusion is **derived, never stored** (D10): `Applicability` gives each element included or excluded with its list of reasons. Evaluation lifts the feasibility domain with one extra value, `Excluded`, never a made-up level: OR ignores excluded children, AND is excluded when any child is (that path cannot happen), and a threat whose root is excluded shows its risk as "not applicable" with the reason "every attack path excluded". Excluded components and interfaces take the steps that go via them (CFG-01) with them.
- **Why:** consultants analyse variants (with and without the Ethernet interface, with the customer's assumptions or without) and today copy the project to do it. Reviewers need to see what was taken out and why, which a silent exclusion hides.
- **Unchanged:** D26 systems are a view and never change a number; conditions are ops and do. D26 settings cap feasibility; conditions say whether a part exists. No number depends on the user's view.
- **Details:** architecture III.6; requirements APL-01 to APL-08; jobs 133 to 139, 157.

## D28. One control status: Implemented, Planned or Off

- **Date:** 2026-11-07
- **Status:** decided by the architect review (section 3.5); the owner can change it.
- **Decision:** `ControlStatus` replaces the idea of two switches ("in the initial analysis", "in the residual analysis"). Implemented controls count in the initial and the residual numbers, Planned ones in the residual only, Off ones in neither. `Op::SetControlStatus` with an exact inverse. Old projects load every control as Planned, which is today's maths (initial = no controls, residual = all), so no saved number changes. The UI says "initial" and "residual" ("with implemented controls" / "after planned treatment").
- **Why:** ISO/SAE 21434 rates the item as designed, which includes the controls already in it; two independent switches would allow a meaningless state.
- **Details:** requirements CFG-10, CFG-11; job 140.

## D29. Shared building blocks by reference inside a project

- **Date:** 2026-11-07
- **Status:** decided by the architect review (sections 3.13 to 3.15); the owner can change it.
- **Decision:** three ways to reuse inside one project, all by reference: (1) a **shared step** rated once and used by leaf nodes of many trees; (2) a **shared fragment**, an attack tree with the fragment role whose root value feeds a node of another tree; fragments may use shared steps and other fragments, and a cycle is rejected with its path (Kahn's algorithm, ADR 0002, over the fragment graph); (3) an optional **bypass tree** (a fragment) on a security control, whose feasibility is the control's ceiling instead of the typed level. Fragments are evaluated before the trees that use them, once each, and explain-why steps into them. Making a block shared and unsharing it are one undo step each; unsharing copies the values back so inverses stay exact.
- **Unchanged:** D14, the library inserts copies with an origin note. D26, no tree links across systems for attack paths ("via" does that); a block used by trees of two systems is shared like a damage scenario. ADR 0064, ceilings are derived and never stored.
- **Why:** a consultant rates "read out the flash memory" once, not in twelve trees, and a reviewer checks it once. Today the same step is rated many times and drifts.
- **Details:** architecture III.7; requirements BLK-01 to BLK-05; jobs 141 to 144.

## D30. Pages grouped by TARA phase; one relation registry for every link

- **Date:** 2026-11-07
- **Status:** decided by the architect review (sections 2.1 and 2.5).
- **Decision:** the sidebar, menu and command palette group pages by the five phases of ISO/SAE 21434 clause 15 (item definition first), then Outputs and Project, from one page table. Every link between element types is described once in a relation registry in `aegis-client` (from, to, label, cardinality, read function, link and unlink edits); the inspector, link chips, the multi-link picker, the traceability graph and matrix, find and report cross-references read it. Every table uses the same column kinds (ID, name, links, computed, rating, text).
- **Why:** six new element types would otherwise add link code to five places each; one table keeps them modular and still linked.
- **Details:** architecture III.9; requirements UI-13 to UI-19, NFR-12; jobs 127 to 131, 149, 150.

## D31. Projects assembled from several TARA files stay in the backlog

- **Date:** 2026-11-07
- **Status:** backlog by the owner's request; needs a design session.
- **Decision:** a project assembled from several TARA files (a supplier's ECU TARA, a platform team's gateway TARA), each of which stays a normal project its owner opens, edits, baselines and reports on alone, is not built now. The first slice to design is importing **external results** from a pinned baseline of another project; the second is a composite project of pinned member projects. D26 (systems inside one project) stays the answer for one team.
- **Details:** [design/reference-ui-review.md](design/reference-ui-review.md) section 4; architecture III.10; requirements MTA-01 to MTA-04; jobs 158 and 159 under "Waiting for a design session".

## D32. A workspace is a folder of JSON files; git is the history in standalone mode

- **Date:** 2026-11-10
- **Status:** agreed by the owner.
- **Decision:** a project is saved as a folder: small JSON files with a `format`, `version` and `kind` envelope, one per system and element kind, one per attack tree, read by the existing strict, size-capped reader (ADR 0009, 0011). The item definition is Markdown. One bundle file (`.aegis`, the folder zipped with a manifest of hashes) is used for handover. In standalone mode the file history is git: a commit is a change, a tag is a baseline. Ops stay in memory for undo and validation. The single snapshot stays readable, and stays the format of server mode.
- **Removed in standalone mode:** the saved op log, the audit hash chain, the baseline store, the History and Versions pages (ADR 0037, 0050).
- **Not chosen:** TOML (a second reader for no gain), a binary format, SQLite.
- **Details:** [design/workbench-architecture.md](design/workbench-architecture.md) section 7.

## D33. Three scope levels; every element kind has one home

- **Date:** 2026-11-10
- **Status:** agreed by the owner.
- **Decision:** workspace (the product), system (one item with its own boundary), component. The ownership table in [design/workbench-architecture.md](design/workbench-architecture.md) section 3.2 says where each kind lives: damage scenarios, control definitions, goals, claims, requirements and shared steps at workspace level; item definition, conditions, assumptions, assets, misuse cases, threats and trees at system level. A control's status moves to its application in a system and resolves through layers (template, workspace, system, configuration). A workspace holds named configurations (sets of switches).
- **Changes:** D26 (a system becomes a folder with its own settings; still no effect on numbers by view), D28 (status per application). **Replaces** D31: a workspace of system folders plus external results covers composite projects.

## D34. One methodology per workspace; packs are data with four kinds of calculation

- **Date:** 2026-11-10
- **Status:** agreed by the owner.
- **Decision:** a workspace pins one methodology and version. A pack is data only: names, scales, factor ratings (factors, a combiner, bands), direct ratings, the tree rule, matrices, extra attributes on element kinds, a lint profile and report sections. CVSS is a built-in function a pack may select. No formulas or code in packs. Another methodology's TARA arrives as external results with a level mapping the user confirms.
- **Out of scope:** quantitative models in money or probability.
- **Details:** architecture document section 6.

## D35. MITRE and UN R155 Annex 5 are catalogs, not methodologies

- **Date:** 2026-11-10
- **Status:** agreed by the owner.
- **Decision:** ATT&CK, CAPEC, CWE, EMB3D, D3FEND, R155 Annex 5, IEC 62443-3-3 and CRA Annex I are catalog packs, separate from methodology packs. Entries attach to attack steps, components, controls and requirements by ID and version. Cross-maps drive proposals, never automatic changes, and never a number.

## D36. One API with scopes; scripts and AI propose

- **Date:** 2026-11-10
- **Status:** agreed by the owner. Refines D20.
- **Decision:** everything outside the app changes a project through the existing gRPC API with a scoped token: `read`, `propose` (ops land on a suggestion branch a person accepts) or `write` (ops applied, limited to named element kinds and systems). Scripts and AI default to `propose`. A task in the workspace declares its command and scope and gets a short-lived token. No interpreter is embedded and scripts do not edit project files.

## D37. A workbench instead of pages; the shell toolkit is decided by a spike

- **Date:** 2026-11-10
- **Status:** agreed by the owner. Changes D7, D25 and the navigation part of D30.
- **Decision:** the app becomes a workbench: a project tree, tabs and splits, docks, a command palette, settings with layers. Pages become tabs; the relation registry stays. GPUI with gpui-component is the first candidate for the shell, decided by a spike on macOS, a Windows laptop and Windows over RDP. Zed's source may be read for ideas; code from its GPL crates is not copied (D5). The Slint shell ships until the new shell covers the consultancy's daily work, then it is removed.
- **Accepted for now:** a weaker screen-reader path and no CPU-only rendering; revisit at the first customer deployment.

## D38. Item definition in Markdown; reports through Typst

- **Date:** 2026-11-10
- **Status:** agreed by the owner. Extends D15 and D17.
- **Decision:** the item definition is a Markdown file whose fenced structure block declares components, functions, interfaces, flows and trust boundaries; the block is the model and the diagram is drawn from it by `aegis-canvas`. PDF output uses Typst templates with diagrams as SVG. No Mermaid.

## D39. Whole-system templates by pinned reference

- **Date:** 2026-11-10
- **Status:** agreed by the owner. Changes D14 for whole systems.
- **Decision:** a system may extend a template (ECU, gateway, network device, sensor, backend) at a pinned version and stores only what differs. An update is a reviewed diff; detach copies everything in. Single fragments from the library are still copied with an origin note (D14).

## D40. GUI first: files are the storage, never the interface

- **Date:** 2026-11-10
- **Status:** steer from the owner; details proposed in [design/workbench-gui-first.md](design/workbench-gui-first.md).
- **Decision:** a customer's analyst never sees JSON, Markdown, file paths or git. Lists are edited in grids, single elements in forms, trees and the item structure on a canvas, item text in sections with light formatting, history as a plain-word Versions timeline backed by a git library inside the app. The source view is a developer option, off by default. Attack trees grow from a context menu, the keyboard and drag from the library: add a step, reference an existing shared step or fragment (D29), or copy from the library (D14).
- **Not built:** a word processor, a free drawing tool, a query or formula language in the UI.
- **Changes:** D38 (the Markdown file is storage; the user edits sections and a diagram), the "text twin" in the architecture document.

## D41. The factory runs one agent at a time and stops for the owner after each small slice

- **Date:** 2026-11-10
- **Status:** asked for by the owner.
- **Decision:** the factory is planner, builder, a script gate (`factory/verify.py`), one reviewer for code and security, then the owner. No agents run in parallel and there are no UI or UX review agents. A job is cut into slices of at most 400 changed lines; each has a self-contained build prompt that also works in another session or with another model. The model is chosen per slice by tier and risk. Lessons from failed gates, review findings and owner reworks go to `factory/lessons.md`; a change to the factory itself is proposed by `/factory retro` and applied only with the owner's yes.
- **Details:** `.claude/skills/factory/SKILL.md`.

## D42. One methodology per workspace, chosen when the project is created

- **Date:** 2026-11-10
- **Status:** steer from the owner.
- **Decision:** a workspace uses exactly one methodology. It is chosen when the project is created and cannot be changed afterwards in v1. Mixing methodologies in one workspace is not allowed. Opening a workspace with a different methodology than its own is refused (job 162 already does this). Changing or converting the methodology later is not built until its cost and its interoperability are understood.
- **Open:** whether a newer version of the same methodology may open an older workspace (today any version is accepted, ADR 0009).

## D43. Every change goes through the business logic; lists are edited like a spreadsheet

- **Date:** 2026-11-10
- **Status:** steer from the owner. Sharpens D40.
- **Decision:** adding, changing or deleting anything (an asset, a misuse case, a damage scenario, or a property of one) is done by the business logic in the model, which checks it and keeps the links between elements correct. The editor never writes to the files directly and never shows JSON. Lists are edited in a grid that behaves like a spreadsheet: add a row, delete a row, type in a cell, with no extra steps. The workspace files (job 162) are only what the business logic saves.

## Open

- Delete the four merged remote branches listed in D9.
- Pick the first SSO provider when a customer order arrives (D10).
- The questions for composite projects in [design/reference-ui-review.md](design/reference-ui-review.md) section 4 (D31).
- The questions in [brainstorm-product-v2.md](brainstorm-product-v2.md) section 11: the first customer's ALM tool, and who translates.
