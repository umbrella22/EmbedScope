#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p escope-desktop
bundle="target/EmbedScope Inspector.app"
mkdir -p "$bundle/Contents/MacOS"
# Replace the inode so macOS does not reuse a cached signature for an older build.
staged_executable=$(mktemp "$bundle/Contents/MacOS/.escope-desktop.XXXXXX")
trap 'rm -f "$staged_executable"' EXIT
cp target/debug/escope-desktop "$staged_executable"
chmod 755 "$staged_executable"
mv -f "$staged_executable" "$bundle/Contents/MacOS/escope-desktop"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>escope-desktop</string>
<key>CFBundleIdentifier</key><string>dev.embedscope.inspector</string>
<key>CFBundleName</key><string>EmbedScope Inspector</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
# Local development signing seals the executable together with its Info.plist.
codesign --force --sign - --timestamp=none "$bundle"
codesign --verify --strict "$bundle"
printf '%s\n' "$PWD/$bundle"
