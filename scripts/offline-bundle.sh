#!/bin/sh
# Build the offline bundle for an air-gapped customer site (job 100).
#
# Usage: scripts/offline-bundle.sh --manifest UPDATE.aegis --installer FILE [--installer FILE]...
#                                  [--images IMAGES.tar.gz] --key KEYFILE --keys KEYS_FILE --out DIR
#
# Makes DIR/aegistara-<release>-offline.tar and DIR/aegistara-<release>-offline.tar.sha256.
# The tar holds one folder, aegistara-<release>-offline/:
#
#   SHA256SUMS        sha256 of every file below
#   SHA256SUMS.sig    raw Ed25519 signature over SHA256SUMS (the update key)
#   update.aegis      the release's signed update manifest
#   installers/<file> the desktop installers
#   images.tar.gz     optional server images
#
# --images is the output of
#   docker save aegistara-server:<release> postgres:16 | gzip -n > images.tar.gz
# --keys is the update key list (normally crates/aegis-license/keys/update.keys).
# --key is the private update key file; it is only read, never copied.
#
# AEGIS_LICENSE_BIN may name a built `aegis-license` binary. Otherwise the
# script runs `cargo build --locked --release -p aegis-license --features
# issuer --bin aegis-license` and uses
# ${CARGO_TARGET_DIR:-<repo>/target}/release/aegis-license.
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)

usage_error() {
    echo "error: $*" >&2
    echo "usage: scripts/offline-bundle.sh --manifest UPDATE.aegis --installer FILE [--installer FILE]... [--images IMAGES.tar.gz] --key KEYFILE --keys KEYS_FILE --out DIR" >&2
    exit 2
}

manifest=
images=
key=
keys=
out=
installers=
while [ $# -gt 0 ]; do
    [ $# -ge 2 ] || usage_error "$1 needs a value"
    case "$1" in
        --manifest) manifest=$2 ;;
        --installer) installers="$installers
$2" ;;
        --images) images=$2 ;;
        --key) key=$2 ;;
        --keys) keys=$2 ;;
        --out) out=$2 ;;
        *) usage_error "unknown flag $1" ;;
    esac
    shift 2
done

[ -n "$manifest" ] || usage_error "--manifest is required"
[ -n "$installers" ] || usage_error "at least one --installer is required"
[ -n "$key" ] || usage_error "--key is required"
[ -n "$keys" ] || usage_error "--keys is required"
[ -n "$out" ] || usage_error "--out is required"
[ -d "$out" ] || usage_error "--out is not a folder: $out"

for f in "$manifest" "$key" "$keys" $images; do
    [ -f "$f" ] || usage_error "not a regular file: $f"
done
old_ifs=$IFS
IFS='
'
for f in $installers; do
    [ -n "$f" ] || continue
    [ -f "$f" ] || usage_error "not a regular file: $f"
    case $(basename "$f") in
        [!A-Za-z0-9]* | *[!A-Za-z0-9._-]*) usage_error "bad installer file name: $f" ;;
    esac
done
IFS=$old_ifs

if [ -n "${AEGIS_LICENSE_BIN:-}" ]; then
    bin=$AEGIS_LICENSE_BIN
else
    (cd "$repo_root" && cargo build --locked --release -p aegis-license --features issuer --bin aegis-license)
    bin=${CARGO_TARGET_DIR:-$repo_root/target}/release/aegis-license
fi
[ -x "$bin" ] || usage_error "aegis-license binary not found: $bin"

# This also proves the manifest is signed by a trusted update key.
release=$("$bin" inspect-update "$manifest" --keys "$keys" | sed -n 's/^release: *//p')
[ -n "$release" ] || { echo "error: could not read the release from $manifest" >&2; exit 1; }

name=aegistara-$release-offline
if [ -e "$out/$name" ] || [ -e "$out/$name.tar" ]; then
    echo "error: $out/$name or $out/$name.tar already exists" >&2
    exit 1
fi

mkdir -p "$out/$name/installers"
cp "$manifest" "$out/$name/update.aegis"
IFS='
'
for f in $installers; do
    [ -n "$f" ] || continue
    cp "$f" "$out/$name/installers/$(basename "$f")"
done
IFS=$old_ifs
if [ -n "$images" ]; then
    cp "$images" "$out/$name/images.tar.gz"
fi

"$bin" sign-bundle "$out/$name" --key "$key"
"$bin" verify-bundle "$out/$name" --keys "$keys"

tar -cf "$out/$name.tar" -C "$out" "$name"
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$out" && sha256sum "$name.tar" > "$name.tar.sha256")
else
    (cd "$out" && shasum -a 256 "$name.tar" > "$name.tar.sha256")
fi

echo "$out/$name.tar"
echo "$out/$name.tar.sha256"
