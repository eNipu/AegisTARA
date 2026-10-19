VERDICT: PASS

Surface: CLI (scripts/deny.sh, tools/deny-fixtures/run.sh) and README section. No desktop-app change.

- Flow: one command, optional check subset, offline variant documented. README shows the cargo-deny 0.20 `--config` before `check` ordering.
- Errors: missing cargo-deny exits 127 with the exact pinned install command. No crates found exits 1 with a message. Fixture runner distinguishes error from a real rejection. Round 1 advisory network hint is now present (scripts/deny.sh end), and the summary prints on stdout with the FAIL lines.
- No silent failures (set -eu, continues after failure, INT/TERM exit 130).
- Keyboard/a11y and 50 ms budget: not applicable.

Non-blocking:
- tools/deny-fixtures/run.sh final mismatch summary goes to stderr while case lines go to stdout (same interleave nit as before) - send to stdout or leave.
