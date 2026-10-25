# Decision Log

Short record of product decisions. Newest at the bottom.
Details live in [direction-v1.md](direction-v1.md) and [ai-backlog.md](ai-backlog.md).

## TLDR

- Automotive first (ISO/SAE 21434, UN R155).
- Native desktop client. Not a browser app.
- The customer company hosts the server in its own cloud. Users log in with their own company credentials.
- Many users per company. One editor per project at a time in v1.
- Proprietary product. Open source libraries are welcome, but only with permissive licenses.
- AI features are in the backlog. The v1 architecture gets ready for them now.

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
- **Please confirm** this interpretation.

## D5. Proprietary product, permissive open source dependencies

- **Date:** 2026-10-10
- **Decision:** AegisTARA is closed source for now. It may use open source libraries with permissive licenses.
- **Allowed:** MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, BSL-1.0, CC0-1.0, MIT-0, OFL-1.1 (fonts).
- **Review first:** MPL-2.0, Slint royalty-free license (chosen in D7, needs your approval), any AI model license, any data catalog license.
- **Not allowed:** GPL, LGPL, AGPL, SSPL, Business Source License, Commons Clause, non-commercial licenses.
- **Non-shipped tooling (added 2026-10-21):** crates used only to build, test, fuzz or benchmark (dev-dependencies and fuzz crates, never linked into a shipped binary) may also use NCSA (the LLVM license, permissive). Each such crate needs a per-crate exception in `deny.toml` that names the crate and version and points to D5. Everything that ships keeps the allow list above.
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
- **Replaces:** the earlier lead pick of iced in direction-v1.md section 4.

## D8. Engineering standards are requirements

- **Date:** 2026-10-15
- **Decision:** All production code follows [engineering-standards.md](engineering-standards.md): performance budgets, data structure and algorithm rules, resource limits, concurrency rules, testing and measurement, and learning practices.
- **Why:** AegisTARA is both a product and a way to learn software engineering. Large attack graphs need efficient data layout and incremental algorithms from the start, and every choice should be measured and explained.
- **Consequence:** Spike code is not promoted to production. Budgets are checked by benchmarks in CI. Every non-trivial choice gets a decision record with numbers.

## D9. Working assumptions while D4 and D7 await the owner

- **Date:** 2026-10-25
- **Context:** the roadmap (docs/status-and-roadmap.md, G4 and G5) lists D4 and D7 as open. Phases 1 and 2 cannot start without an answer, so the build proceeds on the safest reading of each, and each one is cheap to reverse.
- **D4 (edit lock):** the interpretation is built as written. Many users share one server, one editor holds a project's lease at a time, others observe read-only. The lease is 2 minutes and is renewed by the client. The lock lives behind one small trait in `aegis-server`, so per-module locks or co-editing replace it without touching the op path.
- **D7 (Slint):** the shell is built on Slint under the Royalty-free License 2.0. The condition is met by default: the app has an "About Slint" notice, and `docs/release-checklist.md` lists the download-page badge. The Slint crates are the only entry in `deny.toml` `[licenses] exceptions` that is not tooling, it names `slint` and cites this decision, and it applies to `crates/aegis-shell` only. All UI-independent logic lives in `aegis-client`, so switching to iced or buying the commercial license rewrites only `aegis-shell`.
- **Still needed from the owner:** a yes or no on both. If D7 is rejected, delete `crates/aegis-shell` and its exception.
- **Housekeeping (G3):** the branches `claude/pipeline-tasks-3ca78e`, `factory/016-store-trees-and-provenance`, `factory/jobs-013-015` and `factory/jobs-017-019` are fully merged into `master` (0 commits ahead, checked 2026-10-25). The mockup content is in `docs/design/`. No local worktree exists. Deleting remote branches is left to the owner.

## Open

- Confirm D4 interpretation (working assumption in D9).
- Accept or reject the Slint attribution condition (D7, working assumption in D9).
- Delete the four merged remote branches listed in D9.
