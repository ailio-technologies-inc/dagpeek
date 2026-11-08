#!/usr/bin/env bash
# Renders the real popover with sample data to docs/screenshot.png, for the README.
# release.sh runs this, so the image can't drift from what ships.
set -euo pipefail
cd "$(dirname "$0")/.."

CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PORT=8791
tmp=$(mktemp -d)
server=""
cleanup() { [ -n "$server" ] && kill "$server" 2>/dev/null || true; rm -rf "$tmp"; }
trap cleanup EXIT

(cd ui && pnpm exec vite build --logLevel error --emptyOutDir --outDir "$tmp/dist")
cp scripts/screenshot-fixture.js "$tmp/dist/fixture.js"
# Solid colours stand in for the window's glass, which only exists in the real app.
perl -0pi -e 's#<head>#<head><script src="/fixture.js"></script><style>:root{--glass:\#f8f8fa !important;--raised:\#fff !important}html,body{width:380px}</style>#' "$tmp/dist/index.html"

python3 -I -m http.server "$PORT" --bind 127.0.0.1 --directory "$tmp/dist" >/dev/null 2>&1 &
server=$!
sleep 1

"$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-prefers-reduced-motion \
  --force-color-profile=srgb --force-device-scale-factor=2 --default-background-color=00000000 \
  --user-data-dir="$tmp/profile" --window-size=500,800 --virtual-time-budget=2500 \
  --screenshot="$tmp/raw.png" "http://127.0.0.1:$PORT/" >/dev/null 2>&1

mkdir -p docs
magick "$tmp/raw.png" -trim +repage \
  \( +clone -background '#0a1a32' -shadow 45x18+0+10 \) +swap -background none -layers merge +repage \
  -bordercolor none -border 44 -background '#3d74ad' -flatten -strip docs/screenshot.png
echo "wrote docs/screenshot.png"
