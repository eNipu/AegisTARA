#!/bin/sh
# Prove an offline bundle can be checked as a customer would: with OpenSSL 3
# and sha256sum alone (job 100).
#
# Usage: scripts/offline-bundle-test.sh
#   Needs cargo, tar and openssl 3 (OPENSSL may name the binary; LibreSSL
#   cannot check Ed25519 with -rawin). Uses a throwaway key in a temp folder.
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo_root"

OPENSSL=${OPENSSL:-openssl}
for tool in cargo tar "$OPENSSL"; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: $tool is not installed." >&2
        exit 127
    fi
done
case $("$OPENSSL" version | sed -n 's/^OpenSSL \([0-9]*\)\..*/\1/p') in
    '' | 0 | 1 | 2)
        echo "error: $OPENSSL is not OpenSSL 3 or newer (LibreSSL cannot check Ed25519 with -rawin); set OPENSSL=/path/to/openssl" >&2
        exit 127
        ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
    sums_check() { sha256sum -c "$@"; }
    sums_make() { sha256sum "$@"; }
else
    sums_check() { shasum -a 256 -c "$@"; }
    sums_make() { shasum -a 256 "$@"; }
fi

work=$(mktemp -d)
cleanup() {
    rm -rf "$work"
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM

fail() {
    echo "FAIL  $*" >&2
    exit 1
}
ok() {
    echo "ok    $*"
}

cargo build --locked -p aegis-license --features issuer --bin aegis-license
bin=${CARGO_TARGET_DIR:-$repo_root/target}/debug/aegis-license
AEGIS_LICENSE_BIN=$bin
export AEGIS_LICENSE_BIN

cd "$work"
"$bin" keygen --key-id test-upd --out key.pk8 > keys.txt 2>/dev/null

installer=aegis-9.9.9-linux-x86_64.tar.gz
head -c 4096 /dev/urandom > "$installer"
sha=$(sums_make "$installer" | cut -d ' ' -f 1)
size=$(wc -c < "$installer" | tr -d ' ')
cat > manifest.json <<JSON
{
  "release": "9.9.9",
  "published_at": 1800000000,
  "artifacts": [
    {
      "platform": "linux-x86_64",
      "url": "https://downloads.example.com/$installer",
      "sha256": "$sha",
      "bytes": $size
    }
  ],
  "notes": "Test."
}
JSON
"$bin" sign-update --key key.pk8 --key-id test-upd --in manifest.json > update.aegis
head -c 4096 /dev/urandom > images.tar.gz

mkdir out
"$repo_root/scripts/offline-bundle.sh" --manifest update.aegis --installer "$installer" \
    --images images.tar.gz --key key.pk8 --keys keys.txt --out out > /dev/null \
    || fail "scripts/offline-bundle.sh"
ok "scripts/offline-bundle.sh built the bundle"

(cd out && sums_check aegistara-9.9.9-offline.tar.sha256 > /dev/null) \
    || fail "the tar does not match its .sha256 file"
ok "tar matches its .sha256 file"

mkdir site
tar -xf out/aegistara-9.9.9-offline.tar -C site
bundle=$work/site/aegistara-9.9.9-offline
"$bin" public-pem --keys keys.txt --key-id test-upd > update.pem

"$OPENSSL" pkeyutl -verify -pubin -inkey update.pem -rawin \
    -in "$bundle/SHA256SUMS" -sigfile "$bundle/SHA256SUMS.sig" > /dev/null \
    || fail "openssl does not verify SHA256SUMS.sig"
ok "openssl verifies SHA256SUMS.sig"

(cd "$bundle" && sums_check --strict SHA256SUMS > /dev/null) \
    || fail "sha256sum -c --strict SHA256SUMS"
ok "sha256sum -c --strict SHA256SUMS"

"$bin" verify-bundle "$bundle" --keys keys.txt | grep -q '^bundle ok$' \
    || fail "aegis-license verify-bundle"
ok "aegis-license verify-bundle prints bundle ok"

# Negative checks, each on a fresh copy.
fresh() {
    rm -rf bad
    mkdir bad
    cp -R "$bundle" bad/b
}

fresh
printf 'x' >> "bad/b/installers/$installer"
if "$bin" verify-bundle bad/b --keys keys.txt > /dev/null 2>&1; then
    fail "verify-bundle accepted a changed installer"
fi
if (cd bad/b && sums_check --strict SHA256SUMS > /dev/null 2>&1); then
    fail "sha256 -c accepted a changed installer"
fi
ok "a changed installer is refused by both checks"

fresh
echo extra > bad/b/notes.txt
if "$bin" verify-bundle bad/b --keys keys.txt > /dev/null 2>&1; then
    fail "verify-bundle accepted an extra file"
fi
ok "an extra file is refused"

fresh
first=$(head -c 1 bad/b/SHA256SUMS)
if [ "$first" = 0 ]; then other=1; else other=0; fi
{ printf '%s' "$other"; tail -c +2 "$bundle/SHA256SUMS"; } > bad/b/SHA256SUMS
if "$OPENSSL" pkeyutl -verify -pubin -inkey update.pem -rawin \
    -in bad/b/SHA256SUMS -sigfile bad/b/SHA256SUMS.sig > /dev/null 2>&1; then
    fail "openssl accepted a changed SHA256SUMS"
fi
ok "a changed SHA256SUMS fails the signature"

mkdir dup1 dup2 out2
cp "$installer" dup1/same.tar.gz
cp "$installer" dup2/same.tar.gz
set +e
"$repo_root/scripts/offline-bundle.sh" --manifest update.aegis \
    --installer dup1/same.tar.gz --installer dup2/same.tar.gz \
    --key key.pk8 --keys keys.txt --out out2 > /dev/null 2>&1
status=$?
set -e
[ "$status" = 2 ] || fail "offline-bundle.sh accepted two installers with one name (exit $status)"
ok "two installers with one file name are refused"

mkdir "img dir" out3
cp images.tar.gz "img dir/images.tar.gz"
"$repo_root/scripts/offline-bundle.sh" --manifest update.aegis --installer "$installer" \
    --images "img dir/images.tar.gz" --key key.pk8 --keys keys.txt --out out3 > /dev/null \
    || fail "offline-bundle.sh with an images path containing a space"
ok "an images path with a space works"

echo "all offline bundle checks passed"
