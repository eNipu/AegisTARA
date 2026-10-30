# Spec — 069-content-language

## Title
Content language: tagged description translations and bilingual reports (FND-04, L10N-08)

## Design Reference
- Reference: docs/requirements.md FND-04 and L10N-08; docs/ui-ux-requirements.md UI-10.6.
- Affected crates/paths: crates/aegis-model/src/{description,op,project,error,lib}.rs, crates/aegis-model/src/store/{mod,snapshot,wire,op_log}.rs, crates/aegis-client/src/{commands,view,state,session,report/doc}.rs, crates/aegis-client/tests/descriptions.rs, crates/aegis-shell/ui/{app,widgets}.slint, crates/aegis-shell/src/controller.rs.

## Problem & Goal
Descriptions are single-language text. Add a BCP 47 language tag and optional capped translations on description text, an invertible op, backward-compatible storage, an inline translation editor in the inspector, and bilingual description output in reports.

## Acceptance Criteria
1. One invertible op `SetDescriptionTranslation { target, lang, text }` sets or clears a translation (capped count, capped text, short BCP 47 tag); 2. apply-then-undo restores the project exactly, covered by the client undo test; 3. snapshot and op-log v1 fixtures still load (new fields defaulted, new op variant additive); 4. a translation editor in the inspector under the description (no new modal); 5. bilingual description output in `ReportDoc` (HTML, PDF, XLSX).
