#!/usr/bin/env bash
# Installs the latest Dagpeek on a Mac: downloads it, checks its SHA-256, copies it to
# /Applications and links the dagpeek command.
set -euo pipefail

BASE=https://github.com/ailio-technologies-inc/dagpeek/releases/latest/download
[ "$(uname)" = Darwin ] || { echo "This installer is for macOS." >&2; exit 1; }

tmp=$(mktemp -d)
trap 'hdiutil detach -quiet "$tmp/mnt" 2>/dev/null || true; rm -rf "$tmp"' EXIT

curl -fsSL "$BASE/Dagpeek-mac.dmg" -o "$tmp/Dagpeek-mac.dmg"
curl -fsSL "$BASE/SHA256SUMS.txt" -o "$tmp/SHA256SUMS.txt"
(cd "$tmp" && grep ' Dagpeek-mac.dmg$' SHA256SUMS.txt | shasum -a 256 -c -) >/dev/null \
  || { echo "The download's SHA-256 does not match; nothing was installed." >&2; exit 1; }

hdiutil attach -quiet -nobrowse -mountpoint "$tmp/mnt" "$tmp/Dagpeek-mac.dmg"
rm -rf /Applications/Dagpeek.app
cp -R "$tmp/mnt/Dagpeek.app" /Applications/

bin=/usr/local/bin
[ -w "$bin" ] || bin="$HOME/.local/bin"
mkdir -p "$bin"
ln -sf /Applications/Dagpeek.app/Contents/MacOS/dagpeek "$bin/dagpeek"

echo "Dagpeek is in /Applications. The dagpeek command is at $bin/dagpeek."
