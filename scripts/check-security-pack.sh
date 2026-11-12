#!/bin/sh
# Keep the product security pack honest (job 102).
#
# Usage: scripts/check-security-pack.sh [--complete] [PACK_DIR]
#   PACK_DIR defaults to docs/security-pack under the repository root.
#   Source paths are checked against the repository root, or against the
#   folder in PACK_ROOT when it is set.
#   --complete also requires all six chapters in en/ and ja/.
#
# Rules (one `FAIL <file>: <reason>` line per failure; the script keeps going):
#   R1 names    only the six chapter file names are allowed in en/ and ja/
#   R2 pairs    every ja/ file has an en/ file; --complete wants all six
#   R3 shape    line 1 is `# `; every `## ` heading is `## N.M ` counting
#               1, 2, 3 without a gap; at least two of them
#   R4 sources  the last section lists `- `path`` lines; each path exists
#   R5 parity   en and ja have the same heading numbers and row IDs, in order
#   R6 markers  no TODO, TBD, FIXME or XXX
#   R7 ja       a ja/ file is not byte-identical to its en/ file
#   R8 length   no chapter is longer than 220 lines
# Exits 1 if any rule failed, else prints `security pack ok (<n> files)`.
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
complete=0
if [ "${1:-}" = "--complete" ]; then
    complete=1
    shift
fi
pack=${1:-$repo_root/docs/security-pack}
root=${PACK_ROOT:-$repo_root}
names="01-architecture-and-data-flow.md 02-cryptography.md 03-authentication-and-access.md 04-logging-and-retention.md 05-personal-data.md 06-security-questionnaire.md"
fails=0
count=0

fail() {
    echo "FAIL $1: $2"
    fails=$((fails + 1))
}

# Heading numbers (N.M) of a chapter, one per line.
heads() {
    sed -n 's/^## \([0-9][0-9]*\.[0-9][0-9]*\) .*/\1/p' "$1"
}

# Row IDs of a chapter, one per line.
ids() {
    sed -n 's/^| \([A-Z][A-Z]*-[0-9][0-9]*\) |.*/\1/p' "$1"
}

check_chapter() {
    f=$1
    label=$2
    base=$(basename "$f")
    first=$(sed -n 1p "$f")
    case $first in
        '# '*) ;;
        *) fail "$label" "line 1 must start with '# '" ;;
    esac
    n=$(printf '%s' "$base" | cut -c1-2 | sed 's/^0//')
    expect=1
    for h in $(grep '^## ' "$f" | cut -d' ' -f2); do
        [ "$h" = "$n.$expect" ] || fail "$label" "heading '$h' should be '$n.$expect'"
        expect=$((expect + 1))
    done
    [ "$expect" -ge 3 ] || fail "$label" "needs at least two '## ' headings"
    # R4: the last section.
    last=$(grep -n '^## ' "$f" | tail -n 1 | cut -d: -f1 || true)
    paths=$(tail -n +"${last:-1}" "$f" | sed -n 's/^- `\([^`]*\)`$/\1/p')
    [ -n "$paths" ] || fail "$label" "last section lists no source path"
    for p in $paths; do
        [ -e "$root/$p" ] || fail "$label" "source path '$p' does not exist"
    done
    grep -q -w -e TODO -e TBD -e FIXME -e XXX "$f" && fail "$label" "contains a marker word"
    [ "$(wc -l <"$f")" -le 220 ] || fail "$label" "longer than 220 lines"
}

for lang in en ja; do
    dir=$pack/$lang
    [ -d "$dir" ] || continue
    for f in "$dir"/*.md; do
        [ -e "$f" ] || continue
        base=$(basename "$f")
        count=$((count + 1))
        case " $names " in
            *" $base "*) ;;
            *) fail "$lang/$base" "not one of the six chapter names"; continue ;;
        esac
        check_chapter "$f" "$lang/$base"
        if [ "$lang" = "ja" ]; then
            if [ ! -e "$pack/en/$base" ]; then
                fail "ja/$base" "no en/$base"
            elif cmp -s "$f" "$pack/en/$base"; then
                fail "ja/$base" "identical to en/$base"
            else
                [ "$(heads "$f")" = "$(heads "$pack/en/$base")" ] ||
                    fail "ja/$base" "heading numbers differ from en"
                [ "$(ids "$f")" = "$(ids "$pack/en/$base")" ] ||
                    fail "ja/$base" "row IDs differ from en"
            fi
        fi
    done
done

if [ "$complete" -eq 1 ]; then
    for lang in en ja; do
        for n in $names; do
            [ -e "$pack/$lang/$n" ] || fail "$lang/$n" "missing"
        done
    done
fi

[ "$fails" -eq 0 ] || exit 1
echo "security pack ok ($count files)"
