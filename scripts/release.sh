#!/usr/bin/env bash
# Builds, signs, notarizes and publishes a Mac release:  scripts/release.sh 1.0.0
#
# Needs: the Developer ID certificate, the `dagpeek` notarytool profile, the App Store Connect key in
# ~/.appstoreconnect/private_keys, the updater key at ~/.tauri/dagpeek.key, and a sibling-free clone of
# ailio-technologies-inc/homebrew-tap (it is cloned to a temp directory here).
set -euo pipefail

VERSION="${1:?usage: scripts/release.sh <version>}"
REPO=ailio-technologies-inc/dagpeek
TAP=ailio-technologies-inc/homebrew-tap
TARGET=universal-apple-darwin
BUNDLE="target/$TARGET/release/bundle"
OUT="target/release-assets"

fail() { echo "release: $1" >&2; exit 1; }

check_clean() {
  [ -z "$(git status --porcelain)" ] || fail "the working tree is dirty; commit first"
  grep -q "\"version\": \"$VERSION\"" src-tauri/tauri.conf.json || fail "src-tauri/tauri.conf.json is not at $VERSION"
  grep -q "^version = \"$VERSION\"" Cargo.toml || fail "Cargo.toml is not at $VERSION"
}

# The README's image comes from the real UI, so it changes when the UI does.
update_screenshot() {
  scripts/screenshot.sh || fail "could not render the README screenshot"
  git add docs/screenshot.png
  if ! git diff --cached --quiet -- docs/screenshot.png; then
    git commit -q -m "The README screenshot matches the UI." -- docs/screenshot.png
    git push -q origin HEAD
  fi
}

run_tests() {
  cargo test --workspace || fail "tests failed"
}

build_cli() {
  cargo build --release -p dagpeek --target aarch64-apple-darwin || fail "CLI build failed (arm64)"
  cargo build --release -p dagpeek --target x86_64-apple-darwin || fail "CLI build failed (x86_64)"
  mkdir -p target/sidecar
  # Tauri builds each architecture, then the universal app, and wants the command for all three.
  cp target/aarch64-apple-darwin/release/dagpeek target/sidecar/dagpeek-aarch64-apple-darwin
  cp target/x86_64-apple-darwin/release/dagpeek target/sidecar/dagpeek-x86_64-apple-darwin
  lipo -create -output "target/sidecar/dagpeek-$TARGET" \
    target/aarch64-apple-darwin/release/dagpeek target/x86_64-apple-darwin/release/dagpeek
}

build_app() {
  # A disk image left mounted from an earlier build makes the dmg step fail, or write through its
  # Applications shortcut into the real /Applications.
  for volume in /Volumes/Dagpeek* /Volumes/dmg.*; do
    [ -d "$volume" ] && hdiutil detach -force "$volume" || true
  done
  export APPLE_SIGNING_IDENTITY="Developer ID Application: Luigi Poole (N7TM93NL4Q)"
  # APPLE_API_ISSUER and APPLE_API_KEY, kept out of the repo.
  # shellcheck disable=SC1091
  source "$HOME/.appstoreconnect/dagpeek.env" || fail "no ~/.appstoreconnect/dagpeek.env"
  export APPLE_API_ISSUER APPLE_API_KEY
  # Compiled in for the daily usage count. A build without it sends nothing.
  if [ -n "${DAGPEEK_GA_API_SECRET:-}" ]; then export DAGPEEK_GA_API_SECRET; fi
  export APPLE_API_KEY_PATH="$HOME/.appstoreconnect/private_keys/AuthKey_$APPLE_API_KEY.p8"
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/dagpeek.key")"
  export TAURI_SIGNING_PRIVATE_KEY
  pnpm exec tauri build --target "$TARGET" --bundles app,dmg || fail "app build, signing or notarization failed"
}

# Unsigned for now, so Windows SmartScreen warns about an unknown publisher.
build_windows() {
  PATH="$(brew --prefix llvm)/bin:$HOME/.cargo/bin:$PATH"
  cargo xwin build --release -p dagpeek --target x86_64-pc-windows-msvc || fail "Windows CLI build failed"
  cp target/x86_64-pc-windows-msvc/release/dagpeek.exe target/sidecar/dagpeek-x86_64-pc-windows-msvc.exe
  pnpm exec tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis || fail "Windows build failed"
  WIN_EXE="target/x86_64-pc-windows-msvc/release/bundle/nsis/Dagpeek_${VERSION}_x64-setup.exe"
  [ -f "$WIN_EXE" ] && [ -f "$WIN_EXE.sig" ] || fail "no Windows installer or signature for $VERSION"
}

