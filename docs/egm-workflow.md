# Elephant-Goldfish Model (EGM) in AegisTARA

AegisTARA uses the [Elephant-Goldfish Model](https://github.com/google/elephant-goldfish-model) (EGM) alongside the `/factory` loop so architectural specifications and build plans are stress-tested by stateless, zero-context reviewers ("Goldfish") before implementation begins.

## Two-tier placement

| Tier | Trigger | Goldfish personas | Output & gate |
|---|---|---|---|
| **Tier 1: Project-level design & doc review** | Before unlocking any `[design]` backlog item (e.g., `206`–`213`), on major ADRs in `docs/adr/`, or during periodic doc-drift audits | Full set as needed: `egm-zoom-out`, `egm-design`, `egm-inventive-catalyst`, `egm-comprehension`, `egm-codebase-details`, `egm-implementation-readiness`, `egm-structural`, `egm-style`, `egm-synthesizer` | Review report saved alongside the job (`factory/jobs/<id>/egm-review.md`) or under `docs/`. **Human decides** architectural tradeoffs before `[design]` → `[ ]`. |
| **Tier 2: Factory Spec Audit (once per job)** | Automatically between **Plan** and **Slice 1** in `/factory` for any job that has a `high`-risk slice or ≥4 slices | **Two targeted personas only:** `egm-codebase-details` (exact symbols, signatures, existing library reuse, `allow` lists) and `egm-implementation-readiness` (unambiguous specs, testability, realistic `max_lines` caps) | `factory/jobs/<job-id>/egm-review.md`. Concrete defects (wrong method names, missing `allow` paths, impossible `max_lines`, missing edge cases) are fixed in `spec.md`, `plan.json`, and `prompts/*.md` once before building. |

### Why EGM does not run inside every per-slice loop

The per-slice loop (`builder` → `factory/verify.py` → `reviewer`) already implements fresh-context Goldfish verification for code diffs:
- `builder` and `reviewer` start with zero conversation history and read only files on disk.
- `factory/verify.py` deterministically enforces `allow`, `max_lines`, `new_deps`, `fmt`, `clippy -D warnings`, and tests.
- Running a 6–9 persona EGM swarm per slice would multiply token/quota usage by ~10× and stall autonomous slice execution on human design gates.

## Path mapping for AegisTARA

EGM defaults to `docs/designs/`, whereas AegisTARA keeps existing paths fixed (do not relocate docs):
- Design documents: `docs/design/*.md`
- Architecture & decisions: `docs/architecture.md`, `docs/decisions.md`, `docs/adr/*.md`
- Factory job specs & prompts: `factory/jobs/<job-id>/{spec.md,plan.json,prompts/*.md}`
- Job EGM audit output: `factory/jobs/<job-id>/egm-review.md`
- Optional per-crate context maps (`egm-bootstrap-orchestrator`): `crates/*/README.md`

## Compatibility with `factory/verify.py` and quota guardrails

- `*.md` files are listed in `NOT_COUNTED` in `factory/verify.py`, so documentation does not consume `max_lines`. However, `egm-bootstrap-orchestrator` README refreshes and project-level drift fixes must be committed separately from feature slices so they stay within each slice's `allow` list.
- Run Goldfish personas **sequentially (one at a time)** when quota is constrained, matching `/factory`'s "one agent at a time" rule.
- Never auto-apply adversarial architectural changes from Tier 1 reviews without owner approval if they alter a decision in `docs/decisions.md`, a file/snapshot format, licensing, or security boundaries.
