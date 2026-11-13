#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked
bundle="target/Zero Term.app"
mkdir -p "$bundle/Contents/MacOS"
cp target/debug/zero-term "$bundle/Contents/MacOS/zero-term.new"
mv "$bundle/Contents/MacOS/zero-term.new" "$bundle/Contents/MacOS/zero-term"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>zero-term</string>
<key>CFBundleIdentifier</key><string>dev.zero-robotic.zero-term</string>
<key>CFBundleName</key><string>Zero Term</string>
<key>CFBundleDisplayName</key><string>Zero Term</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf 'Built %s\n' "$bundle"
