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

run_tests() {
  cargo test --workspace || fail "tests failed"
}

build_cli() {
  cargo build --release -p dagpeek --target aarch64-apple-darwin || fail "CLI build failed (arm64)"
  cargo build --release -p dagpeek --target x86_64-apple-darwin || fail "CLI build failed (x86_64)"
  mkdir -p target/sidecar
  lipo -create -output "target/sidecar/dagpeek-$TARGET" \
    target/aarch64-apple-darwin/release/dagpeek target/x86_64-apple-darwin/release/dagpeek
}

build_app() {
  export APPLE_SIGNING_IDENTITY="Developer ID Application: Luigi Poole (N7TM93NL4Q)"
  export APPLE_API_ISSUER=85a4b222-6268-4476-b98b-0f29410d87fc APPLE_API_KEY=XZ63X96724
  export APPLE_API_KEY_PATH="$HOME/.appstoreconnect/private_keys/AuthKey_$APPLE_API_KEY.p8"
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/dagpeek.key")"
  export TAURI_SIGNING_PRIVATE_KEY
  pnpm exec tauri build --target "$TARGET" --bundles app,dmg || fail "app build, signing or notarization failed"
}

staple_dmg() {
  DMG=$(ls "$BUNDLE"/dmg/*.dmg | head -1)
  codesign --force --sign "Developer ID Application: Luigi Poole (N7TM93NL4Q)" --timestamp "$DMG"
  xcrun notarytool submit "$DMG" --keychain-profile dagpeek --wait | tee /dev/stderr | grep -q 'status: Accepted' \
    || fail "Apple did not accept the dmg"
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
  SIG=$(cat "$BUNDLE/macos/Dagpeek.app.tar.gz.sig")
  SHA=$(shasum -a 256 "$OUT/Dagpeek-mac.dmg" | cut -d' ' -f1)
  URL="https://github.com/$REPO/releases/download/v$VERSION/Dagpeek-mac.app.tar.gz"
  jq -n --arg v "$VERSION" --arg d "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg s "$SIG" --arg u "$URL" \
    '{version:$v, pub_date:$d, platforms:{"darwin-aarch64":{signature:$s,url:$u},"darwin-x86_64":{signature:$s,url:$u}}}' \
    > "$OUT/latest.json"
  (cd "$OUT" && shasum -a 256 Dagpeek-mac.dmg Dagpeek-mac.app.tar.gz > SHA256SUMS.txt)
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

check_clean
run_tests
build_cli
build_app
staple_dmg
verify
collect
publish
update_cask
echo "released $VERSION"
