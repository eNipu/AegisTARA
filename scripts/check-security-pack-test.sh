#!/bin/sh
# Prove scripts/check-security-pack.sh refuses each kind of broken pack (job 102).
#
# Usage: scripts/check-security-pack-test.sh
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
check="$repo_root/scripts/check-security-pack.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
good=$tmp/good
export PACK_ROOT=$tmp/root
mkdir -p "$PACK_ROOT" "$good/en" "$good/ja"
echo x >"$PACK_ROOT/src.txt"

# chapter <file> <number> <word>
chapter() {
    cat >"$1" <<EOT
# $2. Chapter $3

## $2.1 First $3

Text.

| ID | Thing |
|---|---|
| DF-01 | $3 |

## $2.2 Second $3

## $2.3 Sources

- \`src.txt\`
EOT
}

for lang in en ja; do
    chapter "$good/$lang/01-architecture-and-data-flow.md" 1 "one-$lang"
    chapter "$good/$lang/02-cryptography.md" 2 "two-$lang"
done

failed=0
out=$tmp/out

# expect <case> <want-exit> <args...>
expect() {
    name=$1
    want=$2
    shift 2
    rc=0
    sh "$check" "$@" >"$out" 2>&1 || rc=$?
    if [ "$rc" -ne "$want" ]; then
        echo "FAIL $name (exit $rc, wanted $want)"
        failed=1
    elif [ "$want" -eq 1 ] && ! grep -q FAIL "$out"; then
        echo "FAIL $name (no FAIL line)"
        failed=1
    else
        echo "ok  $name"
    fi
}

# broken <case> <command run inside a fresh copy>
broken() {
    name=$1
    rm -rf "$tmp/bad"
    cp -R "$good" "$tmp/bad"
    (cd "$tmp/bad" && eval "$2")
    expect "$name" 1 "$tmp/bad"
}

expect "good pack" 0 "$good"
broken "ja section removed" "sed -i.bak -e '/^## 1.2 /,/^## 1.3 /{' -e '/^## 1.3 /!d' -e '}' -e 's/^## 1.3 /## 1.2 /' ja/01-architecture-and-data-flow.md && rm -f ja/*.bak"
broken "ja row ID removed" "sed -i.bak 's/DF-01/none/' ja/01-architecture-and-data-flow.md && rm -f ja/*.bak"
broken "missing source path" "sed -i.bak 's/src.txt/gone.txt/' en/02-cryptography.md && rm -f en/*.bak"
broken "heading gap" "sed -i.bak 's/^## 1.2 /## 1.3 /' en/01-architecture-and-data-flow.md ja/01-architecture-and-data-flow.md && rm -f en/*.bak ja/*.bak"
broken "extra file" "echo '# 7. Extra' >en/07-extra.md"
broken "marker word" "echo 'TODO later' >>en/02-cryptography.md"
broken "untranslated ja" "cp en/02-cryptography.md ja/02-cryptography.md"
expect "complete but chapters missing" 1 --complete "$good"

if [ "$failed" -ne 0 ]; then
    exit 1
fi
echo "all cases passed"
