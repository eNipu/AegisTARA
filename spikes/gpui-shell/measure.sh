#!/bin/sh
# Runs the shell in measure mode and prints frame, memory and size numbers.
# Usage: sh measure.sh [nodes] [frames]
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin="${CARGO_TARGET_DIR:-$here/target}/release/aegis-spike-gpui-shell"
if [ ! -x "$bin" ]; then
    echo "binary missing: $bin"
    echo "build it: cargo build --release --locked --manifest-path spikes/gpui-shell/Cargo.toml"
    exit 1
fi

out=$(mktemp)
trap 'rm -f "$out"' EXIT
"$bin" --measure --nodes "${1:-100000}" --frames "${2:-300}" >"$out" &
pid=$!

waited=0
until grep -q MEASURE_DONE "$out"; do
    if [ "$waited" -ge 600 ]; then
        echo "timed out after 120 s"
        kill "$pid" 2>/dev/null || true
        exit 1
    fi
    sleep 0.2
    waited=$((waited + 1))
done

rss_kb=$(ps -o rss= -p "$pid" | tr -d ' ')
wait "$pid" || true

cat "$out"
echo "RSS_MB $(awk -v k="$rss_kb" 'BEGIN { printf "%.1f", k / 1024 }')"
size=$(stat -f%z "$bin" 2>/dev/null || stat -c%s "$bin")
echo "BINARY_MB $(awk -v b="$size" 'BEGIN { printf "%.1f", b / 1048576 }')"
echo "OS $(uname -sr)"
echo "MACHINE $(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m)"
