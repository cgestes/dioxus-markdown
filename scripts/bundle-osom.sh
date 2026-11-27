#!/usr/bin/env bash
# Build Osom, the Topics window as its own macOS app, into target/Osom.app.
#
#   scripts/bundle-osom.sh
#   open target/Osom.app
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/crates/osom/Cargo.toml" | head -1)"
app="$target/Osom.app"

cargo build --locked --release -p osom --manifest-path "$root/Cargo.toml"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$target/release/osom" "$app/Contents/MacOS/osom"

# The icon, drawn by scripts/osom-icon.swift and packed by iconutil.
iconset="$(mktemp -d)/Osom.iconset"
swift "$root/scripts/osom-icon.swift" "$iconset"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Osom.icns"
rm -rf "$(dirname "$iconset")"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Osom</string>
    <key>CFBundleDisplayName</key>
    <string>Osom</string>
    <key>CFBundleExecutable</key>
    <string>osom</string>
    <key>CFBundleIconFile</key>
    <string>Osom</string>
    <key>CFBundleIdentifier</key>
    <string>dev.margin.osom</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

# Ad-hoc signature: one stable identity for the app's Keychain entries.
codesign --force --sign - "$app" >/dev/null
echo "$app"
