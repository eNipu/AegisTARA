# ADR 0060: Excel TARA import with column mapping and dry-run preview

- Status: accepted (job 080-excel-import)
- Date: 2026-10-27
- Scope: `crates/aegis-client/src/excel.rs`,
  `crates/aegis-client/src/{commands,session,state,view}.rs`,
  `crates/aegis-shell/{ui/app.slint,src/controller.rs}`
- Rules: R3 (decode caps), D5 (licenses), D6 (`ProvenanceSource::Import`),
  P4 (determinism), Q3 (no unwrap), INT-02
- Builds on: `0014-provenance.md`, `0056-report-engine-unification-and-export-suite.md`

## Context

Consultancies and automotive OEMs frequently maintain existing TARA spreadsheets
in Microsoft Excel (`.xlsx`). Analysts need to import an `.xlsx` workbook into a
project by selecting a worksheet, mapping columns to TARA elements, inspecting a
dry-run preview that counts the elements to be created and lists any unmapped
rows with clear reasons, and applying the import as one atomic batch stamped
with `ProvenanceSource::Import` (`INT-02`).

## Decisions

### 1. Dependency-free bounded `.xlsx` reader (`excel.rs`)

Rather than adding a third-party spreadsheet crate to the shipped dependency
graph, `crates/aegis-client/src/excel.rs` reads `.xlsx` archives directly using
a bounded ZIP central-directory parser plus `flate2` (already in the dependency
tree via `krilla`) for Deflate-compressed entries and a streaming XML scanner
for `xl/workbook.xml`, `xl/sharedStrings.xml`, and `xl/worksheets/sheet*.xml`:

- **Decode caps (R3):** `MAX_XLSX_BYTES = 16 MiB` archive size,
  `MAX_XLSX_ENTRIES = 128` ZIP entries, `MAX_XLSX_ENTRY_BYTES = 16 MiB`
  decompressed entry size (enforced via `.take()`), `MAX_SHARED_STRINGS = 32_768`,
  `MAX_XLSX_ROWS = 4_096` rows per sheet, and `MAX_XLSX_COLS = 64` columns per
  row.
- **Formula-guard round-trip:** Leading apostrophes (`'`) inserted by
  `report/xlsx.rs` to guard cells starting with `=`, `+`, `-`, `@`, `\t`, `\r`,
  or `\n` are stripped on import so exported workbooks round-trip cleanly.

### 2. Bilingual header auto-detection and user-adjustable column mapping

`ExcelMapping::auto_detect` inspects the first non-empty row of a worksheet and
matches both English and Japanese column headers (`Component` / `コンポーネント`,
`Asset` / `資産`, `Properties` / `セキュリティ特性`, `Damage scenario` /
`ダメージシナリオ`, `Impact` / `影響`, `Threat scenario` / `脅威シナリオ`,
`Category` / `脅威カテゴリ`, `Attack feasibility` / `攻撃可能性`,
`Treatment` / `リスク対応`, `Security control` / `セキュリティ対策`,
`Cybersecurity goal` / `サイバーセキュリティ目標`, `Cybersecurity claim` /
`サイバーセキュリティ主張`). In the UI (`Page::Reports`), the analyst can pick
any worksheet and override any of the 12 column indices (`-1` = unmapped, `0..`
= column index) before applying.

### 3. Validated dry-run preview and single-batch apply with `Provenance::import`

`preview_import(project, workbook, mapping, actor, now_ms)`:

- Clones the current `Project` (`sim`) and builds `Op`s row by row using
  `Provenance::import(actor, now_ms)` (`ProvenanceSource::Import`).
- Deduplicates entities by case-insensitive name across rows (and against
  existing project entities) so multiple threat rows referencing the same
  component, asset, damage scenario, control, goal, or claim reuse the same
  entity ID and add links (`LinkDamage`, `LinkGoalControl`, `SetAssetComponent`).
- Automatically enables the security properties violated by a row's threat
  category on the target asset when no explicit `Properties` column is mapped,
  so importing a `Risk register` sheet succeeds without manual property setup.
- Validates each row's `Op`s against `sim` before committing the row to the
  batch; any row that lacks mapped values, references a threat without an asset,
  or fails model validation is recorded in `ExcelPreview.unmapped_rows` with its
  1-based worksheet row number and reason, without aborting the rest of the
  sheet.
- `Command::ApplyExcelImport` applies `preview.ops` through `Workspace::apply`
  as a single batch, giving one undo step and one `SubmitOps` batch.

## Consequences

- `.xlsx` workbooks (including workbooks exported by AegisTARA itself) can be
  previewed and imported with full `Import` provenance and one-step undo.
- Unmapped or invalid rows are surfaced before any project state changes.
