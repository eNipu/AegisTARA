#!/bin/sh
# Wrap the aegis-shell binary in a macOS app bundle, so the Dock and Finder
# show the AegisTARA icon and name instead of the generic "exec" icon. macOS
# takes an app's icon from its bundle (Contents/Resources + Info.plist); a bare
# binary started from a terminal or `cargo run` has none, and the window's own
# icon property does not reach the Dock.
#
# Usage: scripts/bundle-macos.sh [debug|release]   (default: release)
# Output: target/<profile>/AegisTARA.app; open it with `open target/release/AegisTARA.app`.
# Environment variables (AEGIS_URL, AEGIS_RENDERER, ...) pass through with
# `open --env NAME=value` or by running AegisTARA.app/Contents/MacOS/aegis-shell.
#
# ponytail: unsigned bundle for local use; signing and notarisation belong to
# the release pipeline (docs/release-checklist.md).
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
profile=${1:-release}
case $profile in
    release) cargo build --release -p aegis-shell --manifest-path "$repo_root/Cargo.toml" ;;
    debug) cargo build -p aegis-shell --manifest-path "$repo_root/Cargo.toml" ;;
    *) echo "usage: $0 [debug|release]" >&2; exit 2 ;;
esac

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo_root/crates/aegis-shell/Cargo.toml" | head -n 1)
app="$repo_root/target/$profile/AegisTARA.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$repo_root/target/$profile/aegis-shell" "$app/Contents/MacOS/aegis-shell"
cp "$repo_root/crates/aegis-shell/ui/brand/AegisTARA.icns" "$app/Contents/Resources/AegisTARA.icns"
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>AegisTARA</string>
    <key>CFBundleDisplayName</key><string>AegisTARA</string>
    <key>CFBundleIdentifier</key><string>com.aegistara.desktop</string>
    <key>CFBundleExecutable</key><string>aegis-shell</string>
    <key>CFBundleIconFile</key><string>AegisTARA</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict>
</plist>
EOF
echo "$app"
