## Answers

The owner answered the spec's open questions on 2026-10-21.

### Guiding principle (overrides the spec's question 1 and shapes everything else)

The owner's words: "Make the data structures in a way that is easy to adopt different methodologies even if it's hard to implement initially. We are not in rush to release. Maybe the v0 can accommodate iso21434 but the base shall set up for other methodologies."

So:
- The **core model is methodology-neutral**. It must not hard-code ISO/SAE 21434 vocabulary as Rust enums: no fixed impact categories, rating scales, security properties, threat-category taxonomy (STRIDE) or catalog scheme (R155 Annex 5).
- A **methodology definition** (the data a pack will later provide) declares the vocabularies: impact categories, each category's ordered rating scale, security properties, threat categories and which property each one violates, and catalog schemes with their ID syntax. The model validates every op against the methodology the project was created with.
- **ISO/SAE 21434 + UN R155 is the first and only methodology shipped in v0**, built in code as a constant or constructor using the neutral types. It is a methodology value, not special-cased types. Pack file loading (serde, files) stays out of scope, but the types must not prevent it.
- Prefer the design that keeps a second methodology (for example a TVRA- or a medical-device-style one) a data change, not a code change, even if that costs more work now. Record that test in the ADR: sketch how a second methodology would be declared.

### Answers to the individual questions

1. **Impacts:** superseded by the principle above. The 21434 methodology declares safety, financial, operational and privacy, each with an ordered 4-level scale. Use short labels only, no ISO text or tables (direction-v1 §11). "Unrated" stays separate from the lowest level.
2. **Security properties:** six (confidentiality, integrity, availability, authenticity, authorization, non-repudiation) in the 21434 methodology, with STRIDE mapped one-to-one. Both are methodology data, not core enums.
3. **Threat category:** a threat stores only its category. The targeted property is derived from the methodology's category-to-property mapping, so the two can never disagree (D12). (Default accepted.)
4. **Catalog ID:** optional and checked for syntax only, against the scheme the methodology declares. For R155 Annex 5 that is `N` or `N.M` with N in 1..=32 and M in 1..=99. No catalog contents ship.
5. **Removal:** reject while referenced, with no cascade. This includes removing a property or category from use while a threat relies on it.
6. **Names:** non-empty after trimming and at most 256 chars. (Default accepted.)
7. **Deferred:** provenance (D6), serde, the op log, pack file loading and links to attack trees are separate later jobs. (Default accepted.)

### Answers to the revised spec's open questions (2026-10-21)

1. **Methodology lifetime:** confirmed. A project's methodology is fixed at creation. Migration to another methodology or pack version is a later job.
2. **Vocabulary references:** confirmed. Use small typed indices in memory. The later serde/op-log job writes stable string keys on disk, so reordering a pack never changes the meaning of stored data.
3. **Catalog refs per threat:** confirmed. A sorted set of up to 16 refs, so one threat can cite R155, CAPEC and ATT&CK. This supersedes "optional" in answer 4 above: zero refs is still allowed.

## Rework 1

Round 1: security, UX and UI passed. Code asked for CHANGES, and its only blocker is the AC15 neutrality grep. The builder and the code reviewer agree this is a defect in the check, not in the code: any declaration of the module file `src/methodology/iso21434.rs`, which the spec requires, contains "21434" on a non-comment line outside that file.

**AC15 check, corrected.** This replaces the commands in the spec's AC15 and `## Checks`, and it is what the builder, the reviewers and the approver must run:

```
grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' crates/aegis-model/src --include='*.rs' | grep -v '^crates/aegis-model/src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//' | grep -vE ':[0-9]+:\s*(pub(\([a-z]+\))? )?mod iso21434;$'   # must print nothing
```

It excludes only the module declaration line. Any other match is still a failure.

**Also address, from round-1/review-code.md (minor, cheap):**
- `src/methodology/catalog.rs:167-173`: build the error lazily (`ok_or_else`) instead of `ok_or(err.clone())`.
- `src/methodology/mod.rs:223`: replace `#[allow(dead_code)]` on the test-only constructor with `#[cfg(test)]`, or an equivalent that does not hide dead code.
- The four `clippy::type_complexity` allows: remove them with a small named type if it stays simple. Otherwise leave them and say why in build.md.

**Optional, from round-1/review-ux.md:** make the error messages the UX reviewer listed more specific (which entity's name, the expected catalog syntax, which capacity limit, which catalog ref), as long as they still never include entity names (AC14).

Keep every other choice from round 1.
