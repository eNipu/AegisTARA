#!/bin/sh
# Run the cargo-deny policy (deny.toml, decision D5) on every production crate.
#
# Usage: scripts/deny.sh [CHECK...]
#   CHECK is any of: advisories bans licenses sources (default: all four).
#   `scripts/deny.sh licenses bans sources` runs fully offline.
#
# Checks each crates/*/Cargo.toml in sorted order, keeps going after a
# failure, prints one summary line per crate, and exits 1 if any crate failed.
# The full cargo-deny report is printed only for failing crates.
#
# Cost: one cargo-deny run per crate, O(D) in its locked dependencies.
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
config="$repo_root/deny.toml"

if ! command -v cargo-deny >/dev/null 2>&1; then
    echo "error: cargo-deny is not installed." >&2
    echo "install it with: cargo install cargo-deny --version 0.19.9 --locked" >&2
    exit 127
fi

if [ "$#" -eq 0 ]; then
    set -- advisories bans licenses sources
fi

log=$(mktemp "${TMPDIR:-/tmp}/deny.XXXXXX")
trap 'rm -f "$log"' EXIT INT TERM

failed=0
checked=0
# Globs expand in sorted order in POSIX sh.
for manifest in "$repo_root"/crates/*/Cargo.toml; do
    [ -f "$manifest" ] || continue
    crate=$(basename "$(dirname "$manifest")")
    checked=$((checked + 1))
    if cargo deny --manifest-path "$manifest" check --config "$config" "$@" >"$log" 2>&1; then
        echo "ok    $crate"
    else
        echo "FAIL  $crate"
        sed 's/^/      /' "$log"
        failed=$((failed + 1))
    fi
done

if [ "$checked" -eq 0 ]; then
    echo "error: no crates found under $repo_root/crates" >&2
    exit 1
fi

if [ "$failed" -ne 0 ]; then
    echo "$failed of $checked crate(s) failed: $*" >&2
    exit 1
fi
