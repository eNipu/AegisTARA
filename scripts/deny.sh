#!/bin/sh
# Run the cargo-deny policy (deny.toml, decision D5) on every production crate.
#
# Usage: scripts/deny.sh [CHECK...]
#   CHECK is any of: advisories bans licenses sources (default: all four).
#   `scripts/deny.sh licenses bans sources` runs fully offline.
#
# Checks each crates/*/Cargo.toml, then each non-shipped fuzz crate
# crates/*/fuzz/Cargo.toml, in sorted order. Keeps going after a failure,
# prints one summary line per crate, and exits 1 if any crate failed.
# The full cargo-deny report is printed only for failing crates.
#
# Licenses (decision D5, widened 2026-10-26): the allow list applies to what
# ships. cargo-deny already leaves dev-dependencies out of the license check
# (tools/deny-fixtures/run.sh proves it), and a fuzz crate is development
# tooling that nothing ships, so its licenses are not checked at all. Its
# advisories, bans and sources still are, and [bans] keeps its tooling crates
# out of every shipped crate.
#
# Cost: one cargo-deny run per crate, O(D) in its locked dependencies.
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
config="$repo_root/deny.toml"

if ! command -v cargo-deny >/dev/null 2>&1; then
    echo "error: cargo-deny is not installed." >&2
    echo "install it with: cargo install cargo-deny --version 0.20.2 --locked" >&2
    exit 127
fi

if [ "$#" -eq 0 ]; then
    set -- advisories bans licenses sources
fi

log=$(mktemp "${TMPDIR:-/tmp}/deny.XXXXXX")
trap 'rm -f "$log"' EXIT
# Stop on Ctrl-C or TERM instead of moving on to the next case.
trap 'rm -f "$log"; exit 130' INT TERM

failed=0
checked=0
# Globs expand in sorted order in POSIX sh.
for manifest in "$repo_root"/crates/*/Cargo.toml "$repo_root"/crates/*/fuzz/Cargo.toml; do
    [ -f "$manifest" ] || continue
    crate=${manifest#"$repo_root"/crates/}
    crate=${crate%/Cargo.toml}
    checked=$((checked + 1))
    # A fuzz crate is not shipped: leave `licenses` out of its checks.
    wanted=
    for check in "$@"; do
        case "$crate:$check" in
            */fuzz:licenses) ;;
            *) wanted="$wanted $check" ;;
        esac
    done
    if [ -z "$wanted" ]; then
        echo "ok    $crate (licenses are not checked: development tooling, not shipped)"
        continue
    fi
    # $wanted is a short list of check names, so word splitting is intended.
    # shellcheck disable=SC2086
    if cargo deny --manifest-path "$manifest" --config "$config" check $wanted >"$log" 2>&1; then
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
    echo "$failed of $checked crate(s) failed: $*"
    case " $* " in
        *" advisories "*)
            echo "note: advisories fetch the RustSec database; if the report shows a"
            echo "      network error, rerun offline with: scripts/deny.sh licenses bans sources"
            ;;
    esac
    exit 1
fi
