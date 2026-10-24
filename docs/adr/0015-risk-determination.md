# ADR 0015: Methodology risk matrices and threat risk determination

- Status: proposed (job 017-risk-determination)
- Date: 2026-10-25
- Crate: `crates/aegis-model` (`src/methodology/`, `src/threat.rs`,
  `src/project.rs`, `src/error.rs`, `src/store/mod.rs`, `src/lib.rs`,
  `packs/iso21434-r155.json`)
- Rules: P1, P3, P4, P5, D1, D3, D4, D6, R3, R6, A8, A9, Q3, Q4, Q5, Q6, L1
  in `docs/engineering-standards.md`
- Builds on: `0007-methodology-config.md`, `0010-pack-loader.md`,
  `0011-methodology-independence.md`, `0012-threat-attack-trees.md`

## Context

Prior to this job, `aegis-model` modeled assets, damage scenarios with per-category
impact ratings (`Level`), threat scenarios linked to damage scenarios, and
attack trees whose steps carry feasibility ratings (`FeasibilityLevel`) evaluated
by the attribute-domain evaluator (`Evaluation<FeasibilityLevel>`). However,
it stopped one step short of ISO/SAE 21434 Clause 15 risk determination:
combining each damage scenario's impact ratings with the threat's attack
feasibility through a methodology-defined 2D risk matrix to produce a
`RiskLevel` per `(damage, impact_category)` pair and an overall worst-case risk
for the threat scenario with a full explain-why trace.

Per rule P1 (Core vs. Methodology Pack Boundary), risk levels and the risk
determination matrix must live in the `Methodology` pack rather than being
hardcoded in `aegis-model`. Different methodologies or organizational profiles
may use different risk scales (for example, ISO 21434 Table H.8 uses `1..=5`,
while a medical-device methodology may use `acceptable / monitor / unacceptable`)
or even tailor the 2D lookup table per impact category.

## Options

### 1. Risk matrix placement in `MethodologyDef`

| Option | Good | Bad |
|---|---|---|
| A. Single top-level `risk_matrix` on `MethodologyDef` |Slightly smaller JSON when all categories share the same matrix | Cannot represent methodologies where different impact categories have different impact-level counts (`ImpactCategoryDef::levels` is already per-category) or category-specific risk appetite |
| **B. Per-category `risk_matrix: Vec<Vec<Arc<str>>>` on `ImpactCategoryDef` (`levels.len()` rows by `feasibility_levels.len()` columns) plus top-level ordered `risk_levels: Vec<LevelDef>` on `MethodologyDef`** | Naturally matches per-category `levels: Vec<LevelDef>`; supports both uniform matrices (like ISO 21434 Table H.8 across all four categories) and category-specific matrices; compiles to a flat `Box<[RiskLevel]>` per category for O(1) lookup | Pack files repeat the matrix for each impact category |

**Chosen: B.** Because `ImpactCategoryDef` already defines its own ordered
`levels` scale, the rows of a risk matrix are indexed by a specific category's
`Level`. Placing `risk_matrix` on `ImpactCategoryDef` makes matrix dimension
validation (`rows == category.levels.len()`, `cols == def.feasibility_levels.len()`)
exact and local.

### 2. Computed vs. stored risk values

| Option | Good | Bad |
|---|---|---|
| **A. Compute `ThreatRisk` on demand via `Project::evaluate_risk(threat_id)` without persisting derived risk in snapshots or op logs** | Single source of truth; impossible for stored risk to go stale when a damage impact rating, threat-damage link, or attack-tree step feasibility changes; zero snapshot/op-log wire bloat | Callers invoke `evaluate_risk` when inspecting risk (O(V + E + d * c), which takes sub-microsecond time for project-bounded sizes) |
| B. Cache or persist `RiskLevel` on `ThreatScenario` | Direct field read | Stale cache hazards across mutations to damages, trees, and step feasibilities; redundant state on disk |

**Chosen: A.**

## Decision

### Methodology vocabulary & pack format (`aegis-methodology-pack` v1)

- `MethodologyDef` gains `pub risk_levels: Vec<LevelDef>`, ordered from lowest
  risk (`RiskLevel(0)`) to highest risk (`RiskLevel(n - 1)`), capped at
  `MIN_LEVELS..=MAX_LEVELS` (`2..=16`, `Limit::LEVELS { max: 16 }`,
  `MethodologyError::RiskLevelCount`).
- `ImpactCategoryDef` gains `pub risk_matrix: Vec<Vec<Arc<str>>>`, where row
  `r` corresponds to `category.levels[r]` and column `c` corresponds to
  `def.feasibility_levels[c]`. Each cell is a `risk_levels` key.