staple_dmg() {
  DMG="$BUNDLE/dmg/Dagpeek_${VERSION}_universal.dmg"
  [ -f "$DMG" ] || fail "no dmg for $VERSION at $DMG"
  codesign --force --sign "Developer ID Application: Luigi Poole (N7TM93NL4Q)" --timestamp "$DMG"
  local result
  result=$(xcrun notarytool submit "$DMG" --keychain-profile dagpeek --wait) || fail "could not submit the dmg to Apple"
  echo "$result"
  grep -q 'status: Accepted' <<<"$result" || fail "Apple did not accept the dmg"
  xcrun stapler staple "$DMG" || fail "could not staple the dmg"
}

verify() {
  local app="$BUNDLE/macos/Dagpeek.app"
  codesign --verify --deep --strict "$app" || fail "the app's signature does not verify"
  spctl -a -vv "$app" || fail "Gatekeeper rejects the app"
  xcrun stapler validate "$app" || fail "the app has no notarization ticket"
  spctl -a -t open --context context:primary-signature -vv "$DMG" || fail "Gatekeeper rejects the dmg"
  "$app/Contents/MacOS/dagpeek" nonsense 2>&1 | grep -q usage || true
}

collect() {
  rm -rf "$OUT"; mkdir -p "$OUT"
  cp "$DMG" "$OUT/Dagpeek-mac.dmg"
  cp "$BUNDLE/macos/Dagpeek.app.tar.gz" "$OUT/Dagpeek-mac.app.tar.gz"
  cp "$WIN_EXE" "$OUT/Dagpeek-windows-setup.exe"
  WIN_SIG=$(cat "$WIN_EXE.sig")
  WIN_URL="https://github.com/$REPO/releases/download/v$VERSION/Dagpeek-windows-setup.exe"
  SIG=$(cat "$BUNDLE/macos/Dagpeek.app.tar.gz.sig")
  SHA=$(shasum -a 256 "$OUT/Dagpeek-mac.dmg" | cut -d' ' -f1)
  URL="https://github.com/$REPO/releases/download/v$VERSION/Dagpeek-mac.app.tar.gz"
  jq -n --arg v "$VERSION" --arg d "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg s "$SIG" --arg u "$URL" --arg ws "$WIN_SIG" --arg wu "$WIN_URL" \
    '{version:$v, pub_date:$d, platforms:{"darwin-aarch64":{signature:$s,url:$u},"darwin-x86_64":{signature:$s,url:$u},"windows-x86_64":{signature:$ws,url:$wu}}}' \
    > "$OUT/latest.json"
  (cd "$OUT" && shasum -a 256 Dagpeek-mac.dmg Dagpeek-mac.app.tar.gz Dagpeek-windows-setup.exe > SHA256SUMS.txt)
}

publish() {
  git tag -f "v$VERSION" && git push -f origin "v$VERSION"
  gh release create "v$VERSION" --repo "$REPO" --title "Dagpeek $VERSION" \
    --notes "Download the app from https://dagpeek.com or install it with Homebrew." "$OUT"/* \
    || fail "could not create the GitHub release"
}

update_cask() {
  local dir; dir=$(mktemp -d)
  gh repo clone "$TAP" "$dir" -- -q
  mkdir -p "$dir/Casks"
  sed -e "s/@VERSION@/$VERSION/" -e "s/@SHA256@/$SHA/" scripts/dagpeek.cask.rb > "$dir/Casks/dagpeek.rb"
  (cd "$dir" && git add Casks/dagpeek.rb && git commit -q -m "dagpeek $VERSION" && git push -q origin HEAD) \
    || fail "could not update the Homebrew tap"
  rm -rf "$dir"
}

update_screenshot
check_clean
run_tests
build_cli
build_app
staple_dmg
verify
build_windows
collect
publish
update_cask
echo "released $VERSION"
