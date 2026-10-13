#!/usr/bin/env bash
# Assembles Gezik.app from a built binary and signs it ad hoc (no Developer ID yet).
#   scripts/macos/make-app.sh [binary] [version] [out folder]
# Defaults: target/release/gezik, the workspace version in Cargo.toml, target/release.
# The Info.plist is crates/gezik/macos/Info.plist (the one build.rs embeds in the binary) plus
# the version and, as spec 15 §10.2 asks, the languages AppKit's own menu items may use.
# There is no app icon in the repo yet, so the bundle has none (Finder shows the generic one).
set -eu
cd "$(dirname "$0")/../.."
BIN=${1:-target/release/gezik}
VERSION=${2:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
OUT=${3:-target/release}
APP=$OUT/Gezik.app
PB=/usr/libexec/PlistBuddy

[ -x "$BIN" ] || { echo "no binary at $BIN (cargo build --release -p gezik)" >&2; exit 1; }
[ -n "$VERSION" ] || { echo "no version" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
install -m 0755 "$BIN" "$APP/Contents/MacOS/gezik"
PLIST=$APP/Contents/Info.plist
cp crates/gezik/macos/Info.plist "$PLIST"
$PB -c "Add :CFBundleShortVersionString string $VERSION" \
    -c "Add :CFBundleVersion string $VERSION" \
    -c "Add :CFBundleInfoDictionaryVersion string 6.0" \
    -c "Add :CFBundleDevelopmentRegion string en" \
    -c "Add :CFBundleLocalizations array" \
    -c "Add :CFBundleLocalizations: string en" \
    -c "Add :CFBundleLocalizations: string tr" \
    -c "Add :CFBundleLocalizations: string de" \
    -c "Add :CFBundleLocalizations: string es" \
    -c "Add :CFBundleLocalizations: string fr" \
    -c "Add :CFBundleLocalizations: string zh-Hans" \
    "$PLIST"
printf 'APPL????' > "$APP/Contents/PkgInfo"

codesign --force --sign - "$APP"

# Self-check: a valid plist with the version, a signature that verifies.
plutil -lint "$PLIST" >/dev/null
[ "$($PB -c 'Print :CFBundleShortVersionString' "$PLIST")" = "$VERSION" ]
codesign --verify --strict "$APP"
echo "$APP ($VERSION, $(lipo -archs "$APP/Contents/MacOS/gezik"))"