- Decode-time caps in `src/methodology/pack.rs`:
  - `Limit::LEVELS` (`Limit::Levels { max: 16 }`) bounds `risk_levels` as well
    as `risk_matrix` outer rows and inner columns.
- Validation in `Methodology::new`:
  - `MethodologyError::RiskLevelCount(usize)` when `risk_levels` has `< 2` or
    `> 16` entries.
  - `MethodologyError::DuplicateRiskLevel(usize)` when two risk levels share a
    key.
  - `MethodologyError::RiskMatrixDimensions { category, rows, expected_rows, cols, expected_cols }`
    when a category's `risk_matrix` does not have `levels.len()` rows or
    any row does not have `feasibility_levels.len()` columns.
  - `MethodologyError::UnknownRiskLevel { category, level_row, feasibility_col }`
    when a matrix cell references a key not in `risk_levels`.
- Compiled `Methodology` stores a flat row-major `Box<[RiskLevel]>` per
  `ImpactCategory` and exposes:
  - `RiskLevel(u8)` (`Copy`, `Ord`, `Hash`, `index(self) -> usize`)
  - `risk_levels()`, `risk_level_count()`, `risk_level(key)`,
    `risk_level_key(rl)`, `risk_level_label(rl)`
  - `risk_for(category, impact, feasibility) -> Option<RiskLevel>` (O(1) time
    and space).
- Built-in pack `packs/iso21434-r155.json` defines `r1` (`"1"`, lowest) through
  `r5` (`"5"`, highest) and the ISO/SAE 21434:2021 Annex H Table H.8 4x4 matrix
  on all 4 impact categories:
  - `negligible`: `["r1", "r1", "r1", "r1"]`
  - `moderate`: `["r1", "r1", "r2", "r3"]`
  - `major`: `["r1", "r2", "r3", "r4"]`
  - `severe`: `["r1", "r3", "r4", "r5"]`

### Threat risk determination (`ImpactRisk`, `ThreatRisk`, `Project::evaluate_risk`)

- `ImpactRisk` (`src/threat.rs`) records one evaluated `(damage, category)` pair:
  - `damage() -> DamageId`
  - `category() -> ImpactCategory`
  - `impact() -> Level`
  - `feasibility() -> FeasibilityLevel`
  - `risk() -> RiskLevel`
- `ThreatRisk` (`src/threat.rs`) holds the complete risk determination for a
  threat scenario:
  - `feasibility() -> FeasibilityLevel`: overall attack feasibility of the threat.
  - `tree_evaluation() -> Option<&Evaluation<FeasibilityLevel>>`: the underlying
    attack-tree evaluation (when derived from an attack tree), enabling callers
    to call `why_path` or `explain(node)` to trace which leaf attack steps
    drove the feasibility.
  - `entries() -> &[ImpactRisk]`: per-damage, per-category risk entries in
    damage-link order (`ThreatScenario::damages`) then ascending
    `ImpactCategory` order.
  - `max_risk() -> Option<RiskLevel>`: maximum `RiskLevel` across `entries()`,
    or `None` when the threat has no linked damage scenarios with impact ratings.
  - `deciding_entry() -> Option<&ImpactRisk>`: first entry in `entries()` that
    attains `max_risk()`, providing a deterministic explain-why pointer back to
    the deciding `DamageId` and `ImpactCategory`.
- `Project::evaluate_risk(&self, id: ThreatId) -> Result<ThreatRisk, ModelError>`
  evaluates the threat's feasibility and combines it with every impact rating
  on every linked damage scenario via `Methodology::risk_for`.

## Complexity

V = nodes in the threat's attack tree (`<= 32`), E = edges in the attack tree,
d = linked damage scenarios (`<= 64`), c = impact categories per damage (`<= 32`).

| Operation | Time | Space |
|---|---|---|
| `Methodology::risk_for` | O(1) | O(1) |
| `Project::evaluate_risk` | O(V + E + d * c) | O(V + d * c) |
| `ThreatRisk::deciding_entry` | O(d * c) | O(1) |

## Consequences

- Risk determination is 100% methodology-driven: swapping the methodology pack
  changes risk scales and lookup matrices without touching core evaluation logic.
- Every computed risk value has an end-to-end deterministic explanation:
  `ThreatRisk::deciding_entry()` identifies the exact `(DamageId, ImpactCategory, Level)`
  that produced `max_risk()`, and `ThreatRisk::tree_evaluation()` provides the
  `Evaluation<FeasibilityLevel>` whose `why_path()` traces the decisive attack
  steps.
