VERDICT: PASS

Surface: CLI (scripts/deny.sh, tools/deny-fixtures/run.sh) and README section. No desktop-app change.

- Flow: one command, optional check subset, offline variant documented. Full report only for failing crates; final summary line.
- Errors: missing cargo-deny exits 127 with exact install command. No crates found exits 1 with message. Fixture runner separates `error` from a real rejection.
- No silent failures (set -eu, keeps going after a failure).
- Keyboard/a11y and 50 ms budget: not applicable.

Non-blocking:
- scripts/deny.sh:41 — advisories network failure looks like a real advisory FAIL — add a hint to rerun with `licenses bans sources`.
- scripts/deny.sh:55 — summary goes to stderr while FAIL lines go to stdout, so CI logs may interleave — send both to stderr or leave.
