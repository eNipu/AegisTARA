#!/bin/sh
# Run the CI quality gates (standards Q1, Q7) on every production crate:
# rustfmt, clippy, and the tests. The GitHub Actions `check` job runs this
# same script, so a green local run means a green `check` job.
#
# Usage: scripts/ci.sh [CRATE_DIR...]
#   With no arguments, checks each crates/*/ that has a Cargo.toml, in sorted
#   order (the same discovery rule as scripts/deny.sh).
#
# Steps, run inside each crate directory:
#   fmt     cargo fmt --check
#   clippy  cargo clippy --locked --all-targets -- -D warnings
#   test    cargo test --locked
# `--locked` fails if Cargo.lock is out of date, so CI builds exactly the
# committed dependency set.
#
# Keeps going after a failure, prints one `ok` or `FAIL` line per step, and
# prints the full output only for failing steps. Exits 1 if any step failed or
# if no crate was found. Inside GitHub Actions each crate is a folded log group
# and each failing step is an error annotation.
#
# Cost: three cargo runs per crate, O(crates x build time).
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is not installed (https://rustup.rs)." >&2
    exit 127
fi

log=$(mktemp "${TMPDIR:-/tmp}/ci.XXXXXX")
trap 'rm -f "$log"' EXIT
# Stop on Ctrl-C or TERM instead of moving on to the next step.
trap 'rm -f "$log"; exit 130' INT TERM

in_actions=false
[ "${GITHUB_ACTIONS:-}" = "true" ] && in_actions=true

checked=0
failed=0
# Space-separated "crate/step" list for the summary line.
failures=""

# run_step CRATE DIR STEP COMMAND...
# Runs COMMAND inside DIR and records the result. O(1) besides the command.
run_step() {
    rs_crate=$1
    rs_dir=$2
    rs_step=$3
    shift 3
    if (cd "$rs_dir" && "$@") >"$log" 2>&1; then
        echo "ok    $rs_crate $rs_step"
    else
        echo "FAIL  $rs_crate $rs_step"
        sed 's/^/      /' "$log"
        if [ "$in_actions" = true ]; then
            echo "::error title=ci.sh::FAIL $rs_crate $rs_step"
        fi
        failed=$((failed + 1))
        failures="$failures $rs_crate/$rs_step"
    fi
}

# check_crate DIR
# Runs every step for the crate in DIR, even after an earlier step failed.
check_crate() {
    cc_dir=$1
    cc_crate=$(basename "$cc_dir")
    if [ ! -f "$cc_dir/Cargo.toml" ]; then
        echo "error: no Cargo.toml in $cc_dir" >&2
        failed=$((failed + 1))
        failures="$failures $cc_crate/manifest"
        return 0
    fi
    checked=$((checked + 1))
    [ "$in_actions" = true ] && echo "::group::$cc_crate"
    run_step "$cc_crate" "$cc_dir" fmt cargo fmt --check
    run_step "$cc_crate" "$cc_dir" clippy cargo clippy --locked --all-targets -- -D warnings
    run_step "$cc_crate" "$cc_dir" test cargo test --locked
    [ "$in_actions" = true ] && echo "::endgroup::"
    return 0
}

if [ "$#" -gt 0 ]; then
    for dir in "$@"; do
        check_crate "${dir%/}"
    done
else
    # Globs expand in sorted order in POSIX sh.
    for manifest in "$repo_root"/crates/*/Cargo.toml; do
        [ -f "$manifest" ] || continue
        check_crate "$(dirname "$manifest")"
    done
fi

if [ "$checked" -eq 0 ]; then
    if [ "$#" -gt 0 ]; then
        echo "error: no crates found in: $*" >&2
    else
        echo "error: no crates found under $repo_root/crates" >&2
    fi
    exit 1
fi

if [ "$failed" -ne 0 ]; then
    echo "$failed step(s) failed in $checked crate(s):$failures"
    exit 1
fi
echo "all steps passed in $checked crate(s)"
