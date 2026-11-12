# AegisTARA product security pack

This pack answers the questions in a vendor security questionnaire. It is for
the security reviewer at a buyer, and for the person at the vendor who hands
the pack over before a proof of concept. It is short on purpose: six chapters,
each a few pages.

## Chapters

| Chapter | English | Japanese |
|---|---|---|
| 1. Architecture and data flow | [en/01-architecture-and-data-flow.md](en/01-architecture-and-data-flow.md) | added in a later change |
| 2. Cryptography | added in a later change | added in a later change |
| 3. Authentication and access | added in a later change | added in a later change |
| 4. Logging and retention | added in a later change | added in a later change |
| 5. Personal data | added in a later change | added in a later change |
| 6. Security questionnaire | added in a later change | added in a later change |

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
sh scripts/check-security-pack.sh
```

The script checks that English and Japanese chapters have the same headings
and table IDs, and that every source file a chapter names exists. Add
`--complete` to require all six chapters in both languages.

## Scope

The pack describes the product as built, not a promise about the future. It
is re-read for every release.
