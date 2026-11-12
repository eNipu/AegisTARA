# AegisTARA product security pack

This pack answers the questions in a vendor security questionnaire. It is for
the security reviewer at a buyer, and for the person at the vendor who hands
the pack over before a proof of concept. It is short on purpose: six chapters,
each a few pages.

## Chapters

| Chapter | English | Japanese |
|---|---|---|
| 1. Architecture and data flow | [en/01-architecture-and-data-flow.md](en/01-architecture-and-data-flow.md) | [ja/01-architecture-and-data-flow.md](ja/01-architecture-and-data-flow.md) |
| 2. Cryptography | [en/02-cryptography.md](en/02-cryptography.md) | [ja/02-cryptography.md](ja/02-cryptography.md) |
| 3. Authentication and access | [en/03-authentication-and-access.md](en/03-authentication-and-access.md) | [ja/03-authentication-and-access.md](ja/03-authentication-and-access.md) |
| 4. Logging and retention | [en/04-logging-and-retention.md](en/04-logging-and-retention.md) | [ja/04-logging-and-retention.md](ja/04-logging-and-retention.md) |
| 5. Personal data | [en/05-personal-data.md](en/05-personal-data.md) | [ja/05-personal-data.md](ja/05-personal-data.md) |
| 6. Security questionnaire | [en/06-security-questionnaire.md](en/06-security-questionnaire.md) | [ja/06-security-questionnaire.md](ja/06-security-questionnaire.md) |

## Writing rules

1. State only what the code or a decision record proves. Each fact was
   checked in the file named in the chapter's last section.
2. Mark each thing that is not built as `Planned`, and each duty of the
   customer as `Customer`. The pack claims no certification, audit or
   organisational control.
3. Use plain words and short sentences, for a reader who does not read Rust:
   no type names, no internal IDs. Environment variable names are fine.

## Check the pack

```sh
sh scripts/check-security-pack.sh --complete
```

The script checks that English and Japanese chapters have the same headings
and table IDs, and that every source file a chapter names exists.
`--complete` requires all six chapters in both languages. CI runs it.

## Outside the pack

The owner does these, not the pack:

- Answers to the `Vendor` rows, for each customer.
- A legal review of chapter 5.
- A read of the Japanese by a native speaker before the first hand-over.

## Scope

The pack describes the product as built, not a promise about the future. It
is re-read for every release.
