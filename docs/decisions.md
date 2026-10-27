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
- A standalone app with no server (D22). No live view: read-only users see a saved version (D23). EU CRA and other domains through packs (D24).

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
- **Development tools, widened by the owner (2026-10-26):** tools used only during development (editors, test and benchmark crates, CI tools, paid tools under their own license) may have any license, including copyleft, paid or with other obligations, as long as nothing from them is linked into, copied into or shipped with the product (binaries, installers, container images, packs, reports). The allow list above is unchanged for everything that ships. `deny.toml` keeps checking the shipped dependency graph; job 110 adjusts how it treats dev-dependencies.
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

## Open

- Delete the four merged remote branches listed in D9.
- Pick the first SSO provider when a customer order arrives (D10).
- The questions in [brainstorm-product-v2.md](brainstorm-product-v2.md) section 11: the first customer's ALM tool, and who translates.
