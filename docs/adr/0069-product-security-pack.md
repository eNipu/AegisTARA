# ADR 0069: Product security pack in English and Japanese

- Status: accepted (job 102-product-security-pack)
- Date: 2026-11-12
- Scope: docs/security-pack/, scripts/check-security-pack.sh, scripts/check-security-pack-test.sh, docs/deployment.md, .github/workflows/ci.yml
- Rules: none in `docs/engineering-standards.md`; this is documentation
- Requirement: NFR-07, NFR-09, gap G39 in `docs/requirements.md`

## Context

Buyers, Japanese buyers above all, send a vendor security questionnaire and
ask for a security pack before they agree to a proof of concept. The product
is hosted by the customer, so most answers are about how it is built and
what the customer must do. The pack must be true, short, and the same in both
languages.

## Decisions

1. The pack is Markdown under `docs/security-pack/en/` and
   `docs/security-pack/ja/`. There are six chapters with the same file names
   in both folders.
2. The truth rule: a chapter states only what the code or a decision record
   proves. A thing that is not built is marked `Planned`, a duty of the
   customer is marked `Customer`, and a question about the vendor's
   organisation is marked `Vendor`. The pack claims no certification.
3. Every chapter ends with a section that lists the source files its facts
   were checked in.
4. `scripts/check-security-pack.sh` checks file names, English and Japanese
   pairs, the shape of each chapter, that every source path exists, that
   heading numbers and table row IDs match in both languages, that no
   marker word (TODO, TBD, FIXME, XXX) is left, that a Japanese file is
   translated, and that no chapter is too long. `--complete` requires all
   six chapters in both languages. CI runs it with
   `scripts/check-security-pack-test.sh`.
5. A script and not a review habit: a statement that differs between the two
   languages is a contract risk, and a reviewer does not reliably see it.
   The script cannot judge a translation, only its structure.
6. The database TLS text in `docs/deployment.md` was stale and was corrected
   while the pack was written, so the pack and the deployment guide agree.

## Left for the owner and for later

- The answers to the `Vendor` rows (6.11), per customer.
- A legal review of chapter 5.
- A read of the Japanese by a native speaker before the first hand-over.
- A PDF rendering of the pack.
- The AI audit log statement (PD-14), when AI-05 is built.
- The user manual, which is job 103.

## Alternatives

- One bilingual file per chapter: it hides a missing translation.
- A word-processor document: it cannot be checked in CI.
- Claiming ISO/IEC 27001 or TISAX style controls: none exist, and the truth
  rule forbids it.

## Consequences

- The pack must be re-read for each release (`docs/release-checklist.md`).
- A feature that changes a statement must change both languages in the same
  change, or CI fails on the parity check.
- The script checks structure, not meaning. A wrong sentence in both
  languages passes.

## Tests

- `scripts/check-security-pack-test.sh`, which runs in the CI `check` job.
- `scripts/check-security-pack.sh --complete`, which runs there too.
