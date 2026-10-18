#!/bin/sh
# Prove the license policy in deny.toml accepts and rejects what decision D5
# says it should. Each case is a `publish = false` root crate with one path
# dependency (in deps/) under the license named in the table below.
#
# case       dependency license                                  expected
# ok-mit     MIT                                                 pass
# ok-or      GPL-3.0-only OR MIT                                 pass
# bad-gpl    GPL-3.0-only                                        fail
# bad-agpl   AGPL-3.0-only                                       fail
# bad-lgpl   LGPL-2.1-or-later                                   fail
# bad-mpl    MPL-2.0 (review first)                              fail
# bad-slint  GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0   fail
# bad-and    MIT AND GPL-3.0-only                                fail
#
# Usage: tools/deny-fixtures/run.sh
# Runs offline (no registry dependencies). Exits 1 on any mismatch.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
config="$here/../../deny.toml"

if ! command -v cargo-deny >/dev/null 2>&1; then
    echo "error: cargo-deny is not installed." >&2
    echo "install it with: cargo install cargo-deny --version 0.19.9 --locked" >&2
    exit 127
fi

log=$(mktemp "${TMPDIR:-/tmp}/deny-fixture.XXXXXX")
trap 'rm -f "$log"' EXIT INT TERM

mismatches=0
total=0
# The here-document keeps the loop in this shell, so the counters survive it.
while read -r case expected; do
    [ -n "$case" ] || continue
    total=$((total + 1))
    if cargo deny --manifest-path "$here/$case/Cargo.toml" check --config "$config" licenses >"$log" 2>&1; then
        actual=pass
    elif grep -q 'error\[rejected\]' "$log"; then
        # Count a failure only when cargo-deny rejected a license, so a broken
        # config or manifest cannot pass as a correct rejection.
        actual=fail
    else
        actual=error
    fi
    if [ "$actual" = "$expected" ]; then
        printf 'ok        %-10s expected %s, got %s\n' "$case" "$expected" "$actual"
    else
        printf 'MISMATCH  %-10s expected %s, got %s\n' "$case" "$expected" "$actual"
        sed 's/^/          /' "$log"
        mismatches=$((mismatches + 1))
    fi
done <<'CASES'
ok-mit    pass
ok-or     pass
bad-gpl   fail
bad-agpl  fail
bad-lgpl  fail
bad-mpl   fail
bad-slint fail
bad-and   fail
CASES

if [ "$mismatches" -ne 0 ]; then
    echo "$mismatches of $total fixture case(s) did not match the policy table" >&2
    exit 1
fi
echo "all $total fixture cases match"
